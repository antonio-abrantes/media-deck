//! SQLite repositories for launch profiles and physical media.

use crate::domain::entities::{
    ContentHash, DeviceId, Game, GameId, LaunchKind, LaunchProfile, LibraryScanMode,
    LibraryScanReport, LibraryScanSnapshot, MediaDescriptor, MediaKey, MediaKind, MediaStatus,
    ProfileId,
};
use crate::domain::errors::DomainError;
use crate::domain::ports::{
    MediaRepository, PortResult, ProfileRepository, SteamCatalogRepository,
};
use chrono::{DateTime, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

pub struct SqliteCatalogRepository {
    pool: SqlitePool,
}

impl SqliteCatalogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_profile_by_id(
        &self,
        id: &ProfileId,
    ) -> Result<Option<LaunchProfile>, DomainError> {
        let row = sqlx::query(
            "SELECT p.id, p.game_id, p.name, p.launch_kind, p.executable_path,
                    p.working_directory, p.arguments_json, p.process_hints_json,
                    p.close_policy_override, p.steam_launch_option, p.enabled,
                    p.created_at, p.updated_at,
                    g.provider_game_id
             FROM launch_profiles p
             JOIN games g ON g.id = p.game_id
             WHERE p.id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;

        row.map(|row| map_profile_row(&row)).transpose()
    }

    pub async fn find_profiles_by_game_id(
        &self,
        game_id: &GameId,
    ) -> Result<Vec<LaunchProfile>, DomainError> {
        let rows = sqlx::query(
            "SELECT p.id, p.game_id, p.name, p.launch_kind, p.executable_path,
                    p.working_directory, p.arguments_json, p.process_hints_json,
                    p.close_policy_override, p.steam_launch_option, p.enabled,
                    p.created_at, p.updated_at,
                    g.provider_game_id
             FROM launch_profiles p
             JOIN games g ON g.id = p.game_id
             WHERE p.game_id = ?
             ORDER BY p.name",
        )
        .bind(game_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;

        rows.iter().map(map_profile_row).collect()
    }

    pub async fn upsert_profile(&self, profile: &LaunchProfile) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        upsert_profile_tx(&mut tx, profile).await?;
        tx.commit().await.map_err(db_error)
    }

    pub async fn find_media_by_key(
        &self,
        key: &str,
    ) -> Result<Option<MediaDescriptor>, DomainError> {
        let row = sqlx::query(
            "SELECT id, media_key, profile_id, media_kind, last_device_id,
                    schema_version, content_hash, volume_serial, last_drive,
                    created_at, last_seen_at, last_verified_at, status
             FROM media WHERE media_key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;

        row.map(|row| map_media_row(&row)).transpose()
    }

    pub async fn list_media(&self) -> Result<Vec<MediaDescriptor>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, media_key, profile_id, media_kind, last_device_id,
                    schema_version, content_hash, volume_serial, last_drive,
                    created_at, last_seen_at, last_verified_at, status
             FROM media ORDER BY COALESCE(last_verified_at, created_at) DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.iter().map(map_media_row).collect()
    }

    pub async fn upsert_media(&self, media: &MediaDescriptor) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO media
                (id, media_key, profile_id, media_kind, last_device_id,
                 schema_version, content_hash, volume_serial, last_drive,
                 created_at, last_seen_at, last_verified_at, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                media_key        = excluded.media_key,
                profile_id       = excluded.profile_id,
                media_kind       = excluded.media_kind,
                last_device_id   = excluded.last_device_id,
                schema_version   = excluded.schema_version,
                content_hash     = excluded.content_hash,
                volume_serial    = excluded.volume_serial,
                last_drive       = excluded.last_drive,
                last_seen_at     = excluded.last_seen_at,
                last_verified_at = excluded.last_verified_at,
                status           = excluded.status",
        )
        .bind(media.id.to_string())
        .bind(media.media_key.as_str())
        .bind(media.profile_id.to_string())
        .bind(media_kind_to_str(&media.media_kind))
        .bind(media.last_device_id.map(|id| id.to_string()))
        .bind(i64::from(media.schema_version))
        .bind(media.content_hash.as_str())
        .bind(&media.volume_serial)
        .bind(&media.last_drive)
        .bind(media.created_at.to_rfc3339())
        .bind(media.last_seen_at.map(|value| value.to_rfc3339()))
        .bind(media.last_verified_at.map(|value| value.to_rfc3339()))
        .bind(media_status_to_str(&media.status))
        .execute(&self.pool)
        .await
        .map_err(db_error)?;

        Ok(())
    }

    /// Persist a discovered game and its profile as one atomic unit.
    pub async fn upsert_game_and_profile(
        &self,
        game: &Game,
        profile: &LaunchProfile,
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        upsert_game_tx(&mut tx, game).await?;
        upsert_profile_tx(&mut tx, profile).await?;
        tx.commit().await.map_err(db_error)
    }

    /// Reconcile one local Steam snapshot atomically. Existing launch profiles
    /// are never updated by discovery.
    pub async fn reconcile_steam_snapshot(
        &self,
        snapshot: &LibraryScanSnapshot,
        mode: LibraryScanMode,
    ) -> Result<LibraryScanReport, DomainError> {
        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let mut report = LibraryScanReport {
            discovered: snapshot.games.len(),
            unavailable_libraries: snapshot.unavailable_libraries.len(),
            ..LibraryScanReport::default()
        };
        let mut discovered_ids = std::collections::HashSet::new();

        for discovered in &snapshot.games {
            let provider_id = discovered
                .provider_game_id
                .as_deref()
                .ok_or(DomainError::SteamLibraryInvalid)?;
            discovered_ids.insert(provider_id.to_owned());
            let existing = find_steam_game_tx(&mut tx, provider_id).await?;
            match existing {
                Some(existing) => {
                    let merged = merge_scanned_game(existing, discovered);
                    upsert_game_tx(&mut tx, &merged).await?;
                    report.updated += 1;
                }
                None => {
                    upsert_game_tx(&mut tx, discovered).await?;
                    let app_id = provider_id
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or(DomainError::SteamLibraryInvalid)?;
                    let profile = LaunchProfile::new(
                        discovered.id,
                        "Steam",
                        LaunchKind::Steam { app_id },
                        discovered.created_at,
                    );
                    upsert_profile_tx(&mut tx, &profile).await?;
                    report.inserted += 1;
                }
            }
        }

        if mode == LibraryScanMode::Full {
            let existing = list_steam_games_tx(&mut tx).await?;
            for mut game in existing {
                let Some(provider_id) = game.provider_game_id.as_deref() else {
                    continue;
                };
                if discovered_ids.contains(provider_id) || !game.installed {
                    continue;
                }
                let source_library = steam_source_library(&game.metadata_json);
                let unavailable = source_library.is_some_and(|source| {
                    snapshot
                        .unavailable_libraries
                        .iter()
                        .any(|library| library.eq_ignore_ascii_case(source))
                });
                if unavailable {
                    set_library_offline(&mut game, true);
                    game.updated_at = Utc::now();
                    upsert_game_tx(&mut tx, &game).await?;
                    continue;
                }
                set_library_offline(&mut game, false);
                game.installed = false;
                game.updated_at = Utc::now();
                upsert_game_tx(&mut tx, &game).await?;
                report.marked_uninstalled += 1;
            }
        }

        tx.commit().await.map_err(db_error)?;
        Ok(report)
    }
}

