//! Safe content-addressed artwork storage.

use crate::domain::artwork::CachedArtwork;
use crate::domain::errors::DomainError;
use crate::domain::ports::{ArtworkBlobStore, PortResult};
use image::{ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod remote;

pub const MAX_ARTWORK_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ARTWORK_DIMENSION: u32 = 8_192;
pub const MAX_ARTWORK_PIXELS: u64 = 40_000_000;
const MAX_DECODED_BYTES: u64 = MAX_ARTWORK_PIXELS * 4;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct LocalArtworkCache {
    root: PathBuf,
}

impl LocalArtworkCache {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, DomainError> {
        let root = root.into();
        ensure_real_directory(&root)?;
        Ok(Self { root })
    }

    fn store_sync(&self, bytes: &[u8], declared_mime: &str) -> Result<CachedArtwork, DomainError> {
        if bytes.is_empty() {
            return Err(DomainError::ArtworkInvalid);
        }
        if bytes.len() > MAX_ARTWORK_BYTES {
            return Err(DomainError::ArtworkTooLarge);
        }
        ensure_real_directory(&self.root)?;

        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| DomainError::ArtworkInvalid)?;
        let format = reader.format().ok_or(DomainError::ArtworkInvalid)?;
        let (mime_type, extension) = allowed_format(format)?;
        if declared_mime.trim().to_ascii_lowercase() != mime_type {
            return Err(DomainError::ArtworkInvalid);
        }

        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_ARTWORK_DIMENSION);
        limits.max_image_height = Some(MAX_ARTWORK_DIMENSION);
        limits.max_alloc = Some(MAX_DECODED_BYTES);
        reader.limits(limits);
        let decoded = reader.decode().map_err(map_image_error)?;
        let width = decoded.width();
        let height = decoded.height();
        if width == 0
            || height == 0
            || width > MAX_ARTWORK_DIMENSION
            || height > MAX_ARTWORK_DIMENSION
            || u64::from(width) * u64::from(height) > MAX_ARTWORK_PIXELS
        {
            return Err(DomainError::ArtworkTooLarge);
        }

        // Re-encoding strips metadata and rejects polyglot trailing content.
        let mut normalized = Cursor::new(Vec::new());
        decoded
            .write_to(&mut normalized, format)
            .map_err(map_image_error)?;
        let normalized = normalized.into_inner();
        if normalized.len() > MAX_ARTWORK_BYTES {
            return Err(DomainError::ArtworkTooLarge);
        }

        let sha256 = format!("{:x}", Sha256::digest(&normalized));
        let prefix = &sha256[..2];
        let relative_path = format!("{prefix}/{sha256}.{extension}");
        let directory = self.root.join(prefix);
        create_real_directory(&directory)?;
        let target = directory.join(format!("{sha256}.{extension}"));

        if target.exists() {
            ensure_regular_file(&target)?;
        } else {
            write_atomic(&directory, &target, &normalized)?;
        }

        Ok(CachedArtwork {
            relative_path,
            sha256,
            width,
            height,
            mime_type: mime_type.to_owned(),
        })
    }

    fn resolve_sync(&self, relative_path: &str) -> Result<String, DomainError> {
        let relative = validate_relative_path(relative_path)?;
        let canonical_root = self
            .root
            .canonicalize()
            .map_err(|_| DomainError::ArtworkIoFailed)?;
        let candidate = self.root.join(relative);
        ensure_regular_file(&candidate)?;
        let canonical_candidate = candidate
            .canonicalize()
            .map_err(|_| DomainError::ArtworkIoFailed)?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(DomainError::ArtworkInvalid);
        }
        canonical_candidate
            .to_str()
            .map(str::to_owned)
            .ok_or(DomainError::ArtworkIoFailed)
    }
}

impl ArtworkBlobStore for LocalArtworkCache {
    fn store<'a>(
        &'a self,
        bytes: &'a [u8],
        declared_mime: &'a str,
    ) -> PortResult<'a, CachedArtwork> {
        Box::pin(async move { self.store_sync(bytes, declared_mime) })
    }

    fn resolve<'a>(&'a self, relative_path: &'a str) -> PortResult<'a, String> {
        Box::pin(async move { self.resolve_sync(relative_path) })
    }
}

