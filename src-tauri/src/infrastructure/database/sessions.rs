//! SQLite-backed session and device repositories.
//!
//! Uses runtime `sqlx::query()` (no compile-time macro) to avoid requiring
//! `DATABASE_URL` or `cargo sqlx prepare` at build time.

use crate::domain::entities::{
    DeviceId, DriveType, MediaDevice, MonitorPolicy, ProfileId, SessionId,
};
use crate::domain::errors::DomainError;
use crate::domain::ports::{
    DeviceRepository, PortResult, SessionRecord, SessionRepository, TrackedProcessRecord,
};
use chrono::Utc;
use sqlx::{Row, SqlitePool};

// ─── Session repository ───────────────────────────────────────────────────────

pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: &SessionId) -> Result<Option<SessionRecord>, DomainError> {
        let id_str = id.to_string();
        let row = sqlx::query(
            "SELECT id, media_key, profile_id, state, launch_requested_at, close_result
             FROM game_sessions WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        row.map(|r| map_session_row(&r)).transpose()
    }

    pub async fn upsert(&self, record: &SessionRecord) -> Result<(), DomainError> {
        let id = record.id.to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO game_sessions (
                id, media_key, profile_id, state, launch_requested_at, close_result,
                created_at, updated_at
             )
             VALUES (?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
                media_key = excluded.media_key,
                profile_id = excluded.profile_id,
                state = excluded.state,
                launch_requested_at = excluded.launch_requested_at,
                close_result = excluded.close_result,
                updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&record.media_key)
        .bind(record.profile_id.to_string())
        .bind(&record.state_label)
        .bind(&record.launch_requested_at)
        .bind(&record.close_result)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn find_interrupted(&self) -> Result<Vec<SessionRecord>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, media_key, profile_id, state, launch_requested_at, close_result
             FROM game_sessions
             WHERE state IN ('running', 'awaiting_process', 'launching', 'closing_requested', 'close_decision', 'forced_closing')
             ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        rows.iter().map(map_session_row).collect()
    }

    pub async fn replace_processes(
        &self,
        session_id: &SessionId,
        processes: &[TrackedProcessRecord],
    ) -> Result<(), DomainError> {
        let id = session_id.to_string();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        sqlx::query("DELETE FROM session_processes WHERE session_id = ?")
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        for process in processes {
            sqlx::query(
                "INSERT INTO session_processes (
                    session_id, pid, created_time, executable_path, role, window_handle
                 ) VALUES (?,?,?,?,?,?)",
            )
            .bind(&id)
            .bind(i64::from(process.pid))
            .bind(process.creation_time.to_string())
            .bind(&process.executable_path)
            .bind(&process.role)
            .bind(process.window_handle.map(|handle| handle.to_string()))
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        Ok(())
    }
}

impl SessionRepository for SqliteSessionRepository {
    fn find_by_id<'a>(&'a self, id: &'a SessionId) -> PortResult<'a, Option<SessionRecord>> {
        Box::pin(SqliteSessionRepository::find_by_id(self, id))
    }

    fn upsert<'a>(&'a self, record: &'a SessionRecord) -> PortResult<'a, ()> {
        Box::pin(SqliteSessionRepository::upsert(self, record))
    }

    fn find_interrupted(&self) -> PortResult<'_, Vec<SessionRecord>> {
        Box::pin(SqliteSessionRepository::find_interrupted(self))
    }

    fn replace_processes<'a>(
        &'a self,
        session_id: &'a SessionId,
        processes: &'a [TrackedProcessRecord],
    ) -> PortResult<'a, ()> {
        Box::pin(SqliteSessionRepository::replace_processes(
            self, session_id, processes,
        ))
    }
}

