use crate::application::artwork::{ArtworkImport, ArtworkService};
use crate::application::library::{
    LibraryScanService, LibraryService, ManualGameRegistration, ProfileUpdate,
};
use crate::domain::artwork::{ArtworkKind, ArtworkOrigin};
use crate::domain::entities::{
    Game, GameId, GameProvider, LaunchKind, LibraryScanMode, LibraryScanReport, ProfileId,
};
use crate::domain::errors::{AppError, DomainError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

pub struct LibraryState {
    pub scan: Arc<LibraryScanService>,
    pub library: Arc<LibraryService>,
    pub artwork: Arc<ArtworkService>,
}

#[derive(Debug, Serialize)]
pub struct LibraryEntryDto {
    pub id: String,
    pub provider: GameProvider,
    pub provider_game_id: Option<String>,
    pub display_name: String,
    pub install_dir: Option<String>,
    pub installed: bool,
    pub offline: bool,
}

impl From<Game> for LibraryEntryDto {
    fn from(game: Game) -> Self {
        let offline = game
            .metadata_json
            .pointer("/_steam_scan/library_offline")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        Self {
            id: game.id.to_string(),
            provider: game.provider,
            provider_game_id: game.provider_game_id,
            display_name: game.display_name,
            install_dir: game.install_dir,
            installed: game.installed,
            offline,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LibraryScanDto {
    pub discovered: usize,
    pub inserted: usize,
    pub updated: usize,
    pub marked_uninstalled: usize,
    pub unavailable_libraries: usize,
}

impl From<LibraryScanReport> for LibraryScanDto {
    fn from(report: LibraryScanReport) -> Self {
        Self {
            discovered: report.discovered,
            inserted: report.inserted,
            updated: report.updated,
            marked_uninstalled: report.marked_uninstalled,
            unavailable_libraries: report.unavailable_libraries,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualRegistrationRequest {
    pub display_name: String,
    pub executable_path: String,
    pub working_directory: Option<String>,
    pub arguments: Vec<String>,
    pub process_hints: Vec<String>,
    pub cover_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileUpdateRequest {
    pub profile_id: String,
    pub display_name: String,
    pub provider: GameProvider,
    pub app_id: Option<u64>,
    pub steam_launch_option: Option<u8>,
    pub executable_path: Option<String>,
    pub working_directory: Option<String>,
    pub arguments: Vec<String>,
    pub process_hints: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverUpdateRequest {
    pub game_id: String,
    pub cover_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutReviewRequest {
    pub shortcut_path: String,
}

#[derive(Debug, Serialize)]
pub struct ShortcutReviewDto {
    pub target_path: String,
    pub working_directory: Option<String>,
    pub arguments: String,
    pub resolved: bool,
}

#[derive(Debug, Serialize)]
pub struct LaunchProfileDto {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub steam_launch_option: Option<u8>,
    pub executable_path: Option<String>,
    pub working_directory: Option<String>,
    pub arguments: Vec<String>,
    pub process_hints: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LibraryDetailsDto {
    pub game: LibraryEntryDto,
    pub profiles: Vec<LaunchProfileDto>,
    pub active_cover: Option<ActiveCoverDto>,
}

#[derive(Debug, Serialize)]
pub struct ActiveCoverDto {
    pub id: String,
    pub relative_path: String,
}

#[tauri::command]
pub async fn library_scan_steam(
    state: State<'_, LibraryState>,
) -> Result<LibraryScanDto, AppError> {
    state
        .scan
        .scan(LibraryScanMode::Full)
        .await
        .map(LibraryScanDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn library_list(
    state: State<'_, LibraryState>,
) -> Result<Vec<LibraryEntryDto>, AppError> {
    state
        .library
        .list()
        .await
        .map(|games| games.into_iter().map(LibraryEntryDto::from).collect())
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn library_get(
    game_id: String,
    state: State<'_, LibraryState>,
) -> Result<LibraryDetailsDto, AppError> {
    let id = parse_game_id(&game_id)?;
    let game = state
        .library
        .find(&id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::from(DomainError::GameNotInstalled))?;
    let profiles = state
        .library
        .profiles(&id)
        .await
        .map_err(AppError::from)?
        .into_iter()
        .map(|profile| LaunchProfileDto {
            id: profile.id.to_string(),
            name: profile.name,
            kind: match profile.launch_kind {
                LaunchKind::Steam { .. } => "steam",
                LaunchKind::Executable => "executable",
            },
            steam_launch_option: profile.steam_launch_option,
            executable_path: profile.executable_path,
            working_directory: profile.working_directory,
            arguments: profile.arguments,
            process_hints: profile.process_hints,
        })
        .collect();
    let active_cover = state
        .artwork
        .resolve_active(&id, ArtworkKind::LauncherCover)
        .await
        .map_err(AppError::from)?
        .map(|resolved| ActiveCoverDto {
            id: resolved.artwork.id.to_string(),
            relative_path: resolved.artwork.relative_path,
        });
    Ok(LibraryDetailsDto {
        game: game.into(),
        profiles,
        active_cover,
    })
}

#[tauri::command]
pub async fn library_register_executable(
    request: ManualRegistrationRequest,
    state: State<'_, LibraryState>,
) -> Result<LibraryEntryDto, AppError> {
    let cover_path = request.cover_path.clone();
    let game = state
        .library
        .register_manual(ManualGameRegistration {
            display_name: request.display_name,
            executable_path: request.executable_path,
            working_directory: request.working_directory,
            arguments: request.arguments,
            process_hints: request.process_hints,
        })
        .await
        .map_err(AppError::from)?;

    if let Some(path) = cover_path.filter(|path| !path.trim().is_empty()) {
        let mime_type = image_mime(&path)?;
        let canonical = std::fs::canonicalize(&path)
            .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
        let canonical_text = canonical.to_string_lossy();
        let local_text = canonical_text
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical_text);
        if local_text.starts_with(r"\\")
            || std::fs::metadata(&canonical)
                .map(|metadata| !metadata.is_file() || metadata.len() > 16 * 1024 * 1024)
                .unwrap_or(true)
        {
            return Err(AppError::from(DomainError::ArtworkInvalid));
        }
        let bytes =
            std::fs::read(canonical).map_err(|_| AppError::from(DomainError::ArtworkIoFailed))?;
        state
            .artwork
            .import(ArtworkImport {
                game_id: game.id,
                kind: ArtworkKind::LauncherCover,
                origin: ArtworkOrigin::Manual,
                bytes,
                mime_type,
                source_url: None,
                is_user_override: true,
            })
            .await
            .map_err(AppError::from)?;
    }
    Ok(game.into())
}

#[tauri::command]
pub async fn library_update_profile(
    request: ProfileUpdateRequest,
    state: State<'_, LibraryState>,
) -> Result<LibraryEntryDto, AppError> {
    state
        .library
        .update_profile(ProfileUpdate {
            profile_id: request
                .profile_id
                .parse::<ProfileId>()
                .map_err(|_| AppError::from(DomainError::LaunchProfileInvalid))?,
            display_name: request.display_name,
            provider: request.provider,
            app_id: request.app_id,
            steam_launch_option: request.steam_launch_option,
            executable_path: request.executable_path,
            working_directory: request.working_directory,
            arguments: request.arguments,
            process_hints: request.process_hints,
        })
        .await
        .map(LibraryEntryDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn library_set_cover(
    request: CoverUpdateRequest,
    state: State<'_, LibraryState>,
) -> Result<(), AppError> {
    let game_id = parse_game_id(&request.game_id)?;
    let mime_type = image_mime(&request.cover_path)?;
    let canonical = std::fs::canonicalize(&request.cover_path)
        .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
    let canonical_text = canonical.to_string_lossy();
    let local_text = canonical_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical_text);
    if local_text.starts_with(r"\\")
        || std::fs::metadata(&canonical)
            .map(|metadata| !metadata.is_file() || metadata.len() > 16 * 1024 * 1024)
            .unwrap_or(true)
    {
        return Err(AppError::from(DomainError::ArtworkInvalid));
    }
    let bytes =
        std::fs::read(canonical).map_err(|_| AppError::from(DomainError::ArtworkIoFailed))?;
    state
        .artwork
        .import(ArtworkImport {
            game_id,
            kind: ArtworkKind::LauncherCover,
            origin: ArtworkOrigin::Manual,
            bytes,
            mime_type,
            source_url: None,
            is_user_override: true,
        })
        .await
        .map(|_| ())
        .map_err(AppError::from)
}

#[tauri::command]
pub fn library_review_shortcut(
    request: ShortcutReviewRequest,
) -> Result<ShortcutReviewDto, AppError> {
    #[cfg(windows)]
    {
        let review = crate::infrastructure::windows::inspect_shortcut(&request.shortcut_path)
            .map_err(AppError::from)?;
        Ok(ShortcutReviewDto {
            target_path: review.target_path,
            working_directory: review.working_directory,
            arguments: review.arguments,
            resolved: true,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        Err(AppError::from(DomainError::LaunchProfileInvalid))
    }
}

fn parse_game_id(raw: &str) -> Result<GameId, AppError> {
    raw.parse()
        .map_err(|_| AppError::from(DomainError::GameNotInstalled))
}

fn image_mime(path: &str) -> Result<String, AppError> {
    match std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Ok("image/png".to_owned()),
        Some("jpg" | "jpeg") => Ok("image/jpeg".to_owned()),
        Some("webp") => Ok("image/webp".to_owned()),
        _ => Err(AppError::from(DomainError::ArtworkInvalid)),
    }
}
