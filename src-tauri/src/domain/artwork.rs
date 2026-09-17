//! Artwork value objects shared by application and infrastructure layers.

use crate::domain::entities::{ArtworkId, GameId};
use crate::domain::errors::DomainError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported artwork roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkKind {
    EditorSource,
    LauncherCover,
    Hero,
    Logo,
    JewelFront,
    DiscLabel,
    Icon,
}

impl ArtworkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EditorSource => "editor_source",
            Self::LauncherCover => "launcher_cover",
            Self::Hero => "hero",
            Self::Logo => "logo",
            Self::JewelFront => "jewel_front",
            Self::DiscLabel => "disc_label",
            Self::Icon => "icon",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "editor_source" => Ok(Self::EditorSource),
            "launcher_cover" => Ok(Self::LauncherCover),
            "hero" => Ok(Self::Hero),
            "logo" => Ok(Self::Logo),
            "jewel_front" => Ok(Self::JewelFront),
            "disc_label" => Ok(Self::DiscLabel),
            "icon" => Ok(Self::Icon),
            _ => Err(DomainError::DatabaseOperationFailed(
                "unknown artwork kind".to_owned(),
            )),
        }
    }
}

/// Provenance recorded for a cached artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkOrigin {
    Local,
    Steam,
    Manual,
    Remote,
}

impl ArtworkOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Steam => "steam",
            Self::Manual => "manual",
            Self::Remote => "remote",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "local" => Ok(Self::Local),
            "steam" => Ok(Self::Steam),
            "manual" => Ok(Self::Manual),
            "remote" => Ok(Self::Remote),
            _ => Err(DomainError::DatabaseOperationFailed(
                "unknown artwork provider".to_owned(),
            )),
        }
    }
}

/// Validated metadata for one artwork row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artwork {
    pub id: ArtworkId,
    pub game_id: GameId,
    pub kind: ArtworkKind,
    pub origin: ArtworkOrigin,
    pub relative_path: String,
    pub source_url: Option<String>,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub is_user_override: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Result returned by the local blob store after validation and normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedArtwork {
    pub relative_path: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
}

/// A bounded remote download request.
///
/// Implementations must apply the same URL policy to every redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkDownloadRequest {
    pub url: String,
    pub allowed_hosts: Vec<String>,
    pub max_bytes: usize,
    pub max_redirects: u8,
}

/// Bytes and the response MIME type returned by a downloader capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedArtwork {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}
