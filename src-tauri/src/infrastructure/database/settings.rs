//! SQLite-backed `SettingsRepository`.

use crate::domain::errors::DomainError;
use crate::domain::ports::{PortResult, SettingsRepository};
use chrono::Utc;
use sqlx::SqlitePool;

/// Known settings keys. Only these keys may be read or written.
pub const KNOWN_KEYS: &[&str] = &[
    "monitor_active",
    "launcher_monitor",
    "autostart",
    "presentation_duration",
    "sound_enabled",
    "reduce_motion",
    "close_policy",
    "close_timeout_secs",
    "log_level",
    "session_retention_days",
    "language",
];

pub struct SqliteSettingsRepository {
    pool: SqlitePool,
}

impl SqliteSettingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Validate that `key` is in the known-keys allow-list.
    fn validate_key(key: &str) -> Result<(), DomainError> {
        validate_setting_key(key)
    }

    /// Get the JSON value for a known settings key.
    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, DomainError> {
        Self::validate_key(key)?;

        let row: Option<(String,)> =
            sqlx::query_as("SELECT value_json FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        match row {
            None => Ok(None),
            Some((json,)) => {
                let val: serde_json::Value = serde_json::from_str(&json)
                    .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
                validate_setting(key, &val)?;
                Ok(Some(val))
            }
        }
    }

    /// Upsert a validated JSON value for a known settings key.
    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), DomainError> {
        Self::validate_key(key)?;
        validate_setting(key, &value)?;

        let value_json = serde_json::to_string(&value)
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO settings (key, value_json, updated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                             updated_at  = excluded.updated_at",
        )
        .bind(key)
        .bind(&value_json)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        Ok(())
    }

    /// Atomically apply a settings patch after validating every entry.
    ///
    /// Validation happens before opening the transaction, so an invalid item
    /// cannot leave earlier settings partially updated.
    pub async fn set_many(&self, entries: &[(&str, serde_json::Value)]) -> Result<(), DomainError> {
        for (key, value) in entries {
            Self::validate_key(key)?;
            validate_setting(key, value)?;
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        let now = Utc::now().to_rfc3339();

        for (key, value) in entries {
            let value_json = serde_json::to_string(value)
                .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
            sqlx::query(
                "INSERT INTO settings (key, value_json, updated_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                                 updated_at  = excluded.updated_at",
            )
            .bind(key)
            .bind(value_json)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))
    }
}

impl SettingsRepository for SqliteSettingsRepository {
    fn get<'a>(&'a self, key: &'a str) -> PortResult<'a, Option<serde_json::Value>> {
        Box::pin(SqliteSettingsRepository::get(self, key))
    }

    fn set<'a>(&'a self, key: &'a str, value: serde_json::Value) -> PortResult<'a, ()> {
        Box::pin(SqliteSettingsRepository::set(self, key, value))
    }
}

pub fn validate_setting_key(key: &str) -> Result<(), DomainError> {
    if KNOWN_KEYS.contains(&key) {
        Ok(())
    } else {
        Err(DomainError::SettingInvalid(key.to_owned()))
    }
}

/// Validate the shape and allowed range of a persisted public setting.
pub fn validate_setting(key: &str, value: &serde_json::Value) -> Result<(), DomainError> {
    let valid = match key {
        "monitor_active" | "autostart" | "sound_enabled" | "reduce_motion" => value.is_boolean(),
        "launcher_monitor" => value
            .as_str()
            .is_some_and(|monitor| monitor == "auto" || (1..=256).contains(&monitor.len())),
        "presentation_duration" => matches!(value.as_str(), Some("short" | "normal" | "cinematic")),
        "close_policy" => matches!(
            value.as_str(),
            Some("wait_and_ask" | "force_after_timeout" | "detach")
        ),
        "close_timeout_secs" => value.as_u64().is_some_and(|v| (1..=300).contains(&v)),
        "log_level" => matches!(
            value.as_str(),
            Some("error" | "warn" | "info" | "debug" | "trace")
        ),
        "session_retention_days" => value.as_u64().is_some_and(|v| (1..=3650).contains(&v)),
        "language" => matches!(value.as_str(), Some("pt-BR" | "en-US")),
        _ => false,
    };

    if valid {
        Ok(())
    } else {
        Err(DomainError::SettingInvalid(key.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::open_and_migrate;
    use tempfile::tempdir;

    async fn make_repo() -> SqliteSettingsRepository {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = open_and_migrate(&db_path).await.expect("migrate");
        // Keep dir alive by leaking it in test context (acceptable for tests).
        std::mem::forget(dir);
        SqliteSettingsRepository::new(pool)
    }

    #[tokio::test]
    async fn get_seeded_setting_returns_value() {
        let repo = make_repo().await;
        let val = repo.get("monitor_active").await.expect("get");
        assert_eq!(val, Some(serde_json::Value::Bool(true)));
    }

    #[tokio::test]
    async fn set_and_get_round_trip() {
        let repo = make_repo().await;
        repo.set("sound_enabled", serde_json::Value::Bool(true))
            .await
            .expect("set");
        let val = repo.get("sound_enabled").await.expect("get");
        assert_eq!(val, Some(serde_json::Value::Bool(true)));
    }

    #[tokio::test]
    async fn set_unknown_key_returns_error() {
        let repo = make_repo().await;
        let result = repo.set("hacker_key", serde_json::json!("payload")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_unknown_key_returns_error() {
        let repo = make_repo().await;
        let result = repo.get("not_a_real_key").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn upsert_overwrites_existing() {
        let repo = make_repo().await;
        repo.set("close_timeout_secs", serde_json::json!(30))
            .await
            .expect("first set");
        repo.set("close_timeout_secs", serde_json::json!(60))
            .await
            .expect("second set");
        let val = repo.get("close_timeout_secs").await.expect("get");
        assert_eq!(val, Some(serde_json::json!(60)));
    }

    #[tokio::test]
    async fn rejects_wrong_type_and_out_of_range_values() {
        let repo = make_repo().await;
        assert!(matches!(
            repo.set("sound_enabled", serde_json::json!("yes")).await,
            Err(DomainError::SettingInvalid(_))
        ));
        assert!(matches!(
            repo.set("close_timeout_secs", serde_json::json!(0)).await,
            Err(DomainError::SettingInvalid(_))
        ));
        assert!(matches!(
            repo.set("language", serde_json::json!("xx-YY")).await,
            Err(DomainError::SettingInvalid(_))
        ));
    }

    #[tokio::test]
    async fn set_many_is_atomic_when_patch_is_invalid() {
        let repo = make_repo().await;
        let before = repo.get("sound_enabled").await.expect("get before");
        let result = repo
            .set_many(&[
                ("sound_enabled", serde_json::json!(true)),
                ("close_timeout_secs", serde_json::json!(0)),
            ])
            .await;

        assert!(matches!(result, Err(DomainError::SettingInvalid(_))));
        assert_eq!(
            repo.get("sound_enabled").await.expect("get after"),
            before,
            "valid entries must not be partially applied"
        );
    }
}