impl ProfileRepository for SqliteCatalogRepository {
    fn find_by_id<'a>(&'a self, id: &'a ProfileId) -> PortResult<'a, Option<LaunchProfile>> {
        Box::pin(SqliteCatalogRepository::find_profile_by_id(self, id))
    }

    fn find_by_game_id<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, Vec<LaunchProfile>> {
        Box::pin(SqliteCatalogRepository::find_profiles_by_game_id(
            self, game_id,
        ))
    }

    fn upsert<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()> {
        Box::pin(SqliteCatalogRepository::upsert_profile(self, profile))
    }
}

impl MediaRepository for SqliteCatalogRepository {
    fn find_by_key<'a>(&'a self, key: &'a str) -> PortResult<'a, Option<MediaDescriptor>> {
        Box::pin(SqliteCatalogRepository::find_media_by_key(self, key))
    }

    fn list(&self) -> PortResult<'_, Vec<MediaDescriptor>> {
        Box::pin(SqliteCatalogRepository::list_media(self))
    }

    fn upsert<'a>(&'a self, media: &'a MediaDescriptor) -> PortResult<'a, ()> {
        Box::pin(SqliteCatalogRepository::upsert_media(self, media))
    }
}

impl SteamCatalogRepository for SqliteCatalogRepository {
    fn reconcile<'a>(
        &'a self,
        snapshot: &'a LibraryScanSnapshot,
        mode: LibraryScanMode,
    ) -> PortResult<'a, LibraryScanReport> {
        Box::pin(self.reconcile_steam_snapshot(snapshot, mode))
    }
}

