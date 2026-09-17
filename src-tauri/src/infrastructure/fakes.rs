//! Deterministic in-memory adapters used by domain and application tests.

use crate::domain::entities::{Game, GameId, LaunchProfile, MediaDevice, ProfileId};
use crate::domain::errors::DomainError;
#[cfg(windows)]
use crate::domain::optical::{
    OpticalCancel, OpticalPhase, OpticalPort, OpticalProgress, OpticalProgressSink, OpticalRecorder,
};
use crate::domain::ports::{
    ArtworkProvider, CloseResult, DevicePort, GameProvider, MediaWriter, PortResult, ProcessPort,
    ProcessSnapshot,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
#[derive(Clone, Default)]
pub struct FakeOpticalPort {
    pub recorders: Arc<Mutex<Vec<OpticalRecorder>>>,
    pub burned: Arc<Mutex<Vec<String>>>,
    pub erased: Arc<Mutex<Vec<String>>>,
}

#[cfg(windows)]
impl OpticalPort for FakeOpticalPort {
    fn list_recorders(&self) -> Result<Vec<OpticalRecorder>, DomainError> {
        Ok(self.recorders.lock().unwrap().clone())
    }

    fn export_iso(
        &self,
        _staging: &std::path::Path,
        destination: &std::path::Path,
    ) -> Result<(), DomainError> {
        std::fs::write(destination, b"fake-iso").map_err(|_| DomainError::MediaIoFailed)
    }

    fn burn(
        &self,
        _staging: &std::path::Path,
        expected_unique_id: &str,
        cancel: Arc<OpticalCancel>,
        progress: OpticalProgressSink,
    ) -> Result<i32, DomainError> {
        progress(OpticalProgress {
            phase: OpticalPhase::WritingData,
            percent: 50,
            elapsed_seconds: 1,
            remaining_seconds: Some(1),
            cancellation_safe: true,
        });
        if cancel.requested() {
            return Err(DomainError::OpticalBurnFailed);
        }
        self.burned
            .lock()
            .unwrap()
            .push(expected_unique_id.to_owned());
        Ok(3)
    }

    fn erase(&self, expected_unique_id: &str) -> Result<i32, DomainError> {
        self.erased
            .lock()
            .unwrap()
            .push(expected_unique_id.to_owned());
        Ok(3)
    }
}

#[derive(Clone, Default)]
pub struct FakeDevicePort {
    drives: Arc<Mutex<Vec<MediaDevice>>>,
    readiness: Arc<Mutex<HashMap<String, bool>>>,
}

impl FakeDevicePort {
    pub fn set_drives(&self, drives: Vec<MediaDevice>) {
        *self.drives.lock().unwrap() = drives;
    }

    pub fn set_ready(&self, mount_point: impl Into<String>, ready: bool) {
        self.readiness
            .lock()
            .unwrap()
            .insert(mount_point.into(), ready);
    }
}

impl DevicePort for FakeDevicePort {
    fn list_drives(&self) -> PortResult<'_, Vec<MediaDevice>> {
        Box::pin(async move { Ok(self.drives.lock().unwrap().clone()) })
    }

    fn inspect_drive<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, MediaDevice> {
        Box::pin(async move {
            self.drives
                .lock()
                .unwrap()
                .iter()
                .find(|drive| drive.current_mount_point.as_deref() == Some(mount_point))
                .cloned()
                .ok_or(DomainError::MediaNotReady)
        })
    }

    fn is_ready<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, bool> {
        Box::pin(async move {
            Ok(self
                .readiness
                .lock()
                .unwrap()
                .get(mount_point)
                .copied()
                .unwrap_or(false))
        })
    }
}

#[derive(Clone, Default)]
pub struct MemoryMediaWriter {
    profiles: Arc<Mutex<HashMap<String, String>>>,
    write_protected: Arc<Mutex<bool>>,
}

impl MemoryMediaWriter {
    pub fn set_write_protected(&self, protected: bool) {
        *self.write_protected.lock().unwrap() = protected;
    }
}

