//! Offline local-library scan use case.

use crate::domain::entities::{
    Game, GameProvider, LaunchKind, LaunchProfile, LibraryScanMode, LibraryScanReport,
};
use crate::domain::errors::DomainError;
use crate::domain::ports::{
    GameRepository, LocalLibrarySource, ProfileRepository, SteamCatalogRepository,
};
use chrono::Utc;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

pub struct LibraryScanService {
    source: Arc<dyn LocalLibrarySource>,
    catalog: Arc<dyn SteamCatalogRepository>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualGameRegistration {
    pub display_name: String,
    pub executable_path: String,
    pub working_directory: Option<String>,
    pub arguments: Vec<String>,
    pub process_hints: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileUpdate {
    pub profile_id: crate::domain::entities::ProfileId,
    pub display_name: String,
    pub provider: GameProvider,
    pub app_id: Option<u64>,
    pub steam_launch_option: Option<u8>,
    pub executable_path: Option<String>,
    pub working_directory: Option<String>,
    pub arguments: Vec<String>,
    pub process_hints: Vec<String>,
}

pub struct LibraryService {
    games: Arc<dyn GameRepository>,
    profiles: Arc<dyn ProfileRepository>,
    catalog: Arc<crate::infrastructure::database::catalog::SqliteCatalogRepository>,
}

impl LibraryService {
    pub fn new(
        games: Arc<dyn GameRepository>,
        profiles: Arc<dyn ProfileRepository>,
        catalog: Arc<crate::infrastructure::database::catalog::SqliteCatalogRepository>,
    ) -> Self {
        Self {
            games,
            profiles,
            catalog,
        }
    }

    pub async fn list(&self) -> Result<Vec<Game>, DomainError> {
        self.games.list(0, 2_000).await
    }

    pub async fn find(
        &self,
        id: &crate::domain::entities::GameId,
    ) -> Result<Option<Game>, DomainError> {
        self.games.find_by_id(id).await
    }

    pub async fn profiles(
        &self,
        id: &crate::domain::entities::GameId,
    ) -> Result<Vec<LaunchProfile>, DomainError> {
        self.profiles.find_by_game_id(id).await
    }

    pub async fn register_manual(
        &self,
        request: ManualGameRegistration,
    ) -> Result<Game, DomainError> {
        let display_name = request.display_name.trim();
        if display_name.is_empty()
            || display_name.chars().count() > 160
            || display_name.chars().any(char::is_control)
        {
            return Err(DomainError::LaunchProfileInvalid);
        }

        let executable = canonical_local_executable(&request.executable_path)?;
        let working_directory = match request.working_directory.as_deref() {
            Some(value) if !value.trim().is_empty() => Some(canonical_local_directory(value)?),
            _ => executable
                .parent()
                .map(|path| path.to_string_lossy().into_owned()),
        };
        let now = Utc::now();
        let mut game = Game::new(GameProvider::Executable, None, display_name, now);
        game.install_dir = executable
            .parent()
            .map(|path| path.to_string_lossy().into_owned());
        game.installed = true;
        let profile_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Executable");
        let profile = LaunchProfile::new(game.id, profile_name, LaunchKind::Executable, now)
            .with_executable_target(
                executable.to_string_lossy().into_owned(),
                working_directory,
                request.arguments,
                request.process_hints,
            )?;
        self.catalog
            .upsert_game_and_profile(&game, &profile)
            .await?;
        Ok(game)
    }

    pub async fn update_profile(&self, request: ProfileUpdate) -> Result<Game, DomainError> {
        let mut profile = self
            .profiles
            .find_by_id(&request.profile_id)
            .await?
            .ok_or(DomainError::LaunchProfileInvalid)?;
        let mut game = self
            .games
            .find_by_id(&profile.game_id)
            .await?
            .ok_or(DomainError::GameNotInstalled)?;
        let display_name = request.display_name.trim();
        if display_name.is_empty()
            || display_name.chars().count() > 160
            || display_name.chars().any(char::is_control)
        {
            return Err(DomainError::LaunchProfileInvalid);
        }
        game.display_name = display_name.to_owned();
        game.sort_name = display_name.to_lowercase();
        game.updated_at = Utc::now();
        match request.provider {
            GameProvider::Executable => {
                let executable = canonical_local_executable(
                    request
                        .executable_path
                        .as_deref()
                        .ok_or(DomainError::LaunchProfileInvalid)?,
                )?;
                profile.executable_path = Some(executable.to_string_lossy().into_owned());
                profile.working_directory = match request.working_directory.as_deref() {
                    Some(value) if !value.trim().is_empty() => {
                        Some(canonical_local_directory(value)?)
                    }
                    _ => executable
                        .parent()
                        .map(|path| path.to_string_lossy().into_owned()),
                };
                profile.arguments = request.arguments;
                profile.launch_kind = LaunchKind::Executable;
                profile.steam_launch_option = None;
                game.provider = GameProvider::Executable;
                game.provider_game_id = None;
                game.install_dir = executable
                    .parent()
                    .map(|path| path.to_string_lossy().into_owned());
            }
            GameProvider::Steam => {
                let app_id = request
                    .app_id
                    .filter(|value| *value > 0)
                    .ok_or(DomainError::LaunchProfileInvalid)?;
                if request.executable_path.is_some()
                    || request.working_directory.is_some()
                    || !request.arguments.is_empty()
                {
                    return Err(DomainError::LaunchProfileInvalid);
                }
                profile.launch_kind = LaunchKind::Steam { app_id };
                profile.steam_launch_option = request.steam_launch_option;
                profile.executable_path = None;
                profile.working_directory = None;
                profile.arguments.clear();
                game.provider = GameProvider::Steam;
                game.provider_game_id = Some(app_id.to_string());
                game.install_dir = None;
            }
        }
        profile.process_hints = request.process_hints;
        profile.updated_at = Utc::now();
        profile.validate()?;
        self.catalog
            .upsert_game_and_profile(&game, &profile)
            .await?;
        Ok(game)
    }
}

#[cfg(windows)]
fn canonical_local_executable(raw: &str) -> Result<std::path::PathBuf, DomainError> {
    crate::infrastructure::processes::validate_launch_target(raw)
}

#[cfg(not(windows))]
fn canonical_local_executable(raw: &str) -> Result<std::path::PathBuf, DomainError> {
    let path = normalize_windows_path(
        std::fs::canonicalize(raw).map_err(|_| DomainError::LaunchProfileInvalid)?,
    );
    if !path.is_file()
        || path.to_string_lossy().starts_with(r"\\")
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
    {
        return Err(DomainError::LaunchProfileInvalid);
    }
    Ok(path)
}

fn canonical_local_directory(raw: &str) -> Result<String, DomainError> {
    let path = normalize_windows_path(
        std::fs::canonicalize(Path::new(raw)).map_err(|_| DomainError::LaunchProfileInvalid)?,
    );
    if !path.is_dir() || path.to_string_lossy().starts_with(r"\\") {
        return Err(DomainError::LaunchProfileInvalid);
    }
    Ok(path.to_string_lossy().into_owned())
}

fn normalize_windows_path(path: std::path::PathBuf) -> std::path::PathBuf {
    let value = path.to_string_lossy();
    value
        .strip_prefix(r"\\?\")
        .map(std::path::PathBuf::from)
        .unwrap_or(path)
}

impl LibraryScanService {
    pub fn new(
        source: Arc<dyn LocalLibrarySource>,
        catalog: Arc<dyn SteamCatalogRepository>,
    ) -> Self {
        Self { source, catalog }
    }

    pub async fn scan(&self, mode: LibraryScanMode) -> Result<LibraryScanReport, DomainError> {
        let snapshot = self.source.scan(mode).await?;
        self.catalog.reconcile(&snapshot, mode).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::catalog::SqliteCatalogRepository;
    use crate::infrastructure::database::games::SqliteGameRepository;
    use crate::infrastructure::database::open_and_migrate;
    use tempfile::tempdir;

    #[cfg(windows)]
    #[tokio::test]
    async fn manual_registration_persists_structured_profile() {
        let directory = tempdir().unwrap();
        let pool = open_and_migrate(&directory.path().join("library.db"))
            .await
            .unwrap();
        let executable = directory.path().join("fixture.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let catalog = Arc::new(SqliteCatalogRepository::new(pool.clone()));
        let service = LibraryService::new(
            Arc::new(SqliteGameRepository::new(pool)),
            catalog.clone(),
            catalog.clone(),
        );
        let game = service
            .register_manual(ManualGameRegistration {
                display_name: "Local fixture".into(),
                executable_path: executable.to_string_lossy().into_owned(),
                working_directory: None,
                arguments: vec!["-windowed".into()],
                process_hints: vec!["fixture.exe".into()],
            })
            .await
            .unwrap();
        let profiles = service.profiles(&game.id).await.unwrap();
        assert_eq!(profiles[0].arguments, ["-windowed"]);
        assert_eq!(profiles[0].process_hints, ["fixture.exe"]);
        service
            .update_profile(ProfileUpdate {
                profile_id: profiles[0].id,
                display_name: "Edited local fixture".into(),
                provider: GameProvider::Executable,
                app_id: None,
                steam_launch_option: None,
                executable_path: Some(executable.to_string_lossy().into_owned()),
                working_directory: None,
                arguments: vec!["-safe".into()],
                process_hints: vec!["fixture.exe".into()],
            })
            .await
            .unwrap();
        let updated_game = service.find(&game.id).await.unwrap().unwrap();
        let updated_profiles = service.profiles(&game.id).await.unwrap();
        assert_eq!(updated_game.display_name, "Edited local fixture");
        assert_eq!(updated_profiles[0].arguments, ["-safe"]);
    }
}