fn map_session_row(r: &sqlx::sqlite::SqliteRow) -> Result<SessionRecord, DomainError> {
    let raw_id: String = r
        .try_get("id")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let media_key: String = r
        .try_get("media_key")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let raw_profile_id: String = r
        .try_get("profile_id")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let state_label: String = r
        .try_get("state")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let launch_requested_at: Option<String> = r
        .try_get("launch_requested_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let close_result: Option<String> = r
        .try_get("close_result")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let session_id: SessionId =
        SessionId(raw_id.parse().map_err(|_| {
            DomainError::DatabaseOperationFailed(format!("bad session id: {raw_id}"))
        })?);
    let profile_id: ProfileId = raw_profile_id.parse().map_err(|_| {
        DomainError::DatabaseOperationFailed(format!("bad profile id: {raw_profile_id}"))
    })?;
    Ok(SessionRecord {
        id: session_id,
        media_key,
        profile_id,
        state_label,
        launch_requested_at,
        close_result,
    })
}

// ─── Device repository ────────────────────────────────────────────────────────

pub struct SqliteDeviceRepository {
    pool: SqlitePool,
}

impl SqliteDeviceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_all(&self) -> Result<Vec<MediaDevice>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, device_instance_id, interface_path, friendly_name, drive_type,
                    current_mount_point, capabilities_json, monitor_policy, enabled,
                    last_seen_at, created_at, updated_at
             FROM media_devices ORDER BY friendly_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        rows.iter().map(map_device_row).collect()
    }

    pub async fn find_configured(&self) -> Result<Vec<MediaDevice>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, device_instance_id, interface_path, friendly_name, drive_type,
                    current_mount_point, capabilities_json, monitor_policy, enabled,
                    last_seen_at, created_at, updated_at
             FROM media_devices WHERE enabled = 1 ORDER BY friendly_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        rows.iter().map(map_device_row).collect()
    }

    pub async fn find_by_id(&self, id: &DeviceId) -> Result<Option<MediaDevice>, DomainError> {
        let row = sqlx::query(
            "SELECT id, device_instance_id, interface_path, friendly_name, drive_type,
                    current_mount_point, capabilities_json, monitor_policy, enabled,
                    last_seen_at, created_at, updated_at
             FROM media_devices WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        row.map(|row| map_device_row(&row)).transpose()
    }

    pub async fn upsert(&self, device: &MediaDevice) -> Result<(), DomainError> {
        let id = device.id.to_string();
        let caps = serde_json::to_string(&device.capabilities)
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        let drive_type = drive_type_to_str(&device.drive_type);
        let monitor_policy = monitor_policy_to_str(&device.monitor_policy);
        let created = device.created_at.to_rfc3339();
        let updated = device.updated_at.to_rfc3339();
        let last_seen = device.last_seen_at.map(|t| t.to_rfc3339());

        sqlx::query(
            "INSERT INTO media_devices
                (id, device_instance_id, interface_path, friendly_name, drive_type,
                 current_mount_point, capabilities_json, monitor_policy, enabled,
                 last_seen_at, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
                device_instance_id  = excluded.device_instance_id,
                interface_path      = excluded.interface_path,
                friendly_name       = excluded.friendly_name,
                drive_type          = excluded.drive_type,
                current_mount_point = excluded.current_mount_point,
                capabilities_json   = excluded.capabilities_json,
                monitor_policy      = excluded.monitor_policy,
                enabled             = excluded.enabled,
                last_seen_at        = excluded.last_seen_at,
                updated_at          = excluded.updated_at",
        )
        .bind(&id)
        .bind(&device.device_instance_id)
        .bind(&device.interface_path)
        .bind(&device.friendly_name)
        .bind(drive_type)
        .bind(&device.current_mount_point)
        .bind(&caps)
        .bind(monitor_policy)
        .bind(device.enabled)
        .bind(&last_seen)
        .bind(&created)
        .bind(&updated)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

        Ok(())
    }

    pub async fn update_mount_point(
        &self,
        id: &DeviceId,
        mount_point: Option<&str>,
    ) -> Result<(), DomainError> {
        let id_str = id.to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE media_devices SET current_mount_point = ?, updated_at = ? WHERE id = ?",
        )
        .bind(mount_point)
        .bind(&now)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
        Ok(())
    }
}

impl DeviceRepository for SqliteDeviceRepository {
    fn list_all(&self) -> PortResult<'_, Vec<MediaDevice>> {
        Box::pin(SqliteDeviceRepository::list_all(self))
    }

    fn find_configured(&self) -> PortResult<'_, Vec<MediaDevice>> {
        Box::pin(SqliteDeviceRepository::find_configured(self))
    }

    fn find_by_id<'a>(&'a self, id: &'a DeviceId) -> PortResult<'a, Option<MediaDevice>> {
        Box::pin(SqliteDeviceRepository::find_by_id(self, id))
    }

    fn upsert<'a>(&'a self, device: &'a MediaDevice) -> PortResult<'a, ()> {
        Box::pin(SqliteDeviceRepository::upsert(self, device))
    }

    fn update_mount_point<'a>(
        &'a self,
        id: &'a DeviceId,
        mount_point: Option<&'a str>,
    ) -> PortResult<'a, ()> {
        Box::pin(SqliteDeviceRepository::update_mount_point(
            self,
            id,
            mount_point,
        ))
    }
}

// ─── Row mapping helpers ──────────────────────────────────────────────────────

