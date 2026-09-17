//! SQLite persistence layer for MediaDeck.
//!
//! Uses `sqlx` with compile-time verified queries (`query!` macro).
//! The database file lives in `%LOCALAPPDATA%\MediaDeck\media-deck.db`.
//! Migrations are embedded from `../migrations/` and run automatically on open.

pub mod activations;
pub mod artwork;
pub mod catalog;
pub mod devices;
pub mod fake_repos;
pub mod games;
pub mod labels;
pub mod sessions;
pub mod settings;

use crate::domain::errors::DomainError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;
use std::time::Duration;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Open (or create) the SQLite database at `db_path` and run all pending migrations.
///
/// Returns the connection pool ready for use by repository adapters.
pub async fn open_and_migrate(db_path: &Path) -> Result<SqlitePool, DomainError> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let migration_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| DomainError::DatabaseMigrationFailed(e.to_string()))?;

    run_migrations(&migration_pool).await?;
    migration_pool.close().await;

    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options.journal_mode(SqliteJournalMode::Wal))
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))
}

/// Run all embedded migrations in `migrations/`.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), DomainError> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| DomainError::DatabaseMigrationFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn migration_creates_schema_in_tempdir() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("media-deck.db");
        let pool = open_and_migrate(&db_path).await.expect("open_and_migrate");

        // Verify all 10 tables were created.
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlx_%' AND name NOT LIKE '_sqlx_%' ORDER BY name"
        )
        .fetch_all(&pool)
        .await
        .expect("query tables");

        let expected = vec![
            "artwork",
            "game_activations",
            "game_sessions",
            "games",
            "label_projects",
            "launch_profiles",
            "media",
            "media_devices",
            "session_processes",
            "settings",
        ];
        assert_eq!(tables, expected, "all 10 tables must exist after migration");
    }

    #[tokio::test]
    async fn migration_seeds_default_settings() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = open_and_migrate(&db_path).await.expect("open_and_migrate");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .expect("count settings");

        assert!(count >= 10, "at least 10 default settings must be seeded");
    }

    #[tokio::test]
    async fn migration_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("idem.db");
        let pool = open_and_migrate(&db_path).await.expect("first run");
        // Running migrations again must not error.
        run_migrations(&pool)
            .await
            .expect("second run must be idempotent");
    }

    #[tokio::test]
    async fn database_uses_wal_for_concurrent_access() {
        let dir = tempdir().expect("tempdir");
        let pool = open_and_migrate(&dir.path().join("wal.db"))
            .await
            .expect("open");
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("journal mode");
        assert_eq!(mode.to_ascii_lowercase(), "wal");
    }
}
