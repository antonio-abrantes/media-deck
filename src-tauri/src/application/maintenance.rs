use crate::domain::errors::DomainError;
use crate::support::dirs::AppDirs;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const MAX_DIAGNOSTIC_BYTES: usize = 512 * 1024;
const MAX_BACKUP_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BACKUP_FILES: usize = 20_000;

pub fn apply_pending_restore(dirs: &AppDirs) -> Result<bool, DomainError> {
    let marker = dirs.temp.join("restore.pending");
    if !marker.is_file() {
        return Ok(false);
    }
    let stage = dirs.temp.join("restore-pending");
    verify_staged_backup(&stage)?;
    let safety = dirs.backups.join(format!(
        "pre-restore-{}",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::create_dir(&safety).map_err(io_error)?;
    if dirs.database.is_file() {
        fs::copy(&dirs.database, safety.join("media-deck.db")).map_err(io_error)?;
    }
    copy_tree(&dirs.artwork, &safety.join("artwork"))?;
    copy_tree(&dirs.labels, &safety.join("labels"))?;

    fs::copy(stage.join("media-deck.db"), &dirs.database).map_err(io_error)?;
    remove_real_tree(&dirs.artwork)?;
    remove_real_tree(&dirs.labels)?;
    fs::rename(stage.join("artwork"), &dirs.artwork)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                fs::create_dir(&dirs.artwork)
            } else {
                Err(error)
            }
        })
        .map_err(io_error)?;
    fs::rename(stage.join("labels"), &dirs.labels)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                fs::create_dir(&dirs.labels)
            } else {
                Err(error)
            }
        })
        .map_err(io_error)?;
    fs::remove_file(marker).map_err(io_error)?;
    remove_real_tree(&stage)?;
    Ok(true)
}

