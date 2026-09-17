use crate::application::artwork::{ArtworkImport, ArtworkService};
use crate::application::label_export::{ExportFormat, ExportReceipt, LabelExportService};
use crate::application::labels::{LabelProjectService, SaveLabelProject};
use crate::domain::artwork::{ArtworkKind, ArtworkOrigin};
use crate::domain::entities::{ArtworkId, GameId, LabelProjectId};
use crate::domain::errors::{AppError, DomainError};
use crate::domain::label::{LabelProject, LabelScene, ThumbnailMetadata};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

pub struct LabelProjectState {
    pub projects: Arc<LabelProjectService>,
    pub artwork: Arc<ArtworkService>,
    pub exports: Arc<LabelExportService>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveLabelProjectRequest {
    pub id: Option<String>,
    pub game_id: Option<String>,
    pub name: String,
    pub scene: LabelScene,
    pub thumbnail_data_url: Option<String>,
    pub is_template: bool,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelProjectIdRequest {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteLabelProjectRequest {
    pub id: String,
    pub expected_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelArtworkRequest {
    pub game_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelArtworkIdRequest {
    pub artwork_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelArtworkImportRequest {
    pub game_id: String,
    pub data_url: String,
    pub purpose: ClipboardArtworkPurpose,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardArtworkPurpose {
    EditorSource,
    LauncherCover,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelExportRequest {
    pub project_name: String,
    pub scene: LabelScene,
    pub format: String,
    pub data_url: String,
    pub destination_path: String,
}

#[derive(Debug, Serialize)]
pub struct LabelThumbnailDto {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct LabelArtworkDto {
    pub id: String,
    pub game_id: String,
    pub kind: ArtworkKind,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub bytes: Option<Vec<u8>>,
}

#[tauri::command]
pub async fn label_project_list(
    state: State<'_, LabelProjectState>,
) -> Result<Vec<LabelProject>, AppError> {
    state.projects.list().await.map_err(AppError::from)
}

#[tauri::command]
pub async fn label_project_get(
    request: LabelProjectIdRequest,
    state: State<'_, LabelProjectState>,
) -> Result<LabelProject, AppError> {
    let id = parse_id(&request.id)?;
    state.projects.get(&id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn label_project_upsert(
    request: SaveLabelProjectRequest,
    state: State<'_, LabelProjectState>,
) -> Result<LabelProject, AppError> {
    let id = request.id.as_deref().map(parse_id).transpose()?;
    let game_id = request.game_id.as_deref().map(parse_game_id).transpose()?;
    request.scene.validate().map_err(AppError::from)?;
    crate::domain::label::validate_project_name(&request.name).map_err(AppError::from)?;
    let thumbnail = match request.thumbnail_data_url {
        Some(data_url) => {
            let (relative_path, width_px, height_px) = state
                .exports
                .store_thumbnail(&data_url)
                .map_err(AppError::from)?;
            Some(ThumbnailMetadata {
                relative_path,
                width_px,
                height_px,
                mime_type: "image/png".into(),
                updated_at: chrono::Utc::now(),
            })
        }
        None => None,
    };
    let thumbnail_path = thumbnail.as_ref().map(|value| value.relative_path.clone());
    let result = state
        .projects
        .save(SaveLabelProject {
            id,
            game_id,
            name: request.name,
            scene: request.scene,
            thumbnail,
            is_template: request.is_template,
            expected_revision: request.expected_revision,
        })
        .await;
    if result.is_err() {
        if let Some(path) = thumbnail_path {
            state.exports.remove_thumbnail(&path);
        }
    }
    result.map_err(AppError::from)
}

#[tauri::command]
pub async fn label_export(
    request: LabelExportRequest,
    state: State<'_, LabelProjectState>,
) -> Result<ExportReceipt, AppError> {
    request.scene.validate().map_err(AppError::from)?;
    crate::domain::label::validate_project_name(&request.project_name).map_err(AppError::from)?;
    let format = match request.format.as_str() {
        "png" => ExportFormat::Png,
        "pdf" => ExportFormat::Pdf,
        _ => return Err(AppError::from(DomainError::LabelExportInvalid)),
    };
    state
        .exports
        .export_to_path(
            request.scene.physical.width_mm,
            request.scene.physical.height_mm,
            request.scene.physical.dpi,
            format,
            &request.data_url,
            std::path::Path::new(&request.destination_path),
        )
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn label_calibration_export(
    state: State<'_, LabelProjectState>,
) -> Result<ExportReceipt, AppError> {
    state.exports.calibration_sheet().map_err(AppError::from)
}

#[tauri::command]
pub async fn label_thumbnail_get(
    request: LabelProjectIdRequest,
    state: State<'_, LabelProjectState>,
) -> Result<LabelThumbnailDto, AppError> {
    let project = state
        .projects
        .get(&parse_id(&request.id)?)
        .await
        .map_err(AppError::from)?;
    let thumbnail = project
        .thumbnail
        .ok_or_else(|| AppError::from(DomainError::LabelThumbnailInvalid))?;
    Ok(LabelThumbnailDto {
        mime_type: thumbnail.mime_type,
        bytes: state
            .exports
            .read_thumbnail(&thumbnail.relative_path)
            .map_err(AppError::from)?,
    })
}

#[tauri::command]
pub async fn label_project_delete(
    request: DeleteLabelProjectRequest,
    state: State<'_, LabelProjectState>,
) -> Result<(), AppError> {
    let id = parse_id(&request.id)?;
    state
        .projects
        .delete(&id, request.expected_revision)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn label_artwork_list(
    request: LabelArtworkRequest,
    state: State<'_, LabelProjectState>,
) -> Result<Vec<LabelArtworkDto>, AppError> {
    let game_id = parse_game_id(&request.game_id)?;
    let mut result = Vec::new();
    for kind in [
        ArtworkKind::EditorSource,
        ArtworkKind::LauncherCover,
        ArtworkKind::Hero,
        ArtworkKind::Logo,
        ArtworkKind::JewelFront,
        ArtworkKind::DiscLabel,
        ArtworkKind::Icon,
    ] {
        let Some(resolved) = state
            .artwork
            .resolve_active(&game_id, kind)
            .await
            .map_err(AppError::from)?
        else {
            continue;
        };
        result.push(LabelArtworkDto {
            id: resolved.artwork.id.to_string(),
            game_id: resolved.artwork.game_id.to_string(),
            kind: resolved.artwork.kind,
            width: resolved.artwork.width,
            height: resolved.artwork.height,
            mime_type: resolved.artwork.mime_type,
            bytes: None,
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn label_artwork_get(
    request: LabelArtworkIdRequest,
    state: State<'_, LabelProjectState>,
) -> Result<LabelArtworkDto, AppError> {
    let id = request
        .artwork_id
        .parse::<ArtworkId>()
        .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
    let resolved = state
        .artwork
        .resolve_by_id(&id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::from(DomainError::ArtworkInvalid))?;
    let bytes = tokio::fs::read(&resolved.absolute_path)
        .await
        .map_err(|_| AppError::from(DomainError::ArtworkIoFailed))?;
    Ok(LabelArtworkDto {
        id: resolved.artwork.id.to_string(),
        game_id: resolved.artwork.game_id.to_string(),
        kind: resolved.artwork.kind,
        width: resolved.artwork.width,
        height: resolved.artwork.height,
        mime_type: resolved.artwork.mime_type,
        bytes: Some(bytes),
    })
}

#[tauri::command]
pub async fn label_artwork_import_clipboard(
    request: LabelArtworkImportRequest,
    state: State<'_, LabelProjectState>,
) -> Result<LabelArtworkDto, AppError> {
    let game_id = parse_game_id(&request.game_id)?;
    let (bytes, mime_type) = decode_clipboard_image(&request.data_url)?;
    let kind = match request.purpose {
        ClipboardArtworkPurpose::EditorSource => ArtworkKind::EditorSource,
        ClipboardArtworkPurpose::LauncherCover => ArtworkKind::LauncherCover,
    };
    let artwork = state
        .artwork
        .import(ArtworkImport {
            game_id,
            kind,
            origin: ArtworkOrigin::Manual,
            bytes,
            mime_type,
            source_url: None,
            is_user_override: true,
        })
        .await
        .map_err(AppError::from)?;
    let resolved = state
        .artwork
        .resolve_by_id(&artwork.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::from(DomainError::ArtworkInvalid))?;
    let cached_bytes = tokio::fs::read(&resolved.absolute_path)
        .await
        .map_err(|_| AppError::from(DomainError::ArtworkIoFailed))?;
    Ok(LabelArtworkDto {
        id: artwork.id.to_string(),
        game_id: artwork.game_id.to_string(),
        kind: artwork.kind,
        width: artwork.width,
        height: artwork.height,
        mime_type: artwork.mime_type,
        bytes: Some(cached_bytes),
    })
}

fn decode_clipboard_image(value: &str) -> Result<(Vec<u8>, String), AppError> {
    const MAX_BYTES: usize = crate::infrastructure::artwork::MAX_ARTWORK_BYTES;
    const ALLOWED: [(&str, &str); 3] = [
        ("data:image/png;base64,", "image/png"),
        ("data:image/jpeg;base64,", "image/jpeg"),
        ("data:image/webp;base64,", "image/webp"),
    ];
    let (encoded, mime_type) = ALLOWED
        .iter()
        .find_map(|(prefix, mime)| value.strip_prefix(prefix).map(|data| (data, *mime)))
        .ok_or_else(|| AppError::from(DomainError::ArtworkInvalid))?;
    if encoded.len() > MAX_BYTES * 4 / 3 + 4 {
        return Err(AppError::from(DomainError::ArtworkTooLarge));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(AppError::from(if bytes.len() > MAX_BYTES {
            DomainError::ArtworkTooLarge
        } else {
            DomainError::ArtworkInvalid
        }));
    }
    Ok((bytes, mime_type.to_owned()))
}

fn parse_id(raw: &str) -> Result<LabelProjectId, AppError> {
    raw.parse()
        .map_err(|_| AppError::from(DomainError::LabelProjectInvalid))
}

fn parse_game_id(raw: &str) -> Result<GameId, AppError> {
    raw.parse()
        .map_err(|_| AppError::from(DomainError::LabelProjectInvalid))
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    #[test]
    fn clipboard_image_accepts_only_bounded_image_data_urls() {
        let png = decode_clipboard_image("data:image/png;base64,iVBORw0KGgo=").unwrap();
        assert_eq!(png.1, "image/png");
        assert!(decode_clipboard_image("https://example.test/a.png").is_err());
        assert!(decode_clipboard_image("data:image/svg+xml;base64,PHN2Zz4=").is_err());
        assert!(decode_clipboard_image("data:image/png;base64,%%%").is_err());
    }
}