async fn find_steam_game_tx(
    tx: &mut Transaction<'_, Sqlite>,
    provider_id: &str,
) -> Result<Option<Game>, DomainError> {
    let row = sqlx::query(
        "SELECT id, provider, provider_game_id, display_name, sort_name,
                install_dir, installed, metadata_json, source_updated_at,
                created_at, updated_at
         FROM games WHERE provider = 'steam' AND provider_game_id = ?",
    )
    .bind(provider_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?;
    row.as_ref()
        .map(crate::infrastructure::database::games::map_row)
        .transpose()
}

async fn list_steam_games_tx(tx: &mut Transaction<'_, Sqlite>) -> Result<Vec<Game>, DomainError> {
    let rows = sqlx::query(
        "SELECT id, provider, provider_game_id, display_name, sort_name,
                install_dir, installed, metadata_json, source_updated_at,
                created_at, updated_at
         FROM games WHERE provider = 'steam'",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(db_error)?;
    rows.iter()
        .map(crate::infrastructure::database::games::map_row)
        .collect()
}

fn merge_scanned_game(mut existing: Game, discovered: &Game) -> Game {
    let overrides = existing
        .metadata_json
        .get("manual_overrides")
        .and_then(serde_json::Value::as_array);
    let has_override = |field: &str| {
        overrides.is_some_and(|values| values.iter().any(|value| value.as_str() == Some(field)))
    };
    if !has_override("display_name") {
        existing.display_name.clone_from(&discovered.display_name);
        existing.sort_name.clone_from(&discovered.sort_name);
    }
    if !has_override("install_dir") {
        existing.install_dir.clone_from(&discovered.install_dir);
    }
    let scan_metadata = discovered.metadata_json.get("_steam_scan").cloned();
    if !existing.metadata_json.is_object() {
        existing.metadata_json = serde_json::json!({});
    }
    if let (Some(object), Some(scan_metadata)) =
        (existing.metadata_json.as_object_mut(), scan_metadata)
    {
        let mut scan_metadata = scan_metadata;
        if let Some(scan) = scan_metadata.as_object_mut() {
            scan.insert("library_offline".to_owned(), serde_json::Value::Bool(false));
        }
        object.insert("_steam_scan".to_owned(), scan_metadata);
    }
    existing.installed = discovered.installed;
    existing.source_updated_at = discovered.source_updated_at;
    existing.updated_at = discovered.updated_at;
    existing
}

fn set_library_offline(game: &mut Game, offline: bool) {
    if let Some(scan) = game
        .metadata_json
        .get_mut("_steam_scan")
        .and_then(serde_json::Value::as_object_mut)
    {
        scan.insert(
            "library_offline".to_owned(),
            serde_json::Value::Bool(offline),
        );
    }
}

fn steam_source_library(metadata: &serde_json::Value) -> Option<&str> {
    metadata
        .get("_steam_scan")
        .and_then(|value| value.get("source_library"))
        .and_then(serde_json::Value::as_str)
}

async fn upsert_game_tx(tx: &mut Transaction<'_, Sqlite>, game: &Game) -> Result<(), DomainError> {
    let metadata =
        serde_json::to_string(&game.metadata_json).map_err(|error| db_error(error.to_string()))?;
    sqlx::query(
        "INSERT INTO games
            (id, provider, provider_game_id, display_name, sort_name, install_dir,
             installed, metadata_json, source_updated_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            provider_game_id = excluded.provider_game_id,
            display_name = excluded.display_name,
            sort_name = excluded.sort_name,
            install_dir = excluded.install_dir,
            installed = excluded.installed,
            metadata_json = excluded.metadata_json,
            source_updated_at = excluded.source_updated_at,
            updated_at = excluded.updated_at",
    )
    .bind(game.id.to_string())
    .bind(game.provider.to_string())
    .bind(&game.provider_game_id)
    .bind(&game.display_name)
    .bind(&game.sort_name)
    .bind(&game.install_dir)
    .bind(game.installed)
    .bind(metadata)
    .bind(game.source_updated_at.map(|value| value.to_rfc3339()))
    .bind(game.created_at.to_rfc3339())
    .bind(game.updated_at.to_rfc3339())
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn upsert_profile_tx(
    tx: &mut Transaction<'_, Sqlite>,
    profile: &LaunchProfile,
) -> Result<(), DomainError> {
    validate_profile_target(tx, profile).await?;
    let arguments =
        serde_json::to_string(&profile.arguments).map_err(|error| db_error(error.to_string()))?;
    let process_hints = serde_json::to_string(&profile.process_hints)
        .map_err(|error| db_error(error.to_string()))?;
    let close_policy = profile
        .close_policy_override
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| db_error(error.to_string()))?;

    sqlx::query(
        "INSERT INTO launch_profiles
            (id, game_id, name, launch_kind, executable_path, working_directory,
             arguments_json, process_hints_json, close_policy_override,
             steam_launch_option, enabled, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            game_id = excluded.game_id,
            name = excluded.name,
            launch_kind = excluded.launch_kind,
            executable_path = excluded.executable_path,
            working_directory = excluded.working_directory,
            arguments_json = excluded.arguments_json,
            process_hints_json = excluded.process_hints_json,
            close_policy_override = excluded.close_policy_override,
            steam_launch_option = excluded.steam_launch_option,
            enabled = excluded.enabled,
            updated_at = excluded.updated_at",
    )
    .bind(profile.id.to_string())
    .bind(profile.game_id.to_string())
    .bind(&profile.name)
    .bind(match profile.launch_kind {
        LaunchKind::Steam { .. } => "steam",
        LaunchKind::Executable => "executable",
    })
    .bind(&profile.executable_path)
    .bind(&profile.working_directory)
    .bind(arguments)
    .bind(process_hints)
    .bind(close_policy)
    .bind(profile.steam_launch_option)
    .bind(profile.enabled)
    .bind(profile.created_at.to_rfc3339())
    .bind(profile.updated_at.to_rfc3339())
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn validate_profile_target(
    tx: &mut Transaction<'_, Sqlite>,
    profile: &LaunchProfile,
) -> Result<(), DomainError> {
    profile.validate()?;
    let game = sqlx::query("SELECT provider, provider_game_id FROM games WHERE id = ?")
        .bind(profile.game_id.to_string())
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_error)?
        .ok_or(DomainError::LaunchProfileInvalid)?;
    let provider: String = game.try_get("provider").map_err(db_error)?;
    let provider_game_id: Option<String> = game.try_get("provider_game_id").map_err(db_error)?;

    let valid_target = match profile.launch_kind {
        LaunchKind::Steam { app_id } => {
            let expected = app_id.to_string();
            provider == "steam" && provider_game_id.as_deref() == Some(expected.as_str())
        }
        LaunchKind::Executable => provider == "executable" && provider_game_id.is_none(),
    };
    if !valid_target {
        return Err(DomainError::LaunchProfileInvalid);
    }
    Ok(())
}

fn map_profile_row(row: &sqlx::sqlite::SqliteRow) -> Result<LaunchProfile, DomainError> {
    let id: ProfileId = parse_id(row.try_get::<String, _>("id").map_err(db_error)?, "profile")?;
    let game_id: GameId = parse_id(
        row.try_get::<String, _>("game_id").map_err(db_error)?,
        "game",
    )?;
    let launch_kind = match row
        .try_get::<String, _>("launch_kind")
        .map_err(db_error)?
        .as_str()
    {
        "steam" => {
            let app_id = row
                .try_get::<Option<String>, _>("provider_game_id")
                .map_err(db_error)?
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| db_error("Steam profile has no valid AppID"))?;
            LaunchKind::Steam { app_id }
        }
        "executable" => LaunchKind::Executable,
        value => return Err(db_error(format!("unknown launch kind: {value}"))),
    };

    let profile = LaunchProfile {
        id,
        game_id,
        name: row.try_get("name").map_err(db_error)?,
        launch_kind,
        steam_launch_option: row
            .try_get::<Option<i64>, _>("steam_launch_option")
            .map_err(db_error)?
            .map(u8::try_from)
            .transpose()
            .map_err(|_| db_error("invalid Steam launch option"))?,
        executable_path: row.try_get("executable_path").map_err(db_error)?,
        working_directory: row.try_get("working_directory").map_err(db_error)?,
        arguments: parse_json(row.try_get("arguments_json").map_err(db_error)?)?,
        process_hints: parse_json(row.try_get("process_hints_json").map_err(db_error)?)?,
        close_policy_override: row
            .try_get::<Option<String>, _>("close_policy_override")
            .map_err(db_error)?
            .map(parse_json)
            .transpose()?,
        enabled: row.try_get::<i64, _>("enabled").map_err(db_error)? != 0,
        created_at: parse_datetime(row.try_get("created_at").map_err(db_error)?)?,
        updated_at: parse_datetime(row.try_get("updated_at").map_err(db_error)?)?,
    };
    profile.validate()?;
    Ok(profile)
}

fn map_media_row(row: &sqlx::sqlite::SqliteRow) -> Result<MediaDescriptor, DomainError> {
    let media_kind = match row
        .try_get::<String, _>("media_kind")
        .map_err(db_error)?
        .as_str()
    {
        "floppy" => MediaKind::Floppy,
        "optical" => MediaKind::Optical,
        "removable" => MediaKind::Removable,
        value => return Err(db_error(format!("unknown media kind: {value}"))),
    };
    let status = match row
        .try_get::<String, _>("status")
        .map_err(db_error)?
        .as_str()
    {
        "active" => MediaStatus::Active,
        "damaged" => MediaStatus::Damaged,
        "replaced" => MediaStatus::Replaced,
        "missing" => MediaStatus::Missing,
        value => return Err(db_error(format!("unknown media status: {value}"))),
    };
    let schema_version = row.try_get::<i64, _>("schema_version").map_err(db_error)?;

    Ok(MediaDescriptor {
        id: parse_id(row.try_get::<String, _>("id").map_err(db_error)?, "media")?,
        media_key: MediaKey::new(row.try_get::<String, _>("media_key").map_err(db_error)?)?,
        profile_id: parse_id(
            row.try_get::<String, _>("profile_id").map_err(db_error)?,
            "profile",
        )?,
        media_kind,
        last_device_id: row
            .try_get::<Option<String>, _>("last_device_id")
            .map_err(db_error)?
            .map(|value| parse_id::<DeviceId>(value, "device"))
            .transpose()?,
        schema_version: u32::try_from(schema_version)
            .map_err(|_| db_error("schema version is outside u32 range"))?,
        content_hash: ContentHash::new(row.try_get::<String, _>("content_hash").map_err(db_error)?)
            .map_err(|_| db_error("invalid media content hash"))?,
        volume_serial: row.try_get("volume_serial").map_err(db_error)?,
        last_drive: row.try_get("last_drive").map_err(db_error)?,
        created_at: parse_datetime(row.try_get("created_at").map_err(db_error)?)?,
        last_seen_at: parse_optional_datetime(row.try_get("last_seen_at").map_err(db_error)?)?,
        last_verified_at: parse_optional_datetime(
            row.try_get("last_verified_at").map_err(db_error)?,
        )?,
        status,
    })
}

fn parse_id<T>(value: String, label: &str) -> Result<T, DomainError>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| db_error(format!("invalid {label} ID")))
}

fn parse_json<T: serde::de::DeserializeOwned>(value: String) -> Result<T, DomainError> {
    serde_json::from_str(&value).map_err(|error| db_error(error.to_string()))
}

fn parse_datetime(value: String) -> Result<DateTime<Utc>, DomainError> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| db_error(error.to_string()))
}

