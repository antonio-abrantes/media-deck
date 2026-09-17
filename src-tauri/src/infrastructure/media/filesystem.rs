//! Safe filesystem writer for `GAME.INI`.
//!
//! The protocol is recoverable through `GAME.BAK`; it does not claim that a
//! multi-step FAT/filesystem replacement is a single crash-atomic operation.

use crate::domain::entities::ContentHash;
use crate::domain::errors::DomainError;
use crate::domain::ids::IdGenerator;
use crate::domain::media_profile::{
    parse_profile, write_canonical_v2, MediaProfile, MAX_PROFILE_BYTES,
};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileFile {
    Ini,
    Temporary,
    Backup,
}

impl ProfileFile {
    fn name(self) -> &'static str {
        match self {
            ProfileFile::Ini => "GAME.INI",
            ProfileFile::Temporary => "GAME.TMP",
            ProfileFile::Backup => "GAME.BAK",
        }
    }
}

pub trait ProfileStorage: Send {
    fn exists(&self, file: ProfileFile) -> io::Result<bool>;
    fn read_limited(&self, file: ProfileFile, max_bytes: usize) -> io::Result<Vec<u8>>;
    fn write_and_flush(&mut self, file: ProfileFile, bytes: &[u8]) -> io::Result<()>;
    fn rename(&mut self, from: ProfileFile, to: ProfileFile) -> io::Result<()>;
    fn remove(&mut self, file: ProfileFile) -> io::Result<()>;
}

#[derive(Debug)]
pub struct LocalProfileStorage {
    root: PathBuf,
}

impl LocalProfileStorage {
    /// The caller must also revalidate drive identity and type immediately
    /// before writing; that device-level check is implemented in Phase 4.
    pub(crate) fn from_validated_root(root: impl Into<PathBuf>) -> Result<Self, DomainError> {
        let root = root.into();
        let is_unc = root.to_string_lossy().starts_with(r"\\");
        let is_symlink = std::fs::symlink_metadata(&root)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true);
        if !root.is_absolute() || !root.is_dir() || is_unc || is_symlink {
            return Err(DomainError::MediaDeviceNotAllowed);
        }
        Ok(Self { root })
    }

    fn path(&self, file: ProfileFile) -> PathBuf {
        self.root.join(file.name())
    }
}

impl ProfileStorage for LocalProfileStorage {
    fn exists(&self, file: ProfileFile) -> io::Result<bool> {
        self.path(file).try_exists()
    }

    fn read_limited(&self, file: ProfileFile, max_bytes: usize) -> io::Result<Vec<u8>> {
        let handle = File::open(self.path(file))?;
        let mut bytes = Vec::with_capacity(max_bytes.min(4096));
        handle
            .take(u64::try_from(max_bytes).unwrap_or(u64::MAX) + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "GAME.INI exceeds maximum size",
            ));
        }
        Ok(bytes)
    }

    fn write_and_flush(&mut self, file: ProfileFile, bytes: &[u8]) -> io::Result<()> {
        let mut handle = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(self.path(file))?;
        handle.write_all(bytes)?;
        handle.flush()?;
        handle.sync_all()
    }

    fn rename(&mut self, from: ProfileFile, to: ProfileFile) -> io::Result<()> {
        std::fs::rename(self.path(from), self.path(to))
    }

    fn remove(&mut self, file: ProfileFile) -> io::Result<()> {
        match std::fs::remove_file(self.path(file)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    pub bytes_written: usize,
    pub backup_created: bool,
    pub content_hash: ContentHash,
}

pub struct AtomicProfileWriter<S> {
    storage: S,
}

impl<S: ProfileStorage> AtomicProfileWriter<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn write_verified(
        &mut self,
        profile: &MediaProfile,
        id_generator: &dyn IdGenerator,
    ) -> Result<WriteReceipt, DomainError> {
        let canonical =
            write_canonical_v2(profile).map_err(|_| DomainError::MediaProfileInvalid)?;
        let expected = parse_profile(canonical.as_bytes(), id_generator)
            .map_err(|_| DomainError::MediaProfileInvalid)?
            .profile;
        let content_hash = ContentHash::new(format!("{:x}", Sha256::digest(canonical.as_bytes())))?;

        self.storage
            .write_and_flush(ProfileFile::Temporary, canonical.as_bytes())
            .map_err(map_write_error)?;

        let had_previous = match self.storage.exists(ProfileFile::Ini) {
            Ok(exists) => exists,
            Err(error) => {
                self.cleanup_temporary();
                return Err(map_io_error(error));
            }
        };
        if had_previous {
            if let Err(error) = self.storage.remove(ProfileFile::Backup) {
                self.cleanup_temporary();
                return Err(map_write_error(error));
            }
            if let Err(error) = self.storage.rename(ProfileFile::Ini, ProfileFile::Backup) {
                self.cleanup_temporary();
                return Err(map_write_error(error));
            }
        }

        if let Err(error) = self
            .storage
            .rename(ProfileFile::Temporary, ProfileFile::Ini)
        {
            self.restore_previous(had_previous);
            return Err(map_write_error(error));
        }

        let verified = self
            .storage
            .read_limited(ProfileFile::Ini, MAX_PROFILE_BYTES)
            .map_err(map_io_error)
            .and_then(|bytes| {
                parse_profile(&bytes, id_generator)
                    .map(|parsed| parsed.profile)
                    .map_err(|_| DomainError::MediaVerificationFailed)
            });

        match verified {
            Ok(actual) if actual == expected => Ok(WriteReceipt {
                bytes_written: canonical.len(),
                backup_created: had_previous,
                content_hash,
            }),
            Ok(_) | Err(DomainError::MediaVerificationFailed) => {
                self.restore_previous(had_previous);
                Err(DomainError::MediaVerificationFailed)
            }
            Err(error) => {
                self.restore_previous(had_previous);
                Err(error)
            }
        }
    }

    fn restore_previous(&mut self, had_previous: bool) {
        self.cleanup_temporary();
        if let Err(error) = self.storage.remove(ProfileFile::Ini) {
            log_rollback_failure("remove_unverified_profile", &error);
        }
        if had_previous {
            if let Err(error) = self.storage.rename(ProfileFile::Backup, ProfileFile::Ini) {
                log_rollback_failure("restore_backup", &error);
            }
        }
    }

    fn cleanup_temporary(&mut self) {
        if let Err(error) = self.storage.remove(ProfileFile::Temporary) {
            log_rollback_failure("remove_temporary", &error);
        }
    }
}

