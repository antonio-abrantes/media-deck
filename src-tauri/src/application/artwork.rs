//! Artwork import, resolution and optional remote refresh use cases.

use crate::domain::artwork::{Artwork, ArtworkKind, ArtworkOrigin};
use crate::domain::clock::Clock;
use crate::domain::entities::{ArtworkId, GameId};
use crate::domain::errors::DomainError;
use crate::domain::ports::{
    ArtworkBlobStore, ArtworkDownloader, ArtworkProvider, ArtworkRepository,
};
use std::sync::Arc;
use url::{Host, Url};

const MAX_REMOTE_CANDIDATES: usize = 8;
const MAX_REMOTE_DOWNLOAD_BYTES: usize = 16 * 1024 * 1024;

pub struct ArtworkImport {
    pub game_id: GameId,
    pub kind: ArtworkKind,
    pub origin: ArtworkOrigin,
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub source_url: Option<String>,
    pub is_user_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtwork {
    pub artwork: Artwork,
    /// Canonical path produced by the local store, never accepted from a caller.
    pub absolute_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteArtworkConfig {
    pub enabled: bool,
    pub allowed_hosts: Vec<String>,
    pub max_download_bytes: usize,
    pub max_redirects: u8,
}

impl Default for RemoteArtworkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_hosts: Vec::new(),
            max_download_bytes: MAX_REMOTE_DOWNLOAD_BYTES,
            max_redirects: 2,
        }
    }
}

pub struct ArtworkService {
    repository: Arc<dyn ArtworkRepository>,
    store: Arc<dyn ArtworkBlobStore>,
    clock: Arc<dyn Clock>,
    remote_provider: Option<Arc<dyn ArtworkProvider>>,
    downloader: Option<Arc<dyn ArtworkDownloader>>,
    remote: RemoteArtworkConfig,
}

