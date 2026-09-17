use crate::domain::entities::{ContentHash, ExportKind, GameActivation, GameId, MediaKey};
use crate::domain::errors::DomainError;
use crate::domain::ports::{ActivationRepository, PortResult};
use chrono::{DateTime, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

pub struct SqliteActivationRepository {
    pool: SqlitePool,
}

impl SqliteActivationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn find(&self, game_id: &GameId) -> Result<Option<GameActivation>, DomainError> {
        sqlx::query(
            "SELECT game_id, profile_id, last_export_kind, last_media_key,
                    last_content_hash, schema_version, export_count,
                    first_activated_at, last_exported_at
             FROM game_activations WHERE game_id = ?",
        )
        .bind(game_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?
        .as_ref()
        .map(map_row)
        .transpose()
    }

    async fn list_all(&self) -> Result<Vec<GameActivation>, DomainError> {
        let rows = sqlx::query(
            "SELECT game_id, profile_id, last_export_kind, last_media_key,
                    last_content_hash, schema_version, export_count,
                    first_activated_at, last_exported_at
             FROM game_activations
             ORDER BY last_exported_at DESC, game_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.iter().map(map_row).collect()
    }

    async fn save(&self, activation: &GameActivation) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO game_activations (
                game_id, profile_id, last_export_kind, last_media_key,
                last_content_hash, schema_version, export_count,
                first_activated_at, last_exported_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(game_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                last_export_kind = excluded.last_export_kind,
                last_media_key = excluded.last_media_key,
                last_content_hash = excluded.last_content_hash,
                schema_version = excluded.schema_version,
                export_count = game_activations.export_count + 1,
                last_exported_at = excluded.last_exported_at",
        )
        .bind(activation.game_id.to_string())
        .bind(activation.profile_id.to_string())
        .bind(activation.last_export_kind.as_str())
        .bind(activation.last_media_key.as_str())
        .bind(activation.last_content_hash.as_str())
        .bind(i64::from(activation.schema_version))
        .bind(i64::from(activation.export_count.max(1)))
        .bind(activation.first_activated_at.to_rfc3339())
        .bind(activation.last_exported_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(())
    }

    async fn remove(&self, game_id: &GameId) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM game_activations WHERE game_id = ?")
            .bind(game_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(result.rows_affected() > 0)
    }
}

impl ActivationRepository for SqliteActivationRepository {
    fn find_by_game_id<'a>(
        &'a self,
        game_id: &'a GameId,
    ) -> PortResult<'a, Option<GameActivation>> {
        Box::pin(self.find(game_id))
    }

    fn list(&self) -> PortResult<'_, Vec<GameActivation>> {
        Box::pin(self.list_all())
    }

    fn upsert<'a>(&'a self, activation: &'a GameActivation) -> PortResult<'a, ()> {
        Box::pin(self.save(activation))
    }

    fn delete<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, bool> {
        Box::pin(self.remove(game_id))
    }
}

fn map_row(row: &SqliteRow) -> Result<GameActivation, DomainError> {
    let schema_version = row.try_get::<i64, _>("schema_version").map_err(db_error)?;
    let export_count = row.try_get::<i64, _>("export_count").map_err(db_error)?;
    Ok(GameActivation {
        game_id: parse_id(row, "game_id")?,
        profile_id: parse_id(row, "profile_id")?,
        last_export_kind: ExportKind::from_db(
            &row.try_get::<String, _>("last_export_kind")
                .map_err(db_error)?,
        )?,
        last_media_key: MediaKey::new(
            row.try_get::<String, _>("last_media_key")
                .map_err(db_error)?,
        )
        .map_err(|_| corrupt())?,
        last_content_hash: ContentHash::new(
            row.try_get::<String, _>("last_content_hash")
                .map_err(db_error)?,
        )
        .map_err(|_| corrupt())?,
        schema_version: u32::try_from(schema_version).map_err(|_| corrupt())?,
        export_count: u32::try_from(export_count).map_err(|_| corrupt())?,
        first_activated_at: parse_time(row, "first_activated_at")?,
        last_exported_at: parse_time(row, "last_exported_at")?,
    })
}

fn parse_id<T: std::str::FromStr>(row: &SqliteRow, column: &str) -> Result<T, DomainError> {
    row.try_get::<String, _>(column)
        .map_err(db_error)?
        .parse()
        .map_err(|_| corrupt())
}

fn parse_time(row: &SqliteRow, column: &str) -> Result<DateTime<Utc>, DomainError> {
    DateTime::parse_from_rfc3339(&row.try_get::<String, _>(column).map_err(db_error)?)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| corrupt())
}

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseOperationFailed(error.to_string())
}

fn corrupt() -> DomainError {
    DomainError::DatabaseOperationFailed("invalid game activation row".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{ExportKind, Game, GameProvider, LaunchKind, LaunchProfile};
    use crate::infrastructure::database::catalog::SqliteCatalogRepository;
    use crate::infrastructure::database::open_and_migrate;
    use chrono::TimeZone;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn upsert_preserves_first_activation_and_increments_count() {
        let directory = tempdir().unwrap();
        let pool = open_and_migrate(&directory.path().join("collection.db"))
            .await
            .unwrap();
        let catalog = Arc::new(SqliteCatalogRepository::new(pool.clone()));
        let now = Utc.with_ymd_and_hms(2026, 9, 16, 12, 0, 0).unwrap();
        let game = Game::new(GameProvider::Steam, Some("10".into()), "Game", now);
        let profile = LaunchProfile::new(game.id, "Steam", LaunchKind::Steam { app_id: 10 }, now);
        catalog
            .upsert_game_and_profile(&game, &profile)
            .await
            .unwrap();
        let repository = SqliteActivationRepository::new(pool);
        let first = GameActivation {
            game_id: game.id,
            profile_id: profile.id,
            last_export_kind: ExportKind::IniFile,
            last_media_key: MediaKey::new("GAME-001").unwrap(),
            last_content_hash: ContentHash::new("a".repeat(64)).unwrap(),
            schema_version: 2,
            export_count: 1,
            first_activated_at: now,
            last_exported_at: now,
        };
        repository.upsert(&first).await.unwrap();
        let later = now + chrono::Duration::minutes(5);
        let mut second = first.clone();
        second.last_media_key = MediaKey::new("GAME-002").unwrap();
        second.last_content_hash = ContentHash::new("b".repeat(64)).unwrap();
        second.last_exported_at = later;
        repository.upsert(&second).await.unwrap();

        let saved = repository.find_by_game_id(&game.id).await.unwrap().unwrap();
        assert_eq!(saved.export_count, 2);
        assert_eq!(saved.first_activated_at, now);
        assert_eq!(saved.last_exported_at, later);
        assert_eq!(saved.last_media_key.as_str(), "GAME-002");
        assert!(repository.delete(&game.id).await.unwrap());
        assert!(repository
            .find_by_game_id(&game.id)
            .await
            .unwrap()
            .is_none());
    }
}