fn map_device_row(r: &sqlx::sqlite::SqliteRow) -> Result<MediaDevice, DomainError> {
    let id_str: String = r
        .try_get("id")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let device_instance_id: Option<String> = r
        .try_get("device_instance_id")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let interface_path: Option<String> = r
        .try_get("interface_path")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let friendly_name: String = r
        .try_get("friendly_name")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let drive_type_str: String = r
        .try_get("drive_type")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let current_mount_point: Option<String> = r
        .try_get("current_mount_point")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let capabilities_json_str: String = r
        .try_get("capabilities_json")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let monitor_policy_str: String = r
        .try_get("monitor_policy")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let enabled: i64 = r
        .try_get("enabled")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let last_seen_at: Option<String> = r
        .try_get("last_seen_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let created_at_str: String = r
        .try_get("created_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;
    let updated_at_str: String = r
        .try_get("updated_at")
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

    let id: DeviceId =
        DeviceId(id_str.parse().map_err(|_| {
            DomainError::DatabaseOperationFailed(format!("bad device id: {id_str}"))
        })?);
    let drive_type = match drive_type_str.as_str() {
        "removable" => DriveType::Removable,
        "cdrom" => DriveType::CdRom,
        "fixed" => DriveType::Fixed,
        "remote" => DriveType::Remote,
        "ramdisk" => DriveType::RamDisk,
        _ => DriveType::Unknown,
    };
    let monitor_policy = match monitor_policy_str.as_str() {
        "exact_device" => MonitorPolicy::ExactDevice,
        "any_optical" => MonitorPolicy::AnyOptical,
        _ => MonitorPolicy::Disabled,
    };
    let capabilities: serde_json::Value = serde_json::from_str(&capabilities_json_str)
        .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))?;

    let parse_dt = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|e| DomainError::DatabaseOperationFailed(e.to_string()))
    };

    Ok(MediaDevice {
        id,
        device_instance_id,
        interface_path,
        friendly_name,
        drive_type,
        current_mount_point,
        capabilities,
        monitor_policy,
        enabled: enabled != 0,
        last_seen_at: last_seen_at.as_deref().map(parse_dt).transpose()?,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
    })
}

fn drive_type_to_str(dt: &DriveType) -> &'static str {
    match dt {
        DriveType::Removable => "removable",
        DriveType::CdRom => "cdrom",
        DriveType::Fixed => "fixed",
        DriveType::Remote => "remote",
        DriveType::RamDisk => "ramdisk",
        DriveType::Unknown => "unknown",
    }
}

fn monitor_policy_to_str(mp: &MonitorPolicy) -> &'static str {
    match mp {
        MonitorPolicy::ExactDevice => "exact_device",
        MonitorPolicy::AnyOptical => "any_optical",
        MonitorPolicy::Disabled => "disabled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::open_and_migrate;
    use chrono::Utc;
    use tempfile::tempdir;

    async fn pool() -> SqlitePool {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let p = open_and_migrate(&db_path).await.expect("migrate");
        std::mem::forget(dir);
        p
    }

    #[tokio::test]
    async fn session_upsert_and_find() {
        let p = pool().await;
        let repo = SqliteSessionRepository::new(p);
        let id = SessionId::new();
        let record = SessionRecord {
            id,
            media_key: "NR-001".into(),
            profile_id: ProfileId::new(),
            state_label: "running".into(),
            launch_requested_at: Some("2026-09-15T12:00:00Z".into()),
            close_result: None,
        };
        repo.upsert(&record).await.expect("upsert");
        let found = repo.find_by_id(&id).await.expect("find").expect("exists");
        assert_eq!(found.state_label, "running");
        assert_eq!(found.media_key, "NR-001");
        assert_eq!(
            found.launch_requested_at.as_deref(),
            Some("2026-09-15T12:00:00Z")
        );

        let interrupted = repo.find_interrupted().await.expect("interrupted");
        assert_eq!(interrupted.len(), 1);

        repo.replace_processes(
            &id,
            &[TrackedProcessRecord {
                pid: 42,
                creation_time: 99,
                executable_path: r"C:\Games\a.exe".into(),
                role: "main".into(),
                window_handle: Some(1),
            }],
        )
        .await
        .expect("processes");
    }

    #[tokio::test]
    async fn device_upsert_and_find_configured() {
        let p = pool().await;
        let repo = SqliteDeviceRepository::new(p);
        let mut device = MediaDevice::new("USB Floppy", DriveType::Removable, Utc::now());
        device.enabled = true;
        device.current_mount_point = Some("A:".into());
        repo.upsert(&device).await.expect("upsert");
        let configured = repo.find_configured().await.expect("find");
        assert_eq!(configured.len(), 1);
        assert_eq!(configured[0].current_mount_point, Some("A:".into()));
    }

    #[tokio::test]
    async fn device_update_mount_point() {
        let p = pool().await;
        let repo = SqliteDeviceRepository::new(p);
        let mut device = MediaDevice::new("DVD Drive", DriveType::CdRom, Utc::now());
        device.enabled = true;
        device.current_mount_point = Some("D:".into());
        repo.upsert(&device).await.expect("upsert");
        repo.update_mount_point(&device.id, Some("G:"))
            .await
            .expect("update");
        let configured = repo.find_configured().await.expect("find");
        assert_eq!(configured[0].current_mount_point, Some("G:".into()));
    }
}