impl MediaWriter for MemoryMediaWriter {
    fn write_profile<'a>(&'a self, mount_point: &'a str, content: &'a str) -> PortResult<'a, ()> {
        Box::pin(async move {
            if *self.write_protected.lock().unwrap() {
                return Err(DomainError::MediaWriteProtected);
            }
            self.profiles
                .lock()
                .unwrap()
                .insert(mount_point.to_owned(), content.to_owned());
            Ok(())
        })
    }

    fn read_profile<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, String> {
        Box::pin(async move {
            self.profiles
                .lock()
                .unwrap()
                .get(mount_point)
                .cloned()
                .ok_or(DomainError::MediaProfileMissing)
        })
    }
}

#[derive(Clone, Default)]
pub struct FakeGameProvider {
    games: Arc<Mutex<Vec<Game>>>,
    launched_profiles: Arc<Mutex<Vec<ProfileId>>>,
    launch_fails: Arc<Mutex<bool>>,
}

impl FakeGameProvider {
    pub fn set_games(&self, games: Vec<Game>) {
        *self.games.lock().unwrap() = games;
    }

    pub fn set_launch_fails(&self, fails: bool) {
        *self.launch_fails.lock().unwrap() = fails;
    }

    pub fn launched_profiles(&self) -> Vec<ProfileId> {
        self.launched_profiles.lock().unwrap().clone()
    }
}

impl GameProvider for FakeGameProvider {
    fn scan_local(&self) -> PortResult<'_, Vec<Game>> {
        Box::pin(async move { Ok(self.games.lock().unwrap().clone()) })
    }

    fn launch<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()> {
        Box::pin(async move {
            if *self.launch_fails.lock().unwrap() {
                return Err(DomainError::LaunchFailed);
            }
            self.launched_profiles.lock().unwrap().push(profile.id);
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct FakeProcessPort {
    baseline: Arc<Mutex<Vec<ProcessSnapshot>>>,
    close_result: Arc<Mutex<CloseResult>>,
    killed: Arc<Mutex<Vec<(u32, u64)>>>,
}

impl Default for FakeProcessPort {
    fn default() -> Self {
        Self {
            baseline: Arc::new(Mutex::new(Vec::new())),
            close_result: Arc::new(Mutex::new(CloseResult::Exited)),
            killed: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl FakeProcessPort {
    pub fn set_baseline(&self, baseline: Vec<ProcessSnapshot>) {
        *self.baseline.lock().unwrap() = baseline;
    }

    pub fn set_close_result(&self, result: CloseResult) {
        *self.close_result.lock().unwrap() = result;
    }

    pub fn killed_identities(&self) -> Vec<(u32, u64)> {
        self.killed.lock().unwrap().clone()
    }
}

impl ProcessPort for FakeProcessPort {
    fn snapshot_baseline(&self) -> PortResult<'_, Vec<ProcessSnapshot>> {
        Box::pin(async move { Ok(self.baseline.lock().unwrap().clone()) })
    }

    fn list_processes(&self) -> PortResult<'_, Vec<ProcessSnapshot>> {
        Box::pin(async move { Ok(self.baseline.lock().unwrap().clone()) })
    }

    fn revalidate<'a>(&'a self, snapshot: &'a ProcessSnapshot) -> PortResult<'a, bool> {
        Box::pin(async move {
            Ok(self.baseline.lock().unwrap().iter().any(|known| {
                known.pid == snapshot.pid
                    && known.creation_time == snapshot.creation_time
                    && known.executable_path == snapshot.executable_path
            }))
        })
    }

    fn find_main_window<'a>(
        &'a self,
        snapshot: &'a ProcessSnapshot,
    ) -> PortResult<'a, Option<u64>> {
        Box::pin(async move {
            if snapshot.creation_time == 0 {
                Ok(None)
            } else {
                Ok(Some(u64::from(snapshot.pid)))
            }
        })
    }

    fn list_children<'a>(
        &'a self,
        _snapshot: &'a ProcessSnapshot,
    ) -> PortResult<'a, Vec<ProcessSnapshot>> {
        Box::pin(async move { Ok(Vec::new()) })
    }

    fn request_close<'a>(
        &'a self,
        _snapshot: &'a ProcessSnapshot,
        _timeout_secs: u32,
    ) -> PortResult<'a, CloseResult> {
        Box::pin(async move { Ok(self.close_result.lock().unwrap().clone()) })
    }

    fn force_kill<'a>(&'a self, snapshot: &'a ProcessSnapshot) -> PortResult<'a, ()> {
        Box::pin(async move {
            if snapshot.creation_time == 0 || snapshot.executable_path.is_empty() {
                return Err(DomainError::ProcessNotBound);
            }
            self.killed
                .lock()
                .unwrap()
                .push((snapshot.pid, snapshot.creation_time));
            Ok(())
        })
    }
}

