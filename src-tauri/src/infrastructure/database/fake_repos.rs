//! In-memory fake adapters for use in unit and integration tests.
//!
//! These implement the same interfaces as the SQLite repositories but store
//! data in `HashMap`s. They allow testing domain logic and application use
//! cases without requiring a real database or filesystem.
//!
//! Only compiled in test builds (`#[cfg(test)]`).

#![cfg(test)]

use crate::domain::entities::{
    DeviceId, Game, GameActivation, GameId, LaunchProfile, MediaDescriptor, MediaDevice, ProfileId,
    SessionId,
};
use crate::domain::errors::DomainError;
use crate::domain::ports::{
    ActivationRepository, DeviceRepository, GameRepository, MediaRepository, PortResult,
    ProfileRepository, SessionRecord, SessionRepository, SettingsRepository, TrackedProcessRecord,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ─── Fake Game Repository ────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct FakeGameRepository {
    inner: Arc<Mutex<HashMap<String, Game>>>,
}

impl FakeGameRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_by_id(&self, id: &GameId) -> Result<Option<Game>, DomainError> {
        Ok(self.inner.lock().unwrap().get(&id.to_string()).cloned())
    }

    pub fn find_by_provider_id(
        &self,
        provider: &str,
        provider_id: &str,
    ) -> Result<Option<Game>, DomainError> {
        let guard = self.inner.lock().unwrap();
        Ok(guard
            .values()
            .find(|g| {
                g.provider.to_string() == provider
                    && g.provider_game_id.as_deref() == Some(provider_id)
            })
            .cloned())
    }

    pub fn upsert(&self, game: &Game) -> Result<(), DomainError> {
        self.inner
            .lock()
            .unwrap()
            .insert(game.id.to_string(), game.clone());
        Ok(())
    }

    pub fn list(&self, page: u32, per_page: u32) -> Result<Vec<Game>, DomainError> {
        let guard = self.inner.lock().unwrap();
        let mut games: Vec<Game> = guard.values().cloned().collect();
        games.sort_by(|a, b| a.sort_name.cmp(&b.sort_name));
        let start = (page * per_page) as usize;
        Ok(games
            .into_iter()
            .skip(start)
            .take(per_page as usize)
            .collect())
    }

    pub fn count(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

impl GameRepository for FakeGameRepository {
    fn find_by_id<'a>(&'a self, id: &'a GameId) -> PortResult<'a, Option<Game>> {
        Box::pin(async move { FakeGameRepository::find_by_id(self, id) })
    }

    fn find_by_provider_id<'a>(
        &'a self,
        provider: &'a str,
        provider_id: &'a str,
    ) -> PortResult<'a, Option<Game>> {
        Box::pin(
            async move { FakeGameRepository::find_by_provider_id(self, provider, provider_id) },
        )
    }

    fn upsert<'a>(&'a self, game: &'a Game) -> PortResult<'a, ()> {
        Box::pin(async move { FakeGameRepository::upsert(self, game) })
    }

    fn list(&self, page: u32, per_page: u32) -> PortResult<'_, Vec<Game>> {
        Box::pin(async move { FakeGameRepository::list(self, page, per_page) })
    }
}

// ─── Fake profile and media repositories ─────────────────────────────────────

#[derive(Default, Clone)]
pub struct FakeProfileRepository {
    inner: Arc<Mutex<HashMap<String, LaunchProfile>>>,
}

impl ProfileRepository for FakeProfileRepository {
    fn find_by_id<'a>(&'a self, id: &'a ProfileId) -> PortResult<'a, Option<LaunchProfile>> {
        Box::pin(async move { Ok(self.inner.lock().unwrap().get(&id.to_string()).cloned()) })
    }

    fn find_by_game_id<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, Vec<LaunchProfile>> {
        Box::pin(async move {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .values()
                .filter(|profile| profile.game_id == *game_id)
                .cloned()
                .collect())
        })
    }

    fn upsert<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()> {
        Box::pin(async move {
            self.inner
                .lock()
                .unwrap()
                .insert(profile.id.to_string(), profile.clone());
            Ok(())
        })
    }
}

#[derive(Default, Clone)]
pub struct FakeMediaRepository {
    inner: Arc<Mutex<HashMap<String, MediaDescriptor>>>,
}

impl MediaRepository for FakeMediaRepository {
    fn find_by_key<'a>(&'a self, key: &'a str) -> PortResult<'a, Option<MediaDescriptor>> {
        Box::pin(async move {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .values()
                .find(|media| media.media_key.as_str() == key)
                .cloned())
        })
    }

    fn list(&self) -> PortResult<'_, Vec<MediaDescriptor>> {
        Box::pin(async move { Ok(self.inner.lock().unwrap().values().cloned().collect()) })
    }

    fn upsert<'a>(&'a self, media: &'a MediaDescriptor) -> PortResult<'a, ()> {
        Box::pin(async move {
            self.inner
                .lock()
                .unwrap()
                .insert(media.id.to_string(), media.clone());
            Ok(())
        })
    }
}

