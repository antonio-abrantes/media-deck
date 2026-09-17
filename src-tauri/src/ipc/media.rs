//! DTO boundary for validating untrusted `GAME.INI` content.

use crate::application::media_creator::{
    CreatorPreview, CreatorSelection, IniExportReceipt, LegacyImport, MediaCreatorService,
};
use crate::domain::entities::{DeviceId, GameId, MediaDescriptor, MediaKey, MediaKind, ProfileId};
use crate::domain::errors::{AppError, DomainError};
use crate::domain::ids::SystemIdGenerator;
use crate::domain::media_profile::{
    parse_profile, MediaProfile, MediaProvider, ParsedMediaProfile, ProfileParseError,
    ProfileSourceVersion, ProfileWarning,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
#[cfg(windows)]
use tauri::{AppHandle, Emitter};

pub struct MediaCreatorState(pub Arc<MediaCreatorService>);

#[cfg(windows)]
pub struct OpticalMediaState(pub Arc<crate::application::optical_media::OpticalMediaService>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaValidateRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct MediaValidateResponse {
    pub source_version: ProfileSourceVersion,
    pub profile: MediaProfileDto,
    pub warnings: Vec<ProfileWarning>,
}

#[derive(Debug, Serialize)]
pub struct MediaProfileDto {
    pub media_id: String,
    pub media_kind: Option<String>,
    pub profile_id: String,
    pub created_at: Option<String>,
    pub provider: MediaProvider,
    pub app_id: Option<u64>,
    pub display_name: String,
    pub process_hints: Vec<String>,
    pub cover_artwork_id: Option<String>,
    pub cover_cache_path: Option<String>,
    pub artwork_hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MediaValidationErrorDto {
    pub code: crate::domain::media_profile::ProfileErrorCode,
    pub line: Option<usize>,
    pub field: Option<String>,
}

impl From<ProfileParseError> for MediaValidationErrorDto {
    fn from(error: ProfileParseError) -> Self {
        Self {
            code: error.code,
            line: error.line,
            field: error.field,
        }
    }
}

impl From<ParsedMediaProfile> for MediaValidateResponse {
    fn from(parsed: ParsedMediaProfile) -> Self {
        Self {
            source_version: parsed.source_version,
            profile: parsed.profile.into(),
            warnings: parsed.warnings,
        }
    }
}

impl From<MediaProfile> for MediaProfileDto {
    fn from(profile: MediaProfile) -> Self {
        Self {
            media_id: profile.media_id.to_string(),
            media_kind: profile.media_kind.map(|kind| match kind {
                crate::domain::entities::MediaKind::Floppy => "floppy".to_owned(),
                crate::domain::entities::MediaKind::Optical => "optical".to_owned(),
                crate::domain::entities::MediaKind::Removable => "removable".to_owned(),
            }),
            profile_id: profile.profile_id.to_string(),
            created_at: profile.created_at.map(|value| value.to_rfc3339()),
            provider: profile.provider,
            app_id: profile.app_id,
            display_name: profile.display_name,
            process_hints: profile.process_hints,
            cover_artwork_id: profile.cover_artwork_id.map(|id| id.to_string()),
            cover_cache_path: profile.cover_cache_path,
            artwork_hint: profile.artwork_hint,
        }
    }
}

/// Validate profile content without granting filesystem access to the WebView.
#[tauri::command]
pub fn media_validate(
    request: MediaValidateRequest,
) -> Result<MediaValidateResponse, MediaValidationErrorDto> {
    parse_profile(request.content.as_bytes(), &SystemIdGenerator)
        .map(MediaValidateResponse::from)
        .map_err(MediaValidationErrorDto::from)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatorSelectionRequest {
    pub game_id: String,
    pub profile_id: String,
    pub device_id: Option<String>,
    pub media_id: String,
    pub media_kind: Option<MediaKind>,
    #[serde(default)]
    pub include_artwork: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatorWriteRequest {
    #[serde(flatten)]
    pub selection: CreatorSelectionRequest,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatorIniExportRequest {
    #[serde(flatten)]
    pub selection: CreatorSelectionRequest,
    pub destination_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportInspectRequest {
    pub device_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportUpgradeRequest {
    #[serde(flatten)]
    pub selection: CreatorSelectionRequest,
    pub expected_source_hash: String,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaVerifyRequest {
    pub media_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize)]
pub struct CreatorPreviewDto {
    pub profile: MediaProfileDto,
    pub canonical_ini: String,
    pub device_name: Option<String>,
    pub mount_point: Option<String>,
    pub include_artwork: bool,
}

#[derive(Debug, Serialize)]
pub struct IniExportReceiptDto {
    pub path: String,
    pub content_hash: String,
    pub game_id: String,
    pub profile_id: String,
    pub export_count: u32,
    pub last_exported_at: String,
}

#[derive(Debug, Serialize)]
pub struct LegacyImportDto {
    pub source_version: ProfileSourceVersion,
    pub profile: MediaProfileDto,
    pub warnings: Vec<ProfileWarning>,
    pub source_hash: String,
}

#[derive(Debug, Serialize)]
pub struct MediaDescriptorDto {
    pub id: String,
    pub media_id: String,
    pub profile_id: String,
    pub media_kind: MediaKind,
    pub device_id: Option<String>,
    pub schema_version: u32,
    pub content_hash: String,
    pub last_drive: Option<String>,
    pub created_at: String,
    pub last_verified_at: Option<String>,
    pub status: crate::domain::entities::MediaStatus,
}

impl From<CreatorPreview> for CreatorPreviewDto {
    fn from(value: CreatorPreview) -> Self {
        Self {
            profile: value.profile.into(),
            canonical_ini: value.canonical_ini,
            device_name: value.device_name,
            mount_point: value.mount_point,
            include_artwork: value.include_artwork,
        }
    }
}

impl From<IniExportReceipt> for IniExportReceiptDto {
    fn from(value: IniExportReceipt) -> Self {
        Self {
            path: value.path.to_string_lossy().into_owned(),
            content_hash: value.content_hash.as_str().to_owned(),
            game_id: value.activation.game_id.to_string(),
            profile_id: value.activation.profile_id.to_string(),
            export_count: value.activation.export_count,
            last_exported_at: value.activation.last_exported_at.to_rfc3339(),
        }
    }
}

impl From<LegacyImport> for LegacyImportDto {
    fn from(value: LegacyImport) -> Self {
        Self {
            source_version: value.source_version,
            profile: value.profile.into(),
            warnings: value.warnings,
            source_hash: value.source_hash,
        }
    }
}

impl From<MediaDescriptor> for MediaDescriptorDto {
    fn from(value: MediaDescriptor) -> Self {
        Self {
            id: value.id.to_string(),
            media_id: value.media_key.to_string(),
            profile_id: value.profile_id.to_string(),
            media_kind: value.media_kind,
            device_id: value.last_device_id.map(|id| id.to_string()),
            schema_version: value.schema_version,
            content_hash: value.content_hash.as_str().to_owned(),
            last_drive: value.last_drive,
            created_at: value.created_at.to_rfc3339(),
            last_verified_at: value.last_verified_at.map(|time| time.to_rfc3339()),
            status: value.status,
        }
    }
}

#[tauri::command]
pub async fn media_creator_preview(
    request: CreatorSelectionRequest,
    state: State<'_, MediaCreatorState>,
) -> Result<CreatorPreviewDto, AppError> {
    state
        .0
        .preview(&parse_selection(request)?)
        .await
        .map(CreatorPreviewDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn media_creator_export_ini(
    request: CreatorIniExportRequest,
    state: State<'_, MediaCreatorState>,
) -> Result<IniExportReceiptDto, AppError> {
    let selection = parse_selection(request.selection)?;
    state
        .0
        .export_ini(&selection, std::path::Path::new(&request.destination_path))
        .await
        .map(IniExportReceiptDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn media_creator_write(
    request: CreatorWriteRequest,
    state: State<'_, MediaCreatorState>,
) -> Result<MediaDescriptorDto, AppError> {
    let selection = parse_selection(request.selection)?;
    state
        .0
        .write(&selection, request.confirmed)
        .await
        .map(MediaDescriptorDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn media_import_inspect(
    request: ImportInspectRequest,
    state: State<'_, MediaCreatorState>,
) -> Result<LegacyImportDto, AppError> {
    state
        .0
        .inspect_import(parse_id(&request.device_id)?)
        .await
        .map(LegacyImportDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn media_import_upgrade(
    request: ImportUpgradeRequest,
    state: State<'_, MediaCreatorState>,
) -> Result<MediaDescriptorDto, AppError> {
    let selection = parse_selection(request.selection)?;
    state
        .0
        .upgrade_import(&selection, &request.expected_source_hash, request.confirmed)
        .await
        .map(MediaDescriptorDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn media_history_list(
    state: State<'_, MediaCreatorState>,
) -> Result<Vec<MediaDescriptorDto>, AppError> {
    state
        .0
        .list()
        .await
        .map(|items| items.into_iter().map(MediaDescriptorDto::from).collect())
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn media_history_verify(
    request: MediaVerifyRequest,
    state: State<'_, MediaCreatorState>,
) -> Result<MediaDescriptorDto, AppError> {
    let media_key = MediaKey::new(request.media_id).map_err(AppError::from)?;
    state
        .0
        .verify(&media_key, parse_id(&request.device_id)?)
        .await
        .map(MediaDescriptorDto::from)
        .map_err(AppError::from)
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalDeviceRequest {
    pub device_id: String,
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalBurnRequest {
    #[serde(flatten)]
    pub selection: CreatorSelectionRequest,
    pub recorder_id: String,
    pub confirmed: bool,
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalCancelRequest {
    pub operation_id: String,
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalEraseRequest {
    pub device_id: String,
    pub recorder_id: String,
    pub typed_confirmation: String,
}

#[cfg(windows)]
#[derive(Debug, Serialize, Clone)]
pub struct OpticalProgressEvent {
    pub operation_id: String,
    #[serde(flatten)]
    pub progress: crate::domain::optical::OpticalProgress,
}

#[cfg(windows)]
#[tauri::command]
pub async fn media_optical_recorders(
    request: OpticalDeviceRequest,
    state: State<'_, OpticalMediaState>,
) -> Result<Vec<crate::domain::optical::OpticalRecorder>, AppError> {
    state
        .0
        .recorders(parse_id(&request.device_id)?)
        .await
        .map_err(AppError::from)
}

#[cfg(windows)]
#[tauri::command]
pub async fn media_optical_export_iso(
    request: CreatorSelectionRequest,
    state: State<'_, OpticalMediaState>,
) -> Result<crate::application::optical_media::OpticalArtifact, AppError> {
    state
        .0
        .export_iso(&parse_selection(request)?)
        .await
        .map_err(AppError::from)
}

#[cfg(windows)]
#[tauri::command]
pub async fn media_optical_burn(
    request: OpticalBurnRequest,
    state: State<'_, OpticalMediaState>,
    app: AppHandle,
) -> Result<MediaDescriptorDto, AppError> {
    if !request.confirmed {
        return Err(AppError::from(DomainError::InvalidTransition {
            state: "burn_confirmation".into(),
            event: "not_confirmed".into(),
        }));
    }
    let selection = parse_selection(request.selection)?;
    let operation_id = uuid::Uuid::now_v7().to_string();
    let event_operation_id = operation_id.clone();
    let progress = Arc::new(move |progress| {
        let _ = app.emit_to(
            "main",
            "media://optical_progress",
            OpticalProgressEvent {
                operation_id: event_operation_id.clone(),
                progress,
            },
        );
    });
    state
        .0
        .burn(&selection, request.recorder_id, operation_id, progress)
        .await
        .map(MediaDescriptorDto::from)
        .map_err(AppError::from)
}

#[cfg(windows)]
#[tauri::command]
pub fn media_optical_cancel(
    request: OpticalCancelRequest,
    state: State<'_, OpticalMediaState>,
) -> Result<(), AppError> {
    state
        .0
        .cancel(&request.operation_id)
        .map_err(AppError::from)
}

#[cfg(windows)]
#[tauri::command]
pub async fn media_optical_erase_challenge(
    request: OpticalDeviceRequest,
    state: State<'_, OpticalMediaState>,
) -> Result<crate::application::optical_media::EraseChallenge, AppError> {
    state
        .0
        .erase_challenge(parse_id(&request.device_id)?)
        .await
        .map_err(AppError::from)
}

#[cfg(windows)]
#[tauri::command]
pub async fn media_optical_erase(
    request: OpticalEraseRequest,
    state: State<'_, OpticalMediaState>,
) -> Result<(), AppError> {
    state
        .0
        .erase(
            parse_id(&request.device_id)?,
            &request.recorder_id,
            &request.typed_confirmation,
        )
        .await
        .map_err(AppError::from)
}

pub(crate) fn parse_selection(
    request: CreatorSelectionRequest,
) -> Result<CreatorSelection, AppError> {
    Ok(CreatorSelection {
        game_id: parse_id::<GameId>(&request.game_id)?,
        profile_id: parse_id::<ProfileId>(&request.profile_id)?,
        device_id: request
            .device_id
            .as_deref()
            .map(parse_id::<DeviceId>)
            .transpose()?,
        media_key: MediaKey::new(request.media_id).map_err(AppError::from)?,
        media_kind: request.media_kind,
        include_artwork: request.include_artwork,
    })
}

pub(crate) fn parse_id<T: std::str::FromStr>(value: &str) -> Result<T, AppError> {
    value
        .parse()
        .map_err(|_| AppError::from(DomainError::MediaProfileInvalid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_validate_returns_typed_profile() {
        let response = media_validate(MediaValidateRequest {
            content: "[MEDIA]\nSCHEMA=2\nMEDIA_ID=GAME-1\nPROFILE_ID=01994a56-69d7-7ef4-a137-94808fa24131\n[GAME]\nPROVIDER=steam\nAPP_ID=10\nDISPLAY_NAME=Game\n".into(),
        })
        .expect("valid");

        assert_eq!(response.profile.media_id, "GAME-1");
        assert_eq!(response.profile.app_id, Some(10));
        assert!(response.warnings.is_empty());
    }

    #[test]
    fn media_validate_error_does_not_echo_untrusted_content() {
        let error = media_validate(MediaValidateRequest {
            content: "[GAME]\nCOMMAND=super-secret-command".into(),
        })
        .expect_err("forbidden");
        let serialized = serde_json::to_string(&error).expect("serialize");
        assert!(serialized.contains("FORBIDDEN_FIELD"));
        assert!(!serialized.contains("super-secret-command"));
    }
}