#[derive(Clone, Default)]
pub struct FakeArtworkProvider {
    urls: Arc<Mutex<HashMap<GameId, Vec<String>>>>,
}

impl FakeArtworkProvider {
    pub fn set_urls(&self, game_id: GameId, urls: Vec<String>) {
        self.urls.lock().unwrap().insert(game_id, urls);
    }
}

impl ArtworkProvider for FakeArtworkProvider {
    fn fetch_artwork_urls<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, Vec<String>> {
        Box::pin(async move {
            Ok(self
                .urls
                .lock()
                .unwrap()
                .get(game_id)
                .cloned()
                .unwrap_or_default())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{GameProvider as ProviderKind, LaunchKind};
    use chrono::Utc;

    #[tokio::test]
    async fn memory_media_writer_round_trips_and_simulates_protection() {
        let writer = MemoryMediaWriter::default();
        writer.write_profile("A:", "[MEDIA]").await.unwrap();
        assert_eq!(writer.read_profile("A:").await.unwrap(), "[MEDIA]");

        writer.set_write_protected(true);
        assert_eq!(
            writer.write_profile("A:", "changed").await,
            Err(DomainError::MediaWriteProtected)
        );
    }

    #[tokio::test]
    async fn game_provider_records_only_successful_launches() {
        let provider = FakeGameProvider::default();
        let game = Game::new(ProviderKind::Steam, Some("440".into()), "TF2", Utc::now());
        let profile = LaunchProfile::new(
            game.id,
            "Default",
            LaunchKind::Steam { app_id: 440 },
            Utc::now(),
        );

        provider.launch(&profile).await.unwrap();
        assert_eq!(provider.launched_profiles(), vec![profile.id]);
        provider.set_launch_fails(true);
        assert_eq!(
            provider.launch(&profile).await,
            Err(DomainError::LaunchFailed)
        );
    }

    #[tokio::test]
    async fn process_port_rejects_weak_identity_before_force() {
        let port = FakeProcessPort::default();
        let weak = ProcessSnapshot {
            pid: 42,
            creation_time: 0,
            executable_path: String::new(),
        };
        assert_eq!(
            port.force_kill(&weak).await,
            Err(DomainError::ProcessNotBound)
        );
        assert!(port.killed_identities().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn fake_optical_port_emits_progress_without_hardware() {
        let port = FakeOpticalPort::default();
        let phases = Arc::new(Mutex::new(Vec::new()));
        let received = phases.clone();
        port.burn(
            std::path::Path::new("fixture"),
            "recorder-1",
            Arc::new(OpticalCancel::default()),
            Arc::new(move |progress| {
                received.lock().unwrap().push(progress.phase);
            }),
        )
        .unwrap();
        assert_eq!(
            phases.lock().unwrap().as_slice(),
            &[OpticalPhase::WritingData]
        );
        assert_eq!(port.burned.lock().unwrap().as_slice(), &["recorder-1"]);
    }
}
