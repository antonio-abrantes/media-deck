//! SQLite-backed `GameRepository`.
//!
//! Uses runtime `sqlx::query()` (no compile-time macro) to avoid requiring
//! `DATABASE_URL` or `cargo sqlx prepare` at build time.

use crate::domain::entities::{Game, GameId, GameProvider};
use crate::domain::errors::DomainError;
use crate::domain::ports::{GameRepository, PortResult};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

pub struct SqliteGameRepository {
    pool: SqlitePool,
}

impl GameRepository for SqliteGameRepository {
    fn find_by_id<'a>(&'a self, id: &'a GameId) -> PortResult<'a, Option<Game>> {
        Box::pin(SqliteGameRepository::find_by_id(self, id))
    }

    fn find_by_provider_id<'a>(
        &'a self,
        provider: &'a str,
        provider_id: &'a str,
    ) -> PortResult<'a, Option<Game>> {
        Box::pin(SqliteGameRepository::find_by_provider_id(
            self,
            provider,
            provider_id,
        ))
    }

    fn upsert<'a>(&'a self, game: &'a Game) -> PortResult<'a, ()> {
        Box::pin(SqliteGameRepository::upsert(self, game))
    }

    fn list(&self, page: u32, per_page: u32) -> PortResult<'_, Vec<Game>> {
        Box::pin(SqliteGameRepository::list(self, page, per_page))
    }
}

impl SqliteGameRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: &GameId) -> Result<Option<Game>, DomainError> {
        let id_str = id.to_string();
        let row = sqlx::query(
            "SELECT id, provider, provider_game_id, display_name, sort_name,
                    install_dir, installed, metadata_json,
                    source_updated_at, created_at, updated_at
             FROM games WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        row.map(|r| map_row(&r)).transpose()
    }