fn parse_optional_datetime(value: Option<String>) -> Result<Option<DateTime<Utc>>, DomainError> {
    value.map(parse_datetime).transpose()
}

fn media_kind_to_str(kind: &MediaKind) -> &'static str {
    match kind {
        MediaKind::Floppy => "floppy",
        MediaKind::Optical => "optical",
        MediaKind::Removable => "removable",
    }
}

fn media_status_to_str(status: &MediaStatus) -> &'static str {
    match status {
        MediaStatus::Active => "active",
        MediaStatus::Damaged => "damaged",
        MediaStatus::Replaced => "replaced",
        MediaStatus::Missing => "missing",
    }
}

fn db_error(error: impl std::fmt::Display) -> DomainError {
    DomainError::DatabaseOperationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{GameProvider, MediaId, MediaStatus};
    use crate::infrastructure::database::open_and_migrate;
    use tempfile::tempdir;

    async fn repository() -> SqliteCatalogRepository {
        let directory = tempdir().expect("tempdir");
        let pool = open_and_migrate(&directory.path().join("catalog.db"))
            .await
            .expect("migrate");
        std::mem::forget(directory);
        SqliteCatalogRepository::new(pool)
    }

    #[tokio::test]
    async fn game_and_profile_transaction_round_trip() {
        let repository = repository().await;
        let now = Utc::now();
        let game = Game::new(GameProvider::Steam, Some("440".into()), "TF2", now);
        let profile =
            LaunchProfile::new(game.id, "Default", LaunchKind::Steam { app_id: 440 }, now);

        repository
            .upsert_game_and_profile(&game, &profile)
            .await
            .expect("atomic upsert");

        let found = repository
            .find_profile_by_id(&profile.id)
            .await
            .expect("find")
            .expect("profile exists");
        assert_eq!(found.game_id, game.id);
        assert_eq!(found.launch_kind, LaunchKind::Steam { app_id: 440 });
    }

    #[tokio::test]
    async fn failed_transaction_rolls_back_game() {
        let repository = repository().await;
        let now = Utc::now();
        let game = Game::new(GameProvider::Steam, Some("570".into()), "Dota 2", now);
        let unrelated_game = GameId::new();
        let profile = LaunchProfile::new(
            unrelated_game,
            "Invalid",
            LaunchKind::Steam { app_id: 570 },
            now,
        );

        assert!(repository
            .upsert_game_and_profile(&game, &profile)
            .await
            .is_err());

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games WHERE id = ?")
            .bind(game.id.to_string())
            .fetch_one(&repository.pool)
            .await
            .expect("count");
        assert_eq!(count, 0, "game insert must roll back with profile failure");
    }

    #[tokio::test]
    async fn steam_profile_rejects_mismatched_app_id() {
        let repository = repository().await;
        let now = Utc::now();
        let game = Game::new(GameProvider::Steam, Some("440".into()), "TF2", now);
        let profile = LaunchProfile::new(game.id, "Wrong", LaunchKind::Steam { app_id: 570 }, now);

        let result = repository.upsert_game_and_profile(&game, &profile).await;
        assert_eq!(result, Err(DomainError::LaunchProfileInvalid));

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games WHERE id = ?")
            .bind(game.id.to_string())
            .fetch_one(&repository.pool)
            .await
            .expect("count");
        assert_eq!(count, 0, "invalid profile must roll back game insert");
    }

    #[tokio::test]
    async fn executable_profile_persists_local_target_and_arguments() {
        let repository = repository().await;
        let now = Utc::now();
        let mut game = Game::new(GameProvider::Executable, None, "NFS Underground", now);
        game.install_dir = Some(r"D:\Games\NFSU".to_owned());
        game.installed = true;
        let profile = LaunchProfile::new(game.id, "Speed.exe", LaunchKind::Executable, now)
            .with_executable_target(
                r"D:\Games\NFSU\Speed.exe",
                Some(r"D:\Games\NFSU".to_owned()),
                vec!["-windowed".to_owned()],
                vec!["Speed.exe".to_owned()],
            )
            .expect("valid local target");

        repository
            .upsert_game_and_profile(&game, &profile)
            .await
            .expect("persist executable profile");
        let found = repository
            .find_profile_by_id(&profile.id)
            .await
            .expect("find")
            .expect("profile exists");

        assert_eq!(
            found.executable_path.as_deref(),
            Some(r"D:\Games\NFSU\Speed.exe")
        );
        assert_eq!(found.arguments, ["-windowed"]);
        assert_eq!(found.process_hints, ["Speed.exe"]);
    }

    #[tokio::test]
    async fn executable_profile_rejects_steam_game_target() {
        let repository = repository().await;
        let now = Utc::now();
        let game = Game::new(GameProvider::Steam, Some("620".into()), "Portal 2", now);
        let profile = LaunchProfile::new(game.id, "Wrong provider", LaunchKind::Executable, now)
            .with_executable_target(
                r"D:\Games\Portal2\portal2.exe",
                None,
                Vec::new(),
                vec!["portal2.exe".to_owned()],
            )
            .expect("syntactically valid target");

        assert_eq!(
            repository.upsert_game_and_profile(&game, &profile).await,
            Err(DomainError::LaunchProfileInvalid)
        );
    }

    #[tokio::test]
    async fn media_round_trip() {
        let repository = repository().await;
        let now = Utc::now();
        let game = Game::new(GameProvider::Steam, Some("620".into()), "Portal 2", now);
        let profile =
            LaunchProfile::new(game.id, "Default", LaunchKind::Steam { app_id: 620 }, now);
        repository
            .upsert_game_and_profile(&game, &profile)
            .await
            .expect("catalog");
        let media = MediaDescriptor {
            id: MediaId::new(),
            media_key: MediaKey::new("PORTAL2-001").expect("key"),
            profile_id: profile.id,
            media_kind: MediaKind::Optical,
            last_device_id: None,
            schema_version: 2,
            content_hash: ContentHash::new("a".repeat(64)).expect("hash"),
            volume_serial: Some("1234-ABCD".into()),
            last_drive: Some("G:".into()),
            created_at: now,
            last_seen_at: Some(now),
            last_verified_at: Some(now),
            status: MediaStatus::Active,
        };

        repository.upsert_media(&media).await.expect("upsert media");
        let found = repository
            .find_media_by_key("PORTAL2-001")
            .await
            .expect("find")
            .expect("media exists");
        assert_eq!(found.id, media.id);
        assert_eq!(found.media_kind, MediaKind::Optical);
        assert_eq!(found.status, MediaStatus::Active);
    }

    #[tokio::test]
    async fn steam_reconcile_preserves_manual_overrides_and_profiles() {
        let repository = repository().await;
        let now = Utc::now();
        let mut original = Game::new(GameProvider::Steam, Some("620".into()), "Meu Portal", now);
        original.install_dir = Some(r"E:\Custom\Portal".into());
        original.installed = true;
        original.metadata_json = serde_json::json!({
            "manual_overrides": ["display_name", "install_dir"],
            "notes": "preserve"
        });
        let mut profile = LaunchProfile::new(
            original.id,
            "Minha Steam",
            LaunchKind::Steam { app_id: 620 },
            now,
        );
        profile.close_policy_override = Some(crate::domain::entities::ClosePolicy::Detach);
        repository
            .upsert_game_and_profile(&original, &profile)
            .await
            .expect("seed");

        let mut scanned = Game::new(
            GameProvider::Steam,
            Some("620".into()),
            "Portal 2",
            now + chrono::Duration::minutes(1),
        );
        scanned.install_dir = Some(r"D:\Steam\steamapps\common\Portal 2".into());
        scanned.installed = true;
        scanned.metadata_json = serde_json::json!({
            "_steam_scan": {"source_library": r"D:\Steam", "state_flags": 4}
        });
        repository
            .reconcile_steam_snapshot(
                &LibraryScanSnapshot {
                    games: vec![scanned],
                    configured_libraries: vec![r"D:\Steam".into()],
                    scanned_libraries: vec![r"D:\Steam".into()],
                    unavailable_libraries: Vec::new(),
                },
                LibraryScanMode::Full,
            )
            .await
            .expect("reconcile");

        let found = crate::infrastructure::database::games::SqliteGameRepository::new(
            repository.pool.clone(),
        )
        .find_by_provider_id("steam", "620")
        .await
        .expect("find")
        .expect("game");
        assert_eq!(found.display_name, "Meu Portal");
        assert_eq!(found.install_dir.as_deref(), Some(r"E:\Custom\Portal"));
        assert_eq!(found.metadata_json["notes"], "preserve");
        let found_profile = repository
            .find_profile_by_id(&profile.id)
            .await
            .expect("find profile")
            .expect("profile");
        assert_eq!(
            found_profile.close_policy_override,
            Some(crate::domain::entities::ClosePolicy::Detach)
        );
    }

    #[tokio::test]
    async fn full_scan_marks_absent_accessible_games_but_preserves_offline_library() {
        let repository = repository().await;
        let now = Utc::now();
        for (app_id, library) in [("10", r"D:\Online"), ("20", r"E:\Offline")] {
            let mut game = Game::new(GameProvider::Steam, Some(app_id.into()), app_id, now);
            game.installed = true;
            game.metadata_json = serde_json::json!({"_steam_scan": {"source_library": library}});
            let profile = LaunchProfile::new(
                game.id,
                "Steam",
                LaunchKind::Steam {
                    app_id: app_id.parse().expect("appid"),
                },
                now,
            );
            repository
                .upsert_game_and_profile(&game, &profile)
                .await
                .expect("seed");
        }
        let report = repository
            .reconcile_steam_snapshot(
                &LibraryScanSnapshot {
                    games: Vec::new(),
                    configured_libraries: vec![r"D:\Online".into(), r"E:\Offline".into()],
                    scanned_libraries: vec![r"D:\Online".into()],
                    unavailable_libraries: vec![r"E:\Offline".into()],
                },
                LibraryScanMode::Full,
            )
            .await
            .expect("reconcile");
        assert_eq!(report.marked_uninstalled, 1);

        let games = crate::infrastructure::database::games::SqliteGameRepository::new(
            repository.pool.clone(),
        )
        .list(0, 10)
        .await
        .expect("list");
        assert!(
            !games
                .iter()
                .find(|game| game.provider_game_id.as_deref() == Some("10"))
                .expect("online")
                .installed
        );
        assert!(
            games
                .iter()
                .find(|game| game.provider_game_id.as_deref() == Some("20"))
                .expect("offline")
                .installed
        );
        assert_eq!(
            games
                .iter()
                .find(|game| game.provider_game_id.as_deref() == Some("20"))
                .expect("offline")
                .metadata_json["_steam_scan"]["library_offline"],
            true
        );
    }
}
