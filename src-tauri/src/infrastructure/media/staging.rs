//! Safe optical-media staging. Names are fixed and every operation receives a
//! fresh directory below the application-owned export root.

use crate::domain::errors::DomainError;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

const MAX_PORTABLE_ARTWORK_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct StagedMedia {
    pub root: PathBuf,
    pub canonical_hash: String,
    pub artwork_included: bool,
}

#[derive(Debug, Clone)]
pub struct StagingWriter {
    root: PathBuf,
}

impl StagingWriter {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, DomainError> {
        let root = root.into();
        ensure_real_directory(&root)?;
        Ok(Self { root })
    }

    pub fn prepare(
        &self,
        operation_id: &str,
        canonical_ini: &str,
        artwork: Option<&Path>,
    ) -> Result<StagedMedia, DomainError> {
        if !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(DomainError::MediaProfileInvalid);
        }
        ensure_real_directory(&self.root)?;
        let directory = self.root.join(operation_id);
        fs::create_dir(&directory).map_err(|_| DomainError::MediaIoFailed)?;
        ensure_real_directory(&directory)?;

        write_new_synced(&directory.join("GAME.INI"), canonical_ini.as_bytes())?;
        let artwork_included = if let Some(source) = artwork {
            copy_artwork(source, &directory)?
        } else {
            false
        };
        sync_directory(&directory)?;
        Ok(StagedMedia {
            root: directory,
            canonical_hash: format!("{:x}", Sha256::digest(canonical_ini.as_bytes())),
            artwork_included,
        })
    }
}

fn copy_artwork(source: &Path, staging: &Path) -> Result<bool, DomainError> {
    let metadata = fs::symlink_metadata(source).map_err(|_| DomainError::ArtworkIoFailed)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_PORTABLE_ARTWORK_BYTES
    {
        return Err(DomainError::ArtworkInvalid);
    }
    let artwork_dir = staging.join("ARTWORK");
    fs::create_dir(&artwork_dir).map_err(|_| DomainError::MediaIoFailed)?;
    ensure_real_directory(&artwork_dir)?;
    let input = File::open(source).map_err(|_| DomainError::ArtworkIoFailed)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    input
        .take(MAX_PORTABLE_ARTWORK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| DomainError::ArtworkIoFailed)?;
    if bytes.len() as u64 > MAX_PORTABLE_ARTWORK_BYTES {
        return Err(DomainError::ArtworkTooLarge);
    }
    let decoded = image::load_from_memory(&bytes).map_err(|_| DomainError::ArtworkInvalid)?;
    let mut portable = Cursor::new(Vec::new());
    decoded
        .write_to(&mut portable, image::ImageFormat::WebP)
        .map_err(|_| DomainError::ArtworkInvalid)?;
    write_new_synced(&artwork_dir.join("COVER.WEBP"), portable.get_ref())?;
    sync_directory(&artwork_dir)?;
    Ok(true)
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), DomainError> {
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| DomainError::MediaIoFailed)?;
    output
        .write_all(bytes)
        .and_then(|()| output.sync_all())
        .map_err(|_| DomainError::MediaIoFailed)
}

fn ensure_real_directory(path: &Path) -> Result<(), DomainError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DomainError::MediaIoFailed)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(DomainError::MediaIoFailed);
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn sync_directory(path: &Path) -> Result<(), DomainError> {
    #[cfg(windows)]
    {
        ensure_real_directory(path)
    }
    #[cfg(not(windows))]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| DomainError::MediaIoFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stages_only_fixed_names_and_canonical_bytes() {
        let temporary = tempdir().unwrap();
        let writer = StagingWriter::new(temporary.path()).unwrap();
        let staged = writer
            .prepare(
                "01994a56-69d7-7ef4-a137-94808fa24131",
                "[MEDIA]\r\nSCHEMA=2\r\n",
                None,
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(staged.root.join("GAME.INI")).unwrap(),
            "[MEDIA]\r\nSCHEMA=2\r\n"
        );
        assert!(!staged.artwork_included);
    }

    #[test]
    fn artwork_always_uses_portable_fixed_path() {
        let temporary = tempdir().unwrap();
        let source = temporary.path().join("validated-cache.webp");
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::new(2, 2));
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        fs::write(&source, encoded.into_inner()).unwrap();
        let exports = temporary.path().join("exports");
        fs::create_dir(&exports).unwrap();
        let staged = StagingWriter::new(&exports)
            .unwrap()
            .prepare("safe-id", "[MEDIA]\r\n", Some(&source))
            .unwrap();
        let output = fs::read(staged.root.join("ARTWORK").join("COVER.WEBP")).unwrap();
        assert_eq!(
            image::guess_format(&output).unwrap(),
            image::ImageFormat::WebP
        );
    }
}