impl ArtworkService {
    pub fn new(
        repository: Arc<dyn ArtworkRepository>,
        store: Arc<dyn ArtworkBlobStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            store,
            clock,
            remote_provider: None,
            downloader: None,
            remote: RemoteArtworkConfig::default(),
        }
    }

    pub fn with_remote(
        mut self,
        provider: Arc<dyn ArtworkProvider>,
        downloader: Arc<dyn ArtworkDownloader>,
        config: RemoteArtworkConfig,
    ) -> Self {
        self.remote_provider = Some(provider);
        self.downloader = Some(downloader);
        self.remote = config;
        self
    }

    pub async fn import(&self, request: ArtworkImport) -> Result<Artwork, DomainError> {
        let cached = self.store.store(&request.bytes, &request.mime_type).await?;
        let source_url = request
            .source_url
            .as_deref()
            .map(sanitize_source_url)
            .transpose()?;
        let now = self.clock.now_utc();

        if let Some(mut existing) = self
            .repository
            .find_by_game_kind_hash(&request.game_id, request.kind, &cached.sha256)
            .await?
        {
            if request.is_user_override && !existing.is_user_override {
                existing.origin = request.origin;
                existing.source_url = source_url;
                existing.is_user_override = true;
                existing.updated_at = now;
                self.repository.upsert(&existing).await?;
            }
            return Ok(existing);
        }

        let artwork = Artwork {
            id: ArtworkId::new(),
            game_id: request.game_id,
            kind: request.kind,
            origin: request.origin,
            relative_path: cached.relative_path,
            source_url,
            sha256: cached.sha256,
            width: cached.width,
            height: cached.height,
            mime_type: cached.mime_type,
            is_user_override: request.is_user_override,
            created_at: now,
            updated_at: now,
        };
        self.repository.upsert(&artwork).await?;
        self.repository
            .find_by_game_kind_hash(&artwork.game_id, artwork.kind, &artwork.sha256)
            .await?
            .ok_or_else(|| {
                DomainError::DatabaseOperationFailed("artwork upsert was not visible".to_owned())
            })
    }

    pub async fn resolve_active(
        &self,
        game_id: &GameId,
        kind: ArtworkKind,
    ) -> Result<Option<ResolvedArtwork>, DomainError> {
        let Some(artwork) = self
            .repository
            .list_for_game_kind(game_id, kind)
            .await?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let absolute_path = self.store.resolve(&artwork.relative_path).await?;
        Ok(Some(ResolvedArtwork {
            artwork,
            absolute_path,
        }))
    }

    pub async fn resolve_by_id(
        &self,
        id: &ArtworkId,
    ) -> Result<Option<ResolvedArtwork>, DomainError> {
        let Some(artwork) = self.repository.find_by_id(id).await? else {
            return Ok(None);
        };
        let absolute_path = self.store.resolve(&artwork.relative_path).await?;
        Ok(Some(ResolvedArtwork {
            artwork,
            absolute_path,
        }))
    }

    /// Fetch and cache the first valid candidate. No capability is called while disabled.
    pub async fn refresh_remote(
        &self,
        game_id: GameId,
        kind: ArtworkKind,
    ) -> Result<Option<Artwork>, DomainError> {
        validate_remote_config(&self.remote)?;
        let provider = self
            .remote_provider
            .as_ref()
            .ok_or(DomainError::ArtworkRemoteDisabled)?;
        let downloader = self
            .downloader
            .as_ref()
            .ok_or(DomainError::ArtworkRemoteDisabled)?;
        let urls = provider.fetch_artwork_urls(&game_id).await?;
        if urls.len() > MAX_REMOTE_CANDIDATES {
            return Err(DomainError::ArtworkRemoteRejected);
        }

        let mut last_error = None;
        for url in urls {
            validate_remote_url(&url, &self.remote.allowed_hosts)?;
            let request = crate::domain::artwork::ArtworkDownloadRequest {
                url: url.clone(),
                allowed_hosts: self.remote.allowed_hosts.clone(),
                max_bytes: self.remote.max_download_bytes,
                max_redirects: self.remote.max_redirects,
            };
            match downloader.download(&request).await {
                Ok(downloaded) => {
                    if downloaded.bytes.len() > request.max_bytes {
                        return Err(DomainError::ArtworkTooLarge);
                    }
                    return self
                        .import(ArtworkImport {
                            game_id,
                            kind,
                            origin: ArtworkOrigin::Remote,
                            bytes: downloaded.bytes,
                            mime_type: downloaded.mime_type,
                            source_url: Some(url),
                            is_user_override: false,
                        })
                        .await
                        .map(Some);
                }
                Err(error) => last_error = Some(error),
            }
        }
        match last_error {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}

fn validate_remote_config(config: &RemoteArtworkConfig) -> Result<(), DomainError> {
    if !config.enabled {
        return Err(DomainError::ArtworkRemoteDisabled);
    }
    if config.allowed_hosts.is_empty()
        || config.max_download_bytes == 0
        || config.max_download_bytes > MAX_REMOTE_DOWNLOAD_BYTES
        || config.max_redirects > 5
    {
        return Err(DomainError::ArtworkRemoteRejected);
    }
    for host in &config.allowed_hosts {
        validate_remote_url(&format!("https://{host}/"), std::slice::from_ref(host))?;
    }
    Ok(())
}

fn validate_remote_url(raw_url: &str, allowed_hosts: &[String]) -> Result<(), DomainError> {
    let url = Url::parse(raw_url).map_err(|_| DomainError::ArtworkRemoteRejected)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(DomainError::ArtworkRemoteRejected);
    }
    let host = match url.host() {
        Some(Host::Domain(host)) => host.trim_end_matches('.'),
        _ => return Err(DomainError::ArtworkRemoteRejected),
    };
    if !allowed_hosts
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed.trim_end_matches('.')))
    {
        return Err(DomainError::ArtworkRemoteRejected);
    }
    Ok(())
}