#[derive(Default, Clone)]
pub struct FakeActivationRepository {
    inner: Arc<Mutex<HashMap<String, GameActivation>>>,
}

impl ActivationRepository for FakeActivationRepository {
    fn find_by_game_id<'a>(
        &'a self,
        game_id: &'a GameId,
    ) -> PortResult<'a, Option<GameActivation>> {
        Box::pin(async move {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .get(&game_id.to_string())
                .cloned())
        })
    }

    fn list(&self) -> PortResult<'_, Vec<GameActivation>> {
        Box::pin(async move {
            let mut values: Vec<_> = self.inner.lock().unwrap().values().cloned().collect();
            values.sort_by(|left, right| right.last_exported_at.cmp(&left.last_exported_at));
            Ok(values)
        })
    }

    fn upsert<'a>(&'a self, activation: &'a GameActivation) -> PortResult<'a, ()> {
        Box::pin(async move {
            let mut entries = self.inner.lock().unwrap();
            let mut saved = activation.clone();
            if let Some(previous) = entries.get(&activation.game_id.to_string()) {
                saved.first_activated_at = previous.first_activated_at;
                saved.export_count = previous.export_count + 1;
            }
            entries.insert(activation.game_id.to_string(), saved);
            Ok(())
        })
    }

    fn delete<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, bool> {
        Box::pin(async move {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .remove(&game_id.to_string())
                .is_some())
        })
    }
}

// ─── Fake Session Repository ─────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct FakeSessionRepository {
    inner: Arc<Mutex<HashMap<String, SessionRecord>>>,
    processes: Arc<Mutex<HashMap<String, Vec<TrackedProcessRecord>>>>,
}

impl FakeSessionRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_by_id(&self, id: &SessionId) -> Result<Option<SessionRecord>, DomainError> {
        Ok(self.inner.lock().unwrap().get(&id.to_string()).cloned())
    }

    pub fn upsert(&self, record: &SessionRecord) -> Result<(), DomainError> {
        self.inner
            .lock()
            .unwrap()
            .insert(record.id.to_string(), record.clone());
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn find_interrupted(&self) -> Result<Vec<SessionRecord>, DomainError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .values()
            .filter(|record| {
                matches!(
                    record.state_label.as_str(),
                    "running"
                        | "awaiting_process"
                        | "launching"
                        | "closing_requested"
                        | "close_decision"
                        | "forced_closing"
                )
            })
            .cloned()
            .collect())
    }

    pub fn replace_processes(
        &self,
        session_id: &SessionId,
        processes: &[TrackedProcessRecord],
    ) -> Result<(), DomainError> {
        self.processes
            .lock()
            .unwrap()
            .insert(session_id.to_string(), processes.to_vec());
        Ok(())
    }

    pub fn processes_for(&self, session_id: &SessionId) -> Vec<TrackedProcessRecord> {
        self.processes
            .lock()
            .unwrap()
            .get(&session_id.to_string())
            .cloned()
            .unwrap_or_default()
    }
}

impl SessionRepository for FakeSessionRepository {
    fn find_by_id<'a>(&'a self, id: &'a SessionId) -> PortResult<'a, Option<SessionRecord>> {
        Box::pin(async move { FakeSessionRepository::find_by_id(self, id) })
    }

    fn upsert<'a>(&'a self, record: &'a SessionRecord) -> PortResult<'a, ()> {
        Box::pin(async move { FakeSessionRepository::upsert(self, record) })
    }

    fn find_interrupted(&self) -> PortResult<'_, Vec<SessionRecord>> {
        Box::pin(async move { FakeSessionRepository::find_interrupted(self) })
    }

    fn replace_processes<'a>(
        &'a self,
        session_id: &'a SessionId,
        processes: &'a [TrackedProcessRecord],
    ) -> PortResult<'a, ()> {
        Box::pin(
            async move { FakeSessionRepository::replace_processes(self, session_id, processes) },
        )
    }
}

// ─── Fake Settings Repository ─────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct FakeSettingsRepository {
    inner: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

impl FakeSettingsRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Result<Option<serde_json::Value>, DomainError> {
        super::settings::validate_setting_key(key)?;
        Ok(self.inner.lock().unwrap().get(key).cloned())
    }

    pub fn set(&self, key: &str, value: serde_json::Value) -> Result<(), DomainError> {
        super::settings::validate_setting(key, &value)?;
        self.inner.lock().unwrap().insert(key.to_owned(), value);
        Ok(())
    }
}

impl SettingsRepository for FakeSettingsRepository {
    fn get<'a>(&'a self, key: &'a str) -> PortResult<'a, Option<serde_json::Value>> {
        Box::pin(async move { FakeSettingsRepository::get(self, key) })
    }

