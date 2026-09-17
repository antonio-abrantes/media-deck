//! Application data directories.
//!
//! Resolves and creates all required sub-directories under the MediaDeck
//! data root. Uses `%LOCALAPPDATA%\MediaDeck` on Windows.
//! Falls back to `dirs::data_local_dir()` if the environment variable is absent.

use std::io;
use std::path::{Path, PathBuf};

/// All paths used by MediaDeck, resolved at startup.
#[derive(Debug, Clone)]
pub struct AppDirs {
    /// Root: `%LOCALAPPDATA%\MediaDeck`
    pub root: PathBuf,
    /// `root\media-deck.db`
    pub database: PathBuf,
    /// `root\artwork\`
    pub artwork: PathBuf,
    /// `root\labels\`
    pub labels: PathBuf,
    /// `root\exports\`
    pub exports: PathBuf,
    /// `root\logs\`
    pub logs: PathBuf,
    /// `root\backups\`
    pub backups: PathBuf,
    /// `root\temp\`
    pub temp: PathBuf,
}

impl AppDirs {
    /// Resolve the data root and ensure all sub-directories exist.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if any directory cannot be created.
    pub fn init() -> io::Result<Self> {
        let root = resolve_root()?;
        let dirs = Self {
            database: root.join("media-deck.db"),
            artwork: root.join("artwork"),
            labels: root.join("labels"),
            exports: root.join("exports"),
            logs: root.join("logs"),
            backups: root.join("backups"),
            temp: root.join("temp"),
            root,
        };
        dirs.create_all()?;
        Ok(dirs)
    }

    /// Create all sub-directories (idempotent).
    fn create_all(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.artwork)?;
        std::fs::create_dir_all(&self.labels)?;
        std::fs::create_dir_all(&self.exports)?;
        std::fs::create_dir_all(&self.logs)?;
        std::fs::create_dir_all(&self.backups)?;
        std::fs::create_dir_all(&self.temp)?;
        Ok(())
    }

    /// Construct `AppDirs` from an arbitrary root (tests only).
    #[cfg(test)]
    pub fn from_root(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let dirs = Self {
            database: root.join("media-deck.db"),
            artwork: root.join("artwork"),
            labels: root.join("labels"),
            exports: root.join("exports"),
            logs: root.join("logs"),
            backups: root.join("backups"),
            temp: root.join("temp"),
            root,
        };
        dirs.create_all()?;
        Ok(dirs)
    }
}

/// Resolve the platform data root.
///
/// Priority:
/// 1. `LOCALAPPDATA` env var (Windows standard)
/// 2. `dirs::data_local_dir()` (cross-platform fallback)
fn resolve_root() -> io::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "local data root unavailable"))?;
    validate_local_data_base(&base)?;
    Ok(base.join("MediaDeck"))
}

fn validate_local_data_base(base: &Path) -> io::Result<()> {
    if !base.is_absolute() || base.to_string_lossy().starts_with(r"\\") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local data root must be an absolute local path",
        ));
    }
    let metadata = std::fs::symlink_metadata(base)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local data root must be a real directory",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn from_root_creates_all_subdirs() {
        let dir = tempdir().expect("tempdir");
        let app = AppDirs::from_root(dir.path()).expect("init");

        assert!(app.artwork.exists(), "artwork dir must exist");
        assert!(app.labels.exists(), "labels dir must exist");
        assert!(app.exports.exists(), "exports dir must exist");
        assert!(app.logs.exists(), "logs dir must exist");
        assert!(app.backups.exists(), "backups dir must exist");
        assert!(app.temp.exists(), "temp dir must exist");
    }

    #[test]
    fn from_root_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        AppDirs::from_root(dir.path()).expect("first init");
        AppDirs::from_root(dir.path()).expect("second init must not fail");
    }

    #[test]
    fn database_path_is_inside_root() {
        let dir = tempdir().expect("tempdir");
        let app = AppDirs::from_root(dir.path()).expect("init");
        assert!(app.database.starts_with(&app.root));
        assert_eq!(app.database.file_name().unwrap(), "media-deck.db");
    }

    #[test]
    fn local_data_base_rejects_relative_and_unc_paths() {
        assert!(validate_local_data_base(Path::new("relative")).is_err());
        assert!(validate_local_data_base(Path::new(r"\\server\share")).is_err());
    }
}