fn sanitize_source_url(raw_url: &str) -> Result<String, DomainError> {
    let mut url = Url::parse(raw_url).map_err(|_| DomainError::ArtworkRemoteRejected)?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(DomainError::ArtworkRemoteRejected);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::artwork::{ArtworkDownloadRequest, DownloadedArtwork};
    use crate::domain::clock::FakeClock;
    use crate::domain::entities::{Game, GameProvider};
    use crate::domain::ports::{ArtworkDownloader, PortResult};
    use crate::infrastructure::artwork::LocalArtworkCache;
    use crate::infrastructure::database::artwork::SqliteArtworkRepository;
    use crate::infrastructure::database::games::SqliteGameRepository;
    use crate::infrastructure::database::open_and_migrate;
    use crate::infrastructure::fakes::FakeArtworkProvider;
    use chrono::Utc;
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;
    use tempfile::tempdir;

    #[derive(Clone)]
    struct FakeDownloader {
        bytes: Vec<u8>,
    }

    impl ArtworkDownloader for FakeDownloader {
        fn download<'a>(
            &'a self,
            _request: &'a ArtworkDownloadRequest,
        ) -> PortResult<'a, DownloadedArtwork> {
            Box::pin(async move {
                Ok(DownloadedArtwork {
                    bytes: self.bytes.clone(),
                    mime_type: "image/png".into(),
                })
            })
        }
    }

    fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::new(16, 24))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    async fn service() -> (ArtworkService, GameId, tempfile::TempDir) {
        let directory = tempdir().unwrap();
        let pool = open_and_migrate(&directory.path().join("test.db"))
            .await
            .unwrap();
        let game = Game::new(GameProvider::Steam, Some("440".into()), "TF2", Utc::now());
        SqliteGameRepository::new(pool.clone())
            .upsert(&game)
            .await
            .unwrap();
        let repository = Arc::new(SqliteArtworkRepository::new(pool));
        std::fs::create_dir(directory.path().join("artwork")).unwrap();
        let store = Arc::new(LocalArtworkCache::new(directory.path().join("artwork")).unwrap());
        (
            ArtworkService::new(repository, store, Arc::new(FakeClock::new())),
            game.id,
            directory,
        )
    }

    #[tokio::test]
    async fn import_deduplicates_and_manual_override_wins_resolution() {
        let (service, game_id, _directory) = service().await;
        let remote = service
            .import(ArtworkImport {
                game_id,
                kind: ArtworkKind::LauncherCover,
                origin: ArtworkOrigin::Remote,
                bytes: png(),
                mime_type: "image/png".into(),
                source_url: Some("https://cdn.example.test/cover.png?token=secret".into()),
                is_user_override: false,
            })
            .await
            .unwrap();
        let manual = service
            .import(ArtworkImport {
                game_id,
                kind: ArtworkKind::LauncherCover,
                origin: ArtworkOrigin::Manual,
                bytes: png(),
                mime_type: "image/png".into(),
                source_url: None,
                is_user_override: true,
            })
            .await
            .unwrap();
        assert_eq!(remote.id, manual.id);
        assert!(manual.is_user_override);
        assert_eq!(manual.source_url, None);
        assert!(
            service
                .resolve_active(&game_id, ArtworkKind::LauncherCover)
                .await
                .unwrap()
                .unwrap()
                .artwork
                .is_user_override
        );
    }

    #[tokio::test]
    async fn remote_is_disabled_without_calling_network_capability() {
        let (service, game_id, _directory) = service().await;
        assert_eq!(
            service
                .refresh_remote(game_id, ArtworkKind::LauncherCover)
                .await,
            Err(DomainError::ArtworkRemoteDisabled)
        );
    }

    #[tokio::test]
    async fn enabled_remote_uses_allowlist_and_caches_without_real_network() {
        let (service, game_id, directory) = service().await;
        let provider = FakeArtworkProvider::default();
        provider.set_urls(
            game_id,
            vec!["https://cdn.example.test/cover.png?token=secret".into()],
        );
        let service = service.with_remote(
            Arc::new(provider),
            Arc::new(FakeDownloader { bytes: png() }),
            RemoteArtworkConfig {
                enabled: true,
                allowed_hosts: vec!["cdn.example.test".into()],
                max_download_bytes: 1024 * 1024,
                max_redirects: 1,
            },
        );
        let artwork = service
            .refresh_remote(game_id, ArtworkKind::LauncherCover)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            artwork.source_url.as_deref(),
            Some("https://cdn.example.test/cover.png")
        );
        assert!(directory.path().join("artwork").is_dir());
    }
}