    fn set<'a>(&'a self, key: &'a str, value: serde_json::Value) -> PortResult<'a, ()> {
        Box::pin(async move { FakeSettingsRepository::set(self, key, value) })
    }
}

// ─── Fake Device Repository ───────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct FakeDeviceRepository {
    inner: Arc<Mutex<HashMap<String, MediaDevice>>>,
}

impl FakeDeviceRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_configured(&self) -> Result<Vec<MediaDevice>, DomainError> {
        let guard = self.inner.lock().unwrap();
        Ok(guard.values().filter(|d| d.enabled).cloned().collect())
    }

    pub fn list_all(&self) -> Result<Vec<MediaDevice>, DomainError> {
        Ok(self.inner.lock().unwrap().values().cloned().collect())
    }

    pub fn find_by_id(&self, id: &DeviceId) -> Result<Option<MediaDevice>, DomainError> {
        Ok(self.inner.lock().unwrap().get(&id.to_string()).cloned())
    }

    pub fn upsert(&self, device: &MediaDevice) -> Result<(), DomainError> {
        self.inner
            .lock()
            .unwrap()
            .insert(device.id.to_string(), device.clone());
        Ok(())
    }

    pub fn update_mount_point(
        &self,
        id: &DeviceId,
        mount_point: Option<&str>,
    ) -> Result<(), DomainError> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(d) = guard.get_mut(&id.to_string()) {
            d.current_mount_point = mount_point.map(str::to_owned);
        }
        Ok(())
    }
}

impl DeviceRepository for FakeDeviceRepository {
    fn list_all(&self) -> PortResult<'_, Vec<MediaDevice>> {
        Box::pin(async move { FakeDeviceRepository::list_all(self) })
    }

    fn find_configured(&self) -> PortResult<'_, Vec<MediaDevice>> {
        Box::pin(async move { FakeDeviceRepository::find_configured(self) })
    }

    fn find_by_id<'a>(&'a self, id: &'a DeviceId) -> PortResult<'a, Option<MediaDevice>> {
        Box::pin(async move { FakeDeviceRepository::find_by_id(self, id) })
    }

    fn upsert<'a>(&'a self, device: &'a MediaDevice) -> PortResult<'a, ()> {
        Box::pin(async move { FakeDeviceRepository::upsert(self, device) })
    }

    fn update_mount_point<'a>(
        &'a self,
        id: &'a DeviceId,
        mount_point: Option<&'a str>,
    ) -> PortResult<'a, ()> {
        Box::pin(async move { FakeDeviceRepository::update_mount_point(self, id, mount_point) })
    }
}

// ─── Tests for the fakes themselves ──────────────────────────────────────────

#[cfg(test)]
mod tests_fakes {
    use super::*;
    use crate::domain::entities::{DriveType, Game, GameProvider, MediaDevice};
    use chrono::Utc;

    #[test]
    fn fake_game_repo_upsert_and_find() {
        let repo = FakeGameRepository::new();
        let game = Game::new(GameProvider::Steam, Some("440".into()), "TF2", Utc::now());
        let id = game.id;
        repo.upsert(&game).expect("upsert");
        let found = repo.find_by_id(&id).expect("find").expect("exists");
        assert_eq!(found.display_name, "TF2");
    }

    #[test]
    fn fake_settings_set_and_get() {
        let repo = FakeSettingsRepository::new();
        repo.set("sound_enabled", serde_json::json!(true))
            .expect("set");
        let val = repo.get("sound_enabled").expect("get");
        assert_eq!(val, Some(serde_json::json!(true)));
    }

    #[test]
    fn fake_session_repo_round_trip() {
        let repo = FakeSessionRepository::new();
        let id = SessionId::new();
        let record = SessionRecord {
            id,
            media_key: "KEY".into(),
            profile_id: ProfileId::new(),
            state_label: "running".into(),
            launch_requested_at: None,
            close_result: None,
        };
        repo.upsert(&record).expect("upsert");
        let found = repo.find_by_id(&id).expect("find").expect("exists");
        assert_eq!(found.state_label, "running");
    }

    #[test]
    fn fake_device_repo_only_returns_enabled() {
        let repo = FakeDeviceRepository::new();
        let mut d1 = MediaDevice::new("Drive 1", DriveType::Removable, Utc::now());
        d1.enabled = true;
        let d2 = MediaDevice::new("Drive 2", DriveType::CdRom, Utc::now());
        // d2.enabled = false (default)
        repo.upsert(&d1).expect("upsert d1");
        repo.upsert(&d2).expect("upsert d2");

        let configured = repo.find_configured().expect("find");
        assert_eq!(configured.len(), 1);
        assert_eq!(configured[0].friendly_name, "Drive 1");
    }
}