pub fn read_profile_bytes(root: &Path) -> Result<Vec<u8>, DomainError> {
    LocalProfileStorage::from_validated_root(root)?
        .read_limited(ProfileFile::Ini, MAX_PROFILE_BYTES)
        .map_err(map_initial_read_error)
}

fn log_rollback_failure(operation: &'static str, error: &io::Error) {
    tracing::error!(
        operation,
        error_kind = ?error.kind(),
        "GAME.INI rollback operation failed"
    );
}

fn map_write_error(error: io::Error) -> DomainError {
    match error.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::WriteZero => {
            DomainError::MediaWriteProtected
        }
        io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof => DomainError::MediaChanged,
        _ => DomainError::MediaIoFailed,
    }
}

fn map_io_error(error: io::Error) -> DomainError {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof => DomainError::MediaChanged,
        io::ErrorKind::PermissionDenied => DomainError::MediaWriteProtected,
        io::ErrorKind::InvalidData => DomainError::MediaProfileInvalid,
        _ => DomainError::MediaIoFailed,
    }
}

fn map_initial_read_error(error: io::Error) -> DomainError {
    match error.kind() {
        io::ErrorKind::NotFound => DomainError::MediaProfileMissing,
        _ => map_io_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{MediaDeckId, SystemIdGenerator};
    use crate::domain::media_profile::parse_profile;
    use std::collections::HashMap;
    use std::str::FromStr;
    use tempfile::tempdir;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Fault {
        None,
        WriteProtected,
        RemoveBeforeCommit,
        CorruptAfterCommit,
    }

    struct MemoryStorage {
        files: HashMap<ProfileFile, Vec<u8>>,
        fault: Fault,
    }

    impl MemoryStorage {
        fn with_ini(bytes: Vec<u8>) -> Self {
            Self {
                files: HashMap::from([(ProfileFile::Ini, bytes)]),
                fault: Fault::None,
            }
        }
    }

    impl ProfileStorage for MemoryStorage {
        fn exists(&self, file: ProfileFile) -> io::Result<bool> {
            Ok(self.files.contains_key(&file))
        }

        fn read_limited(&self, file: ProfileFile, max_bytes: usize) -> io::Result<Vec<u8>> {
            let bytes = self
                .files
                .get(&file)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            if bytes.len() > max_bytes {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            Ok(bytes)
        }

        fn write_and_flush(&mut self, file: ProfileFile, bytes: &[u8]) -> io::Result<()> {
            if self.fault == Fault::WriteProtected {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            self.files.insert(file, bytes.to_vec());
            Ok(())
        }

        fn rename(&mut self, from: ProfileFile, to: ProfileFile) -> io::Result<()> {
            if from == ProfileFile::Temporary
                && to == ProfileFile::Ini
                && self.fault == Fault::RemoveBeforeCommit
            {
                self.files.clear();
                return Err(io::Error::from(io::ErrorKind::NotFound));
            }
            let mut bytes = self
                .files
                .remove(&from)
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            if from == ProfileFile::Temporary
                && to == ProfileFile::Ini
                && self.fault == Fault::CorruptAfterCommit
            {
                bytes.extend_from_slice(b"\nCOMMAND=bad");
            }
            self.files.insert(to, bytes);
            Ok(())
        }

        fn remove(&mut self, file: ProfileFile) -> io::Result<()> {
            self.files.remove(&file);
            Ok(())
        }
    }

    struct FixedIdGenerator;

    impl IdGenerator for FixedIdGenerator {
        fn next_id(&self) -> MediaDeckId {
            MediaDeckId::from_str("01994a56-69d7-7ef4-a137-94808fa24131").unwrap()
        }
    }

    fn profile() -> MediaProfile {
        parse_profile(
            b"[MEDIA]\nSCHEMA=2\nMEDIA_ID=GAME-1\nPROFILE_ID=01994a56-69d7-7ef4-a137-94808fa24131\n[GAME]\nPROVIDER=steam\nAPP_ID=10\nDISPLAY_NAME=Game\n",
            &FixedIdGenerator,
        )
        .unwrap()
        .profile
    }

    #[test]
    fn local_storage_writes_flushes_and_creates_backup() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("GAME.INI"), b"old profile").unwrap();
        let storage = LocalProfileStorage::from_validated_root(directory.path()).unwrap();
        let mut writer = AtomicProfileWriter::new(storage);

        let receipt = writer
            .write_verified(&profile(), &SystemIdGenerator)
            .unwrap();
        assert!(receipt.backup_created);
        assert_eq!(receipt.content_hash.as_str().len(), 64);
        assert_eq!(
            std::fs::read(directory.path().join("GAME.BAK")).unwrap(),
            b"old profile"
        );
        assert!(directory.path().join("GAME.INI").exists());
        assert!(!directory.path().join("GAME.TMP").exists());
    }

    #[test]
    fn missing_profile_has_specific_error() {
        let directory = tempdir().unwrap();
        assert_eq!(
            read_profile_bytes(directory.path()),
            Err(DomainError::MediaProfileMissing)
        );
    }

    #[test]
    fn local_storage_rejects_unvalidated_roots() {
        assert!(matches!(
            LocalProfileStorage::from_validated_root("relative"),
            Err(DomainError::MediaDeviceNotAllowed)
        ));
        assert!(matches!(
            LocalProfileStorage::from_validated_root(r"\\server\share"),
            Err(DomainError::MediaDeviceNotAllowed)
        ));
    }

    #[test]
    fn write_protection_preserves_existing_profile() {
        let mut storage = MemoryStorage::with_ini(b"old".to_vec());
        storage.fault = Fault::WriteProtected;
        let mut writer = AtomicProfileWriter::new(storage);

        assert_eq!(
            writer.write_verified(&profile(), &FixedIdGenerator),
            Err(DomainError::MediaWriteProtected)
        );
        assert_eq!(
            writer.storage().files.get(&ProfileFile::Ini),
            Some(&b"old".to_vec())
        );
    }

    #[test]
    fn removal_during_commit_never_reports_success() {
        let mut storage = MemoryStorage::with_ini(b"old".to_vec());
        storage.fault = Fault::RemoveBeforeCommit;
        let mut writer = AtomicProfileWriter::new(storage);

        assert_eq!(
            writer.write_verified(&profile(), &FixedIdGenerator),
            Err(DomainError::MediaChanged)
        );
    }

    #[test]
    fn failed_verification_restores_previous_profile() {
        let mut storage = MemoryStorage::with_ini(b"old".to_vec());
        storage.fault = Fault::CorruptAfterCommit;
        let mut writer = AtomicProfileWriter::new(storage);

        assert_eq!(
            writer.write_verified(&profile(), &FixedIdGenerator),
            Err(DomainError::MediaVerificationFailed)
        );
        assert_eq!(
            writer.storage().files.get(&ProfileFile::Ini),
            Some(&b"old".to_vec())
        );
        assert!(!writer.storage().files.contains_key(&ProfileFile::Backup));
    }
}