#[derive(Clone)]
pub struct MaintenanceService {
    pool: SqlitePool,
    dirs: Arc<AppDirs>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceReceipt {
    pub output_path: String,
    pub files: usize,
    pub bytes: u64,
    pub restart_required: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupManifest {
    format: u32,
    app_version: String,
    created_at: String,
    files: Vec<BackupFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupFile {
    path: String,
    bytes: u64,
    sha256: String,
}

impl MaintenanceService {
    pub fn new(pool: SqlitePool, dirs: Arc<AppDirs>) -> Self {
        Self { pool, dirs }
    }

    pub async fn export_diagnostics(
        &self,
        destination: &Path,
    ) -> Result<MaintenanceReceipt, DomainError> {
        validate_destination(destination, "json")?;
        let settings: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value_json FROM settings ORDER BY key")
                .fetch_all(&self.pool)
                .await
                .map_err(db_error)?;
        let devices: Vec<(String, String, String, bool)> = sqlx::query_as(
            "SELECT friendly_name, drive_type, monitor_policy, enabled
             FROM media_devices ORDER BY friendly_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        let recent_logs = recent_sanitized_logs(&self.dirs.logs)?;
        let document = serde_json::json!({
            "format": 1,
            "generated_at": Utc::now().to_rfc3339(),
            "app_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "settings": settings.into_iter().filter(|(key, _)| key != "autostart").collect::<Vec<_>>(),
            "devices": devices,
            "recent_logs": recent_logs,
            "privacy": "User paths, credentials, GAME.INI contents and command arguments are excluded."
        });
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| DomainError::DatabaseOperationFailed(error.to_string()))?;
        write_selected(destination, &bytes)?;
        Ok(MaintenanceReceipt {
            output_path: destination.to_string_lossy().into_owned(),
            files: 1,
            bytes: bytes.len() as u64,
            restart_required: false,
        })
    }

    pub async fn export_backup(
        &self,
        destination: &Path,
    ) -> Result<MaintenanceReceipt, DomainError> {
        validate_destination(destination, "mdbak")?;
        let snapshot = self.dirs.temp.join("backup-database.sqlite");
        let _ = fs::remove_file(&snapshot);
        let escaped = snapshot.to_string_lossy().replace('\'', "''");
        sqlx::query(&format!("VACUUM INTO '{escaped}'"))
            .execute(&self.pool)
            .await
            .map_err(db_error)?;

        let mut sources = vec![(snapshot.clone(), PathBuf::from("media-deck.db"))];
        collect_files(&self.dirs.artwork, Path::new("artwork"), &mut sources)?;
        collect_files(&self.dirs.labels, Path::new("labels"), &mut sources)?;
        if sources.len() > MAX_BACKUP_FILES {
            return Err(DomainError::DatabaseOperationFailed(
                "backup contains too many files".into(),
            ));
        }
        let mut manifest_files = Vec::with_capacity(sources.len());
        let mut total = 0u64;
        for (source, archive_path) in &sources {
            let metadata = fs::metadata(source).map_err(io_error)?;
            total = total.saturating_add(metadata.len());
            if total > MAX_BACKUP_BYTES {
                return Err(DomainError::DatabaseOperationFailed(
                    "backup exceeds size limit".into(),
                ));
            }
            manifest_files.push(BackupFile {
                path: slash_path(archive_path)?,
                bytes: metadata.len(),
                sha256: hash_file(source)?,
            });
        }
        let manifest = BackupManifest {
            format: 1,
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at: Utc::now().to_rfc3339(),
            files: manifest_files,
        };
        let file = selected_file(destination)?;
        let mut archive = tar::Builder::new(file);
        for (source, archive_path) in &sources {
            archive
                .append_path_with_name(source, archive_path)
                .map_err(io_error)?;
        }
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| DomainError::DatabaseOperationFailed(error.to_string()))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest_bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(&mut header, "manifest.json", manifest_bytes.as_slice())
            .map_err(io_error)?;
        archive.finish().map_err(io_error)?;
        let _ = fs::remove_file(snapshot);
        Ok(MaintenanceReceipt {
            output_path: destination.to_string_lossy().into_owned(),
            files: sources.len(),
            bytes: total,
            restart_required: false,
        })
    }

    pub fn stage_restore(&self, source: &Path) -> Result<MaintenanceReceipt, DomainError> {
        validate_source(source, "mdbak")?;
        let stage = self.dirs.temp.join("restore-pending");
        remove_real_tree(&stage)?;
        fs::create_dir(&stage).map_err(io_error)?;
        let mut archive = tar::Archive::new(File::open(source).map_err(io_error)?);
        let mut files = 0usize;
        let mut total = 0u64;
        for entry in archive.entries().map_err(io_error)? {
            let mut entry = entry.map_err(io_error)?;
            let path = entry.path().map_err(io_error)?.into_owned();
            validate_archive_path(&path)?;
            if !entry.header().entry_type().is_file() {
                return Err(DomainError::DatabaseOperationFailed(
                    "backup contains unsupported entry".into(),
                ));
            }
            files += 1;
            total = total.saturating_add(entry.size());
            if files > MAX_BACKUP_FILES || total > MAX_BACKUP_BYTES {
                return Err(DomainError::DatabaseOperationFailed(
                    "backup limits exceeded".into(),
                ));
            }
            let target = stage.join(&path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            entry.unpack(&target).map_err(io_error)?;
        }
        verify_staged_backup(&stage)?;
        fs::write(self.dirs.temp.join("restore.pending"), b"1").map_err(io_error)?;
        Ok(MaintenanceReceipt {
            output_path: source.to_string_lossy().into_owned(),
            files,
            bytes: total,
            restart_required: true,
        })
    }
}

fn recent_sanitized_logs(log_dir: &Path) -> Result<Vec<String>, DomainError> {
    let mut paths = fs::read_dir(log_dir)
        .map_err(io_error)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let mut output = Vec::new();
    if let Some(path) = paths.last() {
        let mut text = String::new();
        File::open(path)
            .map_err(io_error)?
            .take(MAX_DIAGNOSTIC_BYTES as u64)
            .read_to_string(&mut text)
            .map_err(io_error)?;
        let username = std::env::var("USERNAME").unwrap_or_default();
        let lines = text.lines().collect::<Vec<_>>();
        for line in lines.iter().skip(lines.len().saturating_sub(200)) {
            let lowercase = line.to_ascii_lowercase();
            if ["token", "password", "api_key", "authorization"]
                .iter()
                .any(|secret| lowercase.contains(secret))
            {
                continue;
            }
            output.push(if username.is_empty() {
                (*line).to_owned()
            } else {
                line.replace(&username, "<user>")
            });
        }
    }
    Ok(output)
}

fn collect_files(
    root: &Path,
    archive_root: &Path,
    output: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), DomainError> {
    for entry in fs::read_dir(root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let metadata = entry.file_type().map_err(io_error)?;
        if metadata.is_symlink() {
            return Err(DomainError::DatabaseOperationFailed(
                "backup source contains symlink".into(),
            ));
        }
        let archive_path = archive_root.join(entry.file_name());
        if metadata.is_dir() {
            collect_files(&entry.path(), &archive_path, output)?;
        } else if metadata.is_file() {
            output.push((entry.path(), archive_path));
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), DomainError> {
    fs::create_dir_all(destination).map_err(io_error)?;
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let kind = entry.file_type().map_err(io_error)?;
        if kind.is_symlink() {
            return Err(DomainError::DatabaseOperationFailed(
                "backup source contains symlink".into(),
            ));
        }
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target).map_err(io_error)?;
        }
    }
    Ok(())
}

fn verify_staged_backup(stage: &Path) -> Result<(), DomainError> {
    let manifest: BackupManifest =
        serde_json::from_slice(&fs::read(stage.join("manifest.json")).map_err(io_error)?)
            .map_err(|_| DomainError::DatabaseOperationFailed("invalid backup manifest".into()))?;
    if manifest.format != 1 || !stage.join("media-deck.db").is_file() {
        return Err(DomainError::DatabaseOperationFailed(
            "unsupported backup".into(),
        ));
    }
    for expected in manifest.files {
        let relative = PathBuf::from(&expected.path);
        validate_archive_path(&relative)?;
        let file = stage.join(relative);
        let metadata = fs::metadata(&file).map_err(io_error)?;
        if metadata.len() != expected.bytes || hash_file(&file)? != expected.sha256 {
            return Err(DomainError::DatabaseOperationFailed(
                "backup checksum mismatch".into(),
            ));
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<(), DomainError> {
    let allowed = path == Path::new("media-deck.db")
        || path == Path::new("manifest.json")
        || path.starts_with("artwork")
        || path.starts_with("labels");
    if !allowed
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(DomainError::DatabaseOperationFailed(
            "unsafe backup path".into(),
        ));
    }
    Ok(())
}

fn validate_destination(path: &Path, extension: &str) -> Result<(), DomainError> {
    let parent_is_real = path
        .parent()
        .and_then(|parent| fs::symlink_metadata(parent).ok())
        .is_some_and(|metadata| metadata.is_dir() && !is_reparse_point(&metadata));
    let target_is_safe = fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
        .unwrap_or(true);
    if !path.is_absolute()
        || path.to_string_lossy().starts_with(r"\\")
        || path.extension().and_then(|value| value.to_str()) != Some(extension)
        || !parent_is_real
        || !target_is_safe
    {
        return Err(DomainError::DatabaseOperationFailed(
            "invalid output path".into(),
        ));
    }
    Ok(())
}

fn validate_source(path: &Path, extension: &str) -> Result<(), DomainError> {
    let source_is_real = fs::symlink_metadata(path)
        .map(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
        .unwrap_or(false);
    if !path.is_absolute()
        || path.to_string_lossy().starts_with(r"\\")
        || path.extension().and_then(|value| value.to_str()) != Some(extension)
        || !source_is_real
    {
        return Err(DomainError::DatabaseOperationFailed(
            "invalid backup path".into(),
        ));
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

fn selected_file(path: &Path) -> Result<File, DomainError> {
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(io_error)
}

fn write_selected(path: &Path, bytes: &[u8]) -> Result<(), DomainError> {
    let mut file = selected_file(path)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(io_error)
}

fn hash_file(path: &Path) -> Result<String, DomainError> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash).map_err(io_error)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn slash_path(path: &Path) -> Result<String, DomainError> {
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| DomainError::DatabaseOperationFailed("non-utf8 backup path".into()))
}

fn remove_real_tree(path: &Path) -> Result<(), DomainError> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(DomainError::DatabaseOperationFailed(
            "unsafe restore staging path".into(),
        ));
    }
    fs::remove_dir_all(path).map_err(io_error)
}

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseOperationFailed(error.to_string())
}

fn io_error(error: std::io::Error) -> DomainError {
    DomainError::DatabaseOperationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::open_and_migrate;
    use tempfile::tempdir;

    #[tokio::test]
    async fn backup_is_verified_and_staged_for_restart() {
        let root = tempdir().unwrap();
        let dirs = Arc::new(AppDirs::from_root(root.path()).unwrap());
        let pool = open_and_migrate(&dirs.database).await.unwrap();
        fs::write(dirs.artwork.join("manual.png"), b"manual-art").unwrap();
        fs::write(dirs.labels.join("project.png"), b"label").unwrap();
        let service = MaintenanceService::new(pool, dirs.clone());
        let destination = root.path().join("backup.mdbak");

        let exported = service.export_backup(&destination).await.unwrap();
        assert!(exported.files >= 3);
        fs::write(dirs.artwork.join("manual.png"), b"changed").unwrap();
        let staged = service.stage_restore(&destination).unwrap();
        assert!(staged.restart_required);
        assert!(dirs.temp.join("restore.pending").is_file());
        drop(service);
        assert!(apply_pending_restore(&dirs).unwrap());
        assert_eq!(
            fs::read(dirs.artwork.join("manual.png")).unwrap(),
            b"manual-art"
        );
        assert!(!dirs.temp.join("restore.pending").exists());
    }

    #[tokio::test]
    async fn diagnostics_exclude_secret_shaped_log_lines() {
        let root = tempdir().unwrap();
        let dirs = Arc::new(AppDirs::from_root(root.path()).unwrap());
        let pool = open_and_migrate(&dirs.database).await.unwrap();
        fs::write(
            dirs.logs.join("media-deck.log.2026-09-16"),
            "safe event\napi_key=never-export\n",
        )
        .unwrap();
        let service = MaintenanceService::new(pool, dirs);
        let destination = root.path().join("diagnostics.json");
        service.export_diagnostics(&destination).await.unwrap();
        let output = fs::read_to_string(destination).unwrap();
        assert!(output.contains("safe event"));
        assert!(!output.contains("never-export"));
    }
}
