//! SQLite-backed artwork metadata repository.

use crate::domain::artwork::{Artwork, ArtworkKind, ArtworkOrigin};
use crate::domain::entities::{ArtworkId, GameId};
use crate::domain::errors::DomainError;
use crate::domain::ports::{ArtworkRepository, PortResult};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

pub struct SqliteArtworkRepository {
    pool: SqlitePool,
}

impl SqliteArtworkRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: &ArtworkId) -> Result<Option<Artwork>, DomainError> {
        let row = sqlx::query(
            "SELECT id, game_id, kind, provider, relative_path, source_url, sha256,
                    width, height, mime_type, is_user_override, created_at, updated_at
             FROM artwork WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;
        row.map(|row| map_row(&row)).transpose()
    }

    pub async fn find_by_game_kind_hash(
        &self,
        game_id: &GameId,
        kind: ArtworkKind,
        sha256: &str,
    ) -> Result<Option<Artwork>, DomainError> {
        let row = sqlx::query(
            "SELECT id, game_id, kind, provider, relative_path, source_url, sha256,
                    width, height, mime_type, is_user_override, created_at, updated_at
             FROM artwork WHERE game_id = ? AND kind = ? AND sha256 = ?",
        )
        .bind(game_id.to_string())
        .bind(kind.as_str())
        .bind(sha256)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?;
        row.map(|row| map_row(&row)).transpose()
    }

    pub async fn list_for_game_kind(
        &self,
        game_id: &GameId,
        kind: ArtworkKind,
    ) -> Result<Vec<Artwork>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, game_id, kind, provider, relative_path, source_url, sha256,
                    width, height, mime_type, is_user_override, created_at, updated_at
             FROM artwork WHERE game_id = ? AND kind = ?
             ORDER BY is_user_override DESC, updated_at DESC, id DESC",
        )
        .bind(game_id.to_string())
        .bind(kind.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        rows.iter().map(map_row).collect()
    }

    pub async fn upsert(&self, artwork: &Artwork) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO artwork
                (id, game_id, kind, provider, relative_path, source_url, sha256,
                 width, height, mime_type, is_user_override, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(game_id, kind, sha256) DO UPDATE SET
                provider = CASE
                    WHEN artwork.is_user_override = 1 THEN artwork.provider
                    ELSE excluded.provider
                END,
                relative_path = CASE
                    WHEN artwork.is_user_override = 1 THEN artwork.relative_path
                    ELSE excluded.relative_path
                END,
                source_url = CASE
                    WHEN artwork.is_user_override = 1 THEN artwork.source_url
                    ELSE excluded.source_url
                END,
                width = excluded.width,
                height = excluded.height,
                mime_type = excluded.mime_type,
                is_user_override = MAX(artwork.is_user_override, excluded.is_user_override),
                updated_at = excluded.updated_at",
        )
        .bind(artwork.id.to_string())
        .bind(artwork.game_id.to_string())
        .bind(artwork.kind.as_str())
        .bind(artwork.origin.as_str())
        .bind(&artwork.relative_path)
        .bind(&artwork.source_url)
        .bind(&artwork.sha256)
        .bind(i64::from(artwork.width))
        .bind(i64::from(artwork.height))
        .bind(&artwork.mime_type)
        .bind(artwork.is_user_override)
        .bind(artwork.created_at.to_rfc3339())
        .bind(artwork.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db_error)?;
        Ok(())
    }
}

impl ArtworkRepository for SqliteArtworkRepository {
    fn find_by_id<'a>(&'a self, id: &'a ArtworkId) -> PortResult<'a, Option<Artwork>> {
        Box::pin(self.find_by_id(id))
    }

