//! Fail-closed policy adapter for optional remote artwork capabilities.

use crate::domain::artwork::{ArtworkDownloadRequest, DownloadedArtwork};
use crate::domain::entities::GameId;
use crate::domain::errors::DomainError;
use crate::domain::ports::{ArtworkDownloader, ArtworkProvider, PortResult};
use std::net::IpAddr;
use std::sync::Arc;
use url::{Host, Url};

const MAX_REMOTE_CANDIDATES: usize = 8;

/// Remote policy. Default is deliberately disabled with an empty allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteArtworkPolicy {
    pub enabled: bool,
    pub allowed_hosts: Vec<String>,
    pub max_download_bytes: usize,
    pub max_redirects: u8,
}

impl Default for RemoteArtworkPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_hosts: Vec::new(),
            max_download_bytes: super::MAX_ARTWORK_BYTES,
            max_redirects: 2,
        }
    }
}

impl RemoteArtworkPolicy {
    pub fn validate(&self) -> Result<(), DomainError> {
        if !self.enabled {
            return Err(DomainError::ArtworkRemoteDisabled);
        }
        if self.allowed_hosts.is_empty()
            || self.max_download_bytes == 0
            || self.max_download_bytes > super::MAX_ARTWORK_BYTES
            || self.max_redirects > 5
            || self
                .allowed_hosts
                .iter()
                .any(|host| !is_valid_allowlisted_host(host))
        {
            return Err(DomainError::ArtworkRemoteRejected);
        }
        Ok(())
    }

    pub fn download_request(&self, raw_url: &str) -> Result<ArtworkDownloadRequest, DomainError> {
        self.validate()?;
        validate_remote_url(raw_url, &self.allowed_hosts)?;
        Ok(ArtworkDownloadRequest {
            url: raw_url.to_owned(),
            allowed_hosts: self.allowed_hosts.clone(),
            max_bytes: self.max_download_bytes,
            max_redirects: self.max_redirects,
        })
    }
}

/// Applies enablement and URL policy before exposing candidates from any provider.
pub struct SecureArtworkProvider {
    inner: Arc<dyn ArtworkProvider>,
    policy: RemoteArtworkPolicy,
}

impl SecureArtworkProvider {
    pub fn new(inner: Arc<dyn ArtworkProvider>, policy: RemoteArtworkPolicy) -> Self {
        Self { inner, policy }
    }
}

impl ArtworkProvider for SecureArtworkProvider {
    fn fetch_artwork_urls<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, Vec<String>> {
        Box::pin(async move {
            self.policy.validate()?;
            let urls = self.inner.fetch_artwork_urls(game_id).await?;
            if urls.len() > MAX_REMOTE_CANDIDATES {
                return Err(DomainError::ArtworkRemoteRejected);
            }
            for url in &urls {
                validate_remote_url(url, &self.policy.allowed_hosts)?;
            }
            Ok(urls)
        })
    }
}

/// Default downloader used when no explicitly-enabled HTTP capability exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct DisabledArtworkDownloader;

impl ArtworkDownloader for DisabledArtworkDownloader {
    fn download<'a>(
        &'a self,
        _request: &'a ArtworkDownloadRequest,
    ) -> PortResult<'a, DownloadedArtwork> {
        Box::pin(async { Err(DomainError::ArtworkRemoteDisabled) })
    }
}

pub fn validate_remote_url(raw_url: &str, allowed_hosts: &[String]) -> Result<(), DomainError> {
    let url = Url::parse(raw_url).map_err(|_| DomainError::ArtworkRemoteRejected)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(DomainError::ArtworkRemoteRejected);
    }
    let host = match url.host() {
        Some(Host::Domain(host)) => host.trim_end_matches('.').to_ascii_lowercase(),
        Some(Host::Ipv4(_)) | Some(Host::Ipv6(_)) | None => {
            return Err(DomainError::ArtworkRemoteRejected)
        }
    };
    if !allowed_hosts.iter().any(|allowed| {
        allowed
            .trim_end_matches('.')
            .eq_ignore_ascii_case(host.as_str())
    }) {
        return Err(DomainError::ArtworkRemoteRejected);
    }
    Ok(())
}

fn is_valid_allowlisted_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.contains(['/', '\\', ':', '@'])
        && host.parse::<IpAddr>().is_err()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::fakes::FakeArtworkProvider;

    #[tokio::test]
    async fn default_policy_never_calls_provider() {
        let game_id = GameId::new();
        let inner = FakeArtworkProvider::default();
        inner.set_urls(game_id, vec!["https://cdn.example.test/cover.png".into()]);
        let provider = SecureArtworkProvider::new(Arc::new(inner), RemoteArtworkPolicy::default());
        assert_eq!(
            provider.fetch_artwork_urls(&game_id).await,
            Err(DomainError::ArtworkRemoteDisabled)
        );
    }

    #[test]
    fn url_policy_requires_https_exact_host_and_no_credentials_or_ip() {
        let hosts = vec!["cdn.example.test".to_owned()];
        assert!(validate_remote_url("https://cdn.example.test/cover.png", &hosts).is_ok());
        for url in [
            "http://cdn.example.test/cover.png",
            "https://evil.example.test/cover.png",
            "https://cdn.example.test.evil.test/cover.png",
            "https://user:secret@cdn.example.test/cover.png",
            "https://127.0.0.1/cover.png",
            "https://cdn.example.test:444/cover.png",
        ] {
            assert_eq!(
                validate_remote_url(url, &hosts),
                Err(DomainError::ArtworkRemoteRejected)
            );
        }
    }

    #[test]
    fn enabled_policy_builds_bounded_download_request() {
        let policy = RemoteArtworkPolicy {
            enabled: true,
            allowed_hosts: vec!["cdn.example.test".into()],
            max_download_bytes: 1024,
            max_redirects: 1,
        };
        let request = policy
            .download_request("https://cdn.example.test/cover.png")
            .unwrap();
        assert_eq!(request.max_bytes, 1024);
        assert_eq!(request.max_redirects, 1);
    }
}