fn allowed_format(format: ImageFormat) -> Result<(&'static str, &'static str), DomainError> {
    match format {
        ImageFormat::Png => Ok(("image/png", "png")),
        ImageFormat::Jpeg => Ok(("image/jpeg", "jpg")),
        ImageFormat::WebP => Ok(("image/webp", "webp")),
        _ => Err(DomainError::ArtworkInvalid),
    }
}

fn map_image_error(error: image::ImageError) -> DomainError {
    match error {
        image::ImageError::Limits(_) => DomainError::ArtworkTooLarge,
        _ => DomainError::ArtworkInvalid,
    }
}

fn validate_relative_path(value: &str) -> Result<PathBuf, DomainError> {
    if value.is_empty() || value.contains('\\') || value.chars().any(char::is_control) {
        return Err(DomainError::ArtworkInvalid);
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(DomainError::ArtworkInvalid);
    }
    Ok(path.to_path_buf())
}

fn ensure_real_directory(path: &Path) -> Result<(), DomainError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DomainError::ArtworkIoFailed)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(DomainError::ArtworkIoFailed);
    }
    Ok(())
}

fn create_real_directory(path: &Path) -> Result<(), DomainError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure_real_directory(path)
        }
        Err(_) => Err(DomainError::ArtworkIoFailed),
    }
}

fn ensure_regular_file(path: &Path) -> Result<(), DomainError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DomainError::ArtworkIoFailed)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(DomainError::ArtworkIoFailed);
    }
    Ok(())
}

fn write_atomic(directory: &Path, target: &Path, bytes: &[u8]) -> Result<(), DomainError> {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(".artwork-{}-{sequence}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| DomainError::ArtworkIoFailed)?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| DomainError::ArtworkIoFailed)?;
        match fs::rename(&temporary, target) {
            Ok(()) => Ok(()),
            Err(_) if target.exists() => {
                ensure_regular_file(target)?;
                Ok(())
            }
            Err(_) => Err(DomainError::ArtworkIoFailed),
        }
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage};
    use tempfile::tempdir;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::new(width, height));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[tokio::test]
    async fn stores_normalized_content_by_hash_and_deduplicates() {
        let directory = tempdir().unwrap();
        let cache = LocalArtworkCache::new(directory.path()).unwrap();
        let first = cache.store(&png(32, 48), "image/png").await.unwrap();
        let second = cache.store(&png(32, 48), "image/png").await.unwrap();

        assert_eq!(first, second);
        assert_eq!(first.width, 32);
        assert_eq!(first.height, 48);
        assert!(Path::new(&cache.resolve(&first.relative_path).await.unwrap()).is_file());
    }

    #[tokio::test]
    async fn rejects_mime_mismatch_svg_and_oversized_input() {
        let directory = tempdir().unwrap();
        let cache = LocalArtworkCache::new(directory.path()).unwrap();
        assert_eq!(
            cache.store(&png(1, 1), "image/jpeg").await,
            Err(DomainError::ArtworkInvalid)
        );
        assert_eq!(
            cache
                .store(
                    b"<svg xmlns='http://www.w3.org/2000/svg'/>",
                    "image/svg+xml"
                )
                .await,
            Err(DomainError::ArtworkInvalid)
        );
        assert_eq!(
            cache
                .store(&vec![0; MAX_ARTWORK_BYTES + 1], "image/png")
                .await,
            Err(DomainError::ArtworkTooLarge)
        );
        assert_eq!(
            cache
                .store(&png(MAX_ARTWORK_DIMENSION + 1, 1), "image/png")
                .await,
            Err(DomainError::ArtworkTooLarge)
        );
    }

    #[tokio::test]
    async fn resolver_rejects_traversal_and_absolute_paths() {
        let directory = tempdir().unwrap();
        let cache = LocalArtworkCache::new(directory.path()).unwrap();
        for path in ["../secret.png", "/secret.png", r"aa\secret.png"] {
            assert_eq!(cache.resolve(path).await, Err(DomainError::ArtworkInvalid));
        }
    }
}