    pub async fn find_by_provider_id(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> Result<Option<Game>, DomainError> {
        let row = sqlx::query(
            "SELECT id, provider, provider_game_id, display_name, sort_name,
                    install_dir, installed, metadata_json,
                    source_updated_at, created_at, updated_at
             FROM games WHERE provider = ? AND provider_game_id = ?",
        )
        .bind(provider)
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        row.map(|r| map_row(&r)).transpose()
    }

    pub async fn upsert(&self, game: &Game) -> Result<(), DomainError> {
        let id = game.id.to_string();
        let provider = game.provider.to_string();
        let meta = serde_json::to_string(&game.metadata_json)
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        let created = game.created_at.to_rfc3339();
        let updated = game.updated_at.to_rfc3339();
        let source_updated = game.source_updated_at.map(|t| t.to_rfc3339());

        sqlx::query(
            "INSERT INTO games
                (id, provider, provider_game_id, display_name, sort_name,
                 install_dir, installed, metadata_json,
                 source_updated_at, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
                provider           = excluded.provider,
                provider_game_id   = excluded.provider_game_id,
                display_name       = excluded.display_name,
                sort_name          = excluded.sort_name,
                install_dir        = excluded.install_dir,
                installed          = excluded.installed,
                metadata_json      = excluded.metadata_json,
                source_updated_at  = excluded.source_updated_at,
                updated_at         = excluded.updated_at",
        )
        .bind(&id)
        .bind(&provider)
        .bind(&game.provider_game_id)
        .bind(&game.display_name)
        .bind(&game.sort_name)
        .bind(&game.install_dir)
        .bind(game.installed)
        .bind(&meta)
        .bind(&source_updated)
        .bind(&created)
        .bind(&updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn list(&self, page: u32, per_page: u32) -> Result<Vec<Game>, DomainError> {
        let offset = (page * per_page) as i64;
        let limit = per_page as i64;

        let rows = sqlx::query(
            "SELECT id, provider, provider_game_id, display_name, sort_name,
                    install_dir, installed, metadata_json,
                    source_updated_at, created_at, updated_at
             FROM games ORDER BY sort_name ASC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        rows.iter().map(map_row).collect()
    }
}

// ─── Row mapping ──────────────────────────────────────────────────────────────

pub(crate) fn map_row(r: &sqlx::sqlite::SqliteRow) -> Result<Game, DomainError> {
    let id_str: String = r
        .try_get("id")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let provider_str: String = r
        .try_get("provider")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let provider_game_id: Option<String> = r
        .try_get("provider_game_id")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let display_name: String = r
        .try_get("display_name")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let sort_name: String = r
        .try_get("sort_name")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let install_dir: Option<String> = r
        .try_get("install_dir")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let installed: i64 = r
        .try_get("installed")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let metadata_json_str: String = r
        .try_get("metadata_json")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let source_updated_at: Option<String> = r
        .try_get("source_updated_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let created_at_str: String = r
        .try_get("created_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let updated_at_str: String = r
        .try_get("updated_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

    let id: GameId = GameId(
        id_str
            .parse()
            .map_err(|_| DomainError::DatabaseOperationFailed(format!("bad id: {id_str}")))?,
    );
    let provider = match provider_str.as_str() {
        "steam" => GameProvider::Steam,
        "executable" => GameProvider::Executable,
        other => {
            return Err(DomainError::DatabaseOperationFailed(format!(
                "unknown provider: {other}"
            )))
        }
    };
    let metadata_json: serde_json::Value = serde_json::from_str(&metadata_json_str)
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

    let parse_dt = |s: &str| -> Result<DateTime<Utc>, DomainError> {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))
    };

    Ok(Game {
        id,
        provider,
        provider_game_id,
        display_name,
        sort_name,
        install_dir,
        installed: installed != 0,
        metadata_json,
        source_updated_at: source_updated_at.as_deref().map(parse_dt).transpose()?,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::open_and_migrate;
    use chrono::Utc;
    use tempfile::tempdir;

    async fn make_repo() -> SqliteGameRepository {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = open_and_migrate(&db_path).await.expect("migrate");
        std::mem::forget(dir);
        SqliteGameRepository::new(pool)
    }

    #[tokio::test]
    async fn upsert_and_find_by_id() {
        let repo = make_repo().await;
        let game = Game::new(
            GameProvider::Steam,
            Some("440".into()),
            "Team Fortress 2",
            Utc::now(),
        );
        let id = game.id;
        repo.upsert(&game).await.expect("upsert");
        let found = repo
            .find_by_id(&id)
            .await
            .expect("find")
            .expect("should exist");
        assert_eq!(found.display_name, "Team Fortress 2");
        assert_eq!(found.provider_game_id, Some("440".into()));
    }

    #[tokio::test]
    async fn find_by_provider_id() {
        let repo = make_repo().await;
        let game = Game::new(
            GameProvider::Steam,
            Some("570".into()),
            "Dota 2",
            Utc::now(),
        );
        repo.upsert(&game).await.expect("upsert");
        let found = repo
            .find_by_provider_id("steam", "570")
            .await
            .expect("find")
            .expect("should exist");
        assert_eq!(found.sort_name, "dota 2");
    }

    #[tokio::test]
    async fn upsert_overwrites_existing() {
        let repo = make_repo().await;
        let mut game = Game::new(
            GameProvider::Steam,
            Some("220".into()),
            "Half-Life 2",
            Utc::now(),
        );
        repo.upsert(&game).await.expect("first upsert");
        game.installed = true;
        game.updated_at = Utc::now();
        repo.upsert(&game).await.expect("second upsert");
        let found = repo
            .find_by_id(&game.id)
            .await
            .expect("find")
            .expect("should exist");
        assert!(found.installed);
    }

    #[tokio::test]
    async fn list_returns_games_sorted_by_sort_name() {
        let repo = make_repo().await;
        let now = Utc::now();
        repo.upsert(&Game::new(GameProvider::Steam, None, "Zork", now))
            .await
            .unwrap();
        repo.upsert(&Game::new(GameProvider::Steam, None, "Ace Ventura", now))
            .await
            .unwrap();
        repo.upsert(&Game::new(GameProvider::Steam, None, "Minecraft", now))
            .await
            .unwrap();
        let games = repo.list(0, 10).await.expect("list");
        let names: Vec<&str> = games.iter().map(|g| g.sort_name.as_str()).collect();
        assert_eq!(names, vec!["ace ventura", "minecraft", "zork"]);
    }
}