    fn find_by_game_kind_hash<'a>(
        &'a self,
        game_id: &'a GameId,
        kind: ArtworkKind,
        sha256: &'a str,
    ) -> PortResult<'a, Option<Artwork>> {
        Box::pin(self.find_by_game_kind_hash(game_id, kind, sha256))
    }

    fn list_for_game_kind<'a>(
        &'a self,
        game_id: &'a GameId,
        kind: ArtworkKind,
    ) -> PortResult<'a, Vec<Artwork>> {
        Box::pin(self.list_for_game_kind(game_id, kind))
    }

    fn upsert<'a>(&'a self, artwork: &'a Artwork) -> PortResult<'a, ()> {
        Box::pin(self.upsert(artwork))
    }
}

fn map_row(row: &sqlx::sqlite::SqliteRow) -> Result<Artwork, DomainError> {
    let id = parse_id::<ArtworkId>(&row.try_get::<String, _>("id").map_err(db_error)?)?;
    let game_id = parse_id::<GameId>(&row.try_get::<String, _>("game_id").map_err(db_error)?)?;
    let width = row.try_get::<i64, _>("width").map_err(db_error)?;
    let height = row.try_get::<i64, _>("height").map_err(db_error)?;
    Ok(Artwork {
        id,
        game_id,
        kind: ArtworkKind::from_db(&row.try_get::<String, _>("kind").map_err(db_error)?)?,
        origin: ArtworkOrigin::from_db(&row.try_get::<String, _>("provider").map_err(db_error)?)?,
        relative_path: row.try_get("relative_path").map_err(db_error)?,
        source_url: row.try_get("source_url").map_err(db_error)?,
        sha256: row.try_get("sha256").map_err(db_error)?,
        width: u32::try_from(width).map_err(|_| corrupt_row())?,
        height: u32::try_from(height).map_err(|_| corrupt_row())?,
        mime_type: row.try_get("mime_type").map_err(db_error)?,
        is_user_override: row
            .try_get::<i64, _>("is_user_override")
            .map_err(db_error)?
            != 0,
        created_at: parse_datetime(&row.try_get::<String, _>("created_at").map_err(db_error)?)?,
        updated_at: parse_datetime(&row.try_get::<String, _>("updated_at").map_err(db_error)?)?,
    })
}

fn parse_id<T>(value: &str) -> Result<T, DomainError>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| corrupt_row())
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, DomainError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| corrupt_row())
}

fn corrupt_row() -> DomainError {
    DomainError::DatabaseOperationFailed("invalid artwork row".to_owned())
}

fn db_error(error: impl std::fmt::Display) -> DomainError {
    DomainError::DatabaseOperationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{Game, GameProvider};
    use crate::infrastructure::database::games::SqliteGameRepository;
    use crate::infrastructure::database::open_and_migrate;
    use tempfile::tempdir;

    #[tokio::test]
    async fn lists_user_override_before_newer_remote_artwork() {
        let directory = tempdir().unwrap();
        let pool = open_and_migrate(&directory.path().join("artwork.db"))
            .await
            .unwrap();
        let game = Game::new(GameProvider::Steam, Some("440".into()), "TF2", Utc::now());
        SqliteGameRepository::new(pool.clone())
            .upsert(&game)
            .await
            .unwrap();
        let repository = SqliteArtworkRepository::new(pool);
        let now = Utc::now();
        for (hash, override_value, offset) in
            [("a".repeat(64), true, 0), ("b".repeat(64), false, 1)]
        {
            repository
                .upsert(&Artwork {
                    id: ArtworkId::new(),
                    game_id: game.id,
                    kind: ArtworkKind::LauncherCover,
                    origin: if override_value {
                        ArtworkOrigin::Manual
                    } else {
                        ArtworkOrigin::Remote
                    },
                    relative_path: format!("aa/{hash}.png"),
                    source_url: None,
                    sha256: hash,
                    width: 32,
                    height: 48,
                    mime_type: "image/png".into(),
                    is_user_override: override_value,
                    created_at: now + chrono::Duration::seconds(offset),
                    updated_at: now + chrono::Duration::seconds(offset),
                })
                .await
                .unwrap();
        }
        let records = repository
            .list_for_game_kind(&game.id, ArtworkKind::LauncherCover)
            .await
            .unwrap();
        assert!(records[0].is_user_override);
    }
}
