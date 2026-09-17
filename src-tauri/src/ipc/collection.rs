use crate::application::collection::{CollectionEntry, GameCollectionService};
use crate::domain::entities::{GameId, GameProvider};
use crate::domain::errors::{AppError, DomainError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

pub struct CollectionState(pub Arc<GameCollectionService>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionGameRequest {
    pub game_id: String,
}

#[derive(Debug, Serialize)]
pub struct CollectionCoverDto {
    pub id: String,
    pub relative_path: String,
}

#[derive(Debug, Serialize)]
pub struct CollectionCoverBytesDto {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct CollectionEntryDto {
    pub game_id: String,
    pub display_name: String,
    pub provider: GameProvider,
    pub provider_game_id: Option<String>,
    pub install_dir: Option<String>,
    pub installed: bool,
    pub offline: bool,
    pub profile_id: String,
    pub profile_name: String,
    pub last_export_kind: String,
    pub last_media_key: String,
    pub last_content_hash: String,
    pub schema_version: u32,
    pub export_count: u32,
    pub first_activated_at: String,
    pub last_exported_at: String,
    pub active_cover: Option<CollectionCoverDto>,
}

impl From<CollectionEntry> for CollectionEntryDto {
    fn from(value: CollectionEntry) -> Self {
        let offline = value
            .game
            .metadata_json
            .pointer("/_steam_scan/library_offline")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        Self {
            game_id: value.game.id.to_string(),
            display_name: value.game.display_name,
            provider: value.game.provider,
            provider_game_id: value.game.provider_game_id,
            install_dir: value.game.install_dir,
            installed: value.game.installed,
            offline,
            profile_id: value.profile.id.to_string(),
            profile_name: value.profile.name,
            last_export_kind: value.activation.last_export_kind.as_str().into(),
            last_media_key: value.activation.last_media_key.to_string(),
            last_content_hash: value.activation.last_content_hash.as_str().into(),
            schema_version: value.activation.schema_version,
            export_count: value.activation.export_count,
            first_activated_at: value.activation.first_activated_at.to_rfc3339(),
            last_exported_at: value.activation.last_exported_at.to_rfc3339(),
            active_cover: value.active_cover.map(|cover| CollectionCoverDto {
                id: cover.artwork.id.to_string(),
                relative_path: cover.artwork.relative_path,
            }),
        }
    }
}

#[tauri::command]
pub async fn collection_list(
    state: State<'_, CollectionState>,
) -> Result<Vec<CollectionEntryDto>, AppError> {
    state
        .0
        .list()
        .await
        .map(|entries| entries.into_iter().map(CollectionEntryDto::from).collect())
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn collection_get(
    request: CollectionGameRequest,
    state: State<'_, CollectionState>,
) -> Result<Option<CollectionEntryDto>, AppError> {
    state
        .0
        .get(&parse_game_id(&request.game_id)?)
        .await
        .map(|entry| entry.map(CollectionEntryDto::from))
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn collection_cover_get(
    request: CollectionGameRequest,
    state: State<'_, CollectionState>,
) -> Result<Option<CollectionCoverBytesDto>, AppError> {
    state
        .0
        .cover(&parse_game_id(&request.game_id)?)
        .await
        .map(|cover| {
            cover.map(|value| CollectionCoverBytesDto {
                mime_type: value.mime_type,
                bytes: value.bytes,
            })
        })
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn collection_deactivate(
    request: CollectionGameRequest,
    state: State<'_, CollectionState>,
) -> Result<(), AppError> {
    state
        .0
        .deactivate(&parse_game_id(&request.game_id)?)
        .await
        .map_err(AppError::from)
}

fn parse_game_id(value: &str) -> Result<GameId, AppError> {
    value
        .parse()
        .map_err(|_| AppError::from(DomainError::GameNotInstalled))
}
