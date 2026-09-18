//! Runtime vertical-slice coordinator: session snapshot + event bridge.

use crate::domain::entities::{MediaKey, SessionId};
use crate::domain::errors::DomainError;
use crate::domain::media_profile::{MediaProfile, MediaProvider};
use crate::domain::ports::SettingsRepository;
use crate::domain::session::{self, SessionEvent, SessionState};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

/// Duração total da animação fictícia no modo normal.
/// Altere somente este valor para ajustar a velocidade padrão.
pub const PHYSICAL_LAUNCH_ANIMATION_MS: u64 = 5_500;
const COVER_RENDER_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStepDto {
    pub id: String,
    pub kind: String,
    pub message_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshotDto {
    pub session_id: Option<String>,
    pub state: String,
    pub media_key: Option<String>,
    pub cover_artwork_id: Option<String>,
    pub display_name: String,
    pub provider_badge: String,
    pub drive_label: String,
    pub mount_point: Option<String>,
    pub progress: u32,
    pub progress_caption: String,
    pub simulated: bool,
    pub close_decision_required: bool,
    pub error_code: Option<String>,
    pub steps: Vec<RuntimeStepDto>,
}

impl Default for SessionSnapshotDto {
    fn default() -> Self {
        Self {
            session_id: None,
            state: "idle".into(),
            media_key: None,
            cover_artwork_id: None,
            display_name: "SYSTEM READY".into(),
            provider_badge: "LOCAL".into(),
            drive_label: "NO DRIVE".into(),
            mount_point: None,
            progress: 0,
            progress_caption: "AWAITING MEDIA".into(),
            simulated: false,
            close_decision_required: false,
            error_code: None,
            steps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettingsDto {
    pub presentation_duration: String,
    pub reduce_motion: bool,
    pub sound_enabled: bool,
}

pub trait RuntimeEventSink: Send + Sync {
    fn state_changed(&self, snapshot: &SessionSnapshotDto) -> Result<(), DomainError>;
    fn step(&self, step: &RuntimeStepDto) -> Result<(), DomainError>;
    fn close_decision_required(&self, session_id: &str) -> Result<(), DomainError>;
}

#[derive(Clone)]
pub struct RuntimeCoordinator {
    inner: Arc<Mutex<RuntimeInner>>,
    settings: Arc<dyn SettingsRepository>,
    sink: Arc<dyn RuntimeEventSink>,
    cover_ready: Arc<Notify>,
    animation_ready: Arc<Notify>,
}

struct RuntimeInner {
    state: SessionState,
    snapshot: SessionSnapshotDto,
    cover_rendered: bool,
    animation_rendered: bool,
}

impl RuntimeCoordinator {
    pub fn new(settings: Arc<dyn SettingsRepository>, sink: Arc<dyn RuntimeEventSink>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeInner {
                state: SessionState::Idle,
                snapshot: SessionSnapshotDto::default(),
                cover_rendered: false,
                animation_rendered: false,
            })),
            settings,
            sink,
            cover_ready: Arc::new(Notify::new()),
            animation_ready: Arc::new(Notify::new()),
        }
    }

    pub fn snapshot(&self) -> SessionSnapshotDto {
        self.inner.lock().unwrap().snapshot.clone()
    }

    pub async fn runtime_settings(&self) -> Result<RuntimeSettingsDto, DomainError> {
        let duration = self
            .settings
            .get("presentation_duration")
            .await?
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "normal".into());
        let reduce_motion = self
            .settings
            .get("reduce_motion")
            .await?
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let sound_enabled = self
            .settings
            .get("sound_enabled")
            .await?
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        Ok(RuntimeSettingsDto {
            presentation_duration: duration,
            reduce_motion,
            sound_enabled,
        })
    }

    pub fn present_valid_media(
        &self,
        profile: &MediaProfile,
        mount_point: &str,
    ) -> Result<SessionSnapshotDto, DomainError> {
        if self.inner.lock().unwrap().state != SessionState::Idle {
            return Err(DomainError::SessionAlreadyActive);
        }
        self.inner.lock().unwrap().cover_rendered = profile.cover_artwork_id.is_none();
        self.inner.lock().unwrap().animation_rendered = false;
        self.apply(
            SessionEvent::MediaInserted(profile.media_id.clone()),
            |snapshot| {
                snapshot.session_id = Some(SessionId::new().to_string());
                snapshot.cover_artwork_id =
                    profile.cover_artwork_id.as_ref().map(ToString::to_string);
                snapshot.display_name.clone_from(&profile.display_name);
                snapshot.provider_badge = match profile.provider {
                    MediaProvider::Steam => "STEAM",
                    MediaProvider::Executable => "LOCAL",
                }
                .into();
                snapshot.drive_label = "MEDIA INSERTED".into();
                snapshot.mount_point = Some(mount_point.into());
                snapshot.progress = 10;
                snapshot.progress_caption = "MEDIA DETECTED".into();
                snapshot.simulated = false;
                snapshot.close_decision_required = false;
                snapshot.error_code = None;
                snapshot.steps.clear();
            },
        )?;
        self.emit_step(RuntimeStepDto {
            id: "detect".into(),
            kind: "success".into(),
            message_key: "runtime.step.media_detected".into(),
            message: Some(format!("MEDIA DETECTED: {mount_point}")),
            minimum_duration_ms: None,
            operation_id: None,
            progress: Some(10),
        })?;
        self.apply(SessionEvent::ValidationStarted, |snapshot| {
            snapshot.progress = 30;
            snapshot.progress_caption = "VALIDATING GAME.INI".into();
        })?;
        self.emit_step(RuntimeStepDto {
            id: "read_ini".into(),
            kind: "success".into(),
            message_key: "runtime.step.reading_profile".into(),
            message: Some("GAME.INI ............ VERIFIED".into()),
            minimum_duration_ms: None,
            operation_id: None,
            progress: Some(35),
        })?;
        self.apply(SessionEvent::ValidationPassed, |snapshot| {
            snapshot.progress = 55;
            snapshot.progress_caption = "PROFILE VALIDATED".into();
        })?;
        self.apply(SessionEvent::ResolutionPassed, |snapshot| {
            snapshot.progress = 70;
            snapshot.progress_caption = "READY TO LAUNCH".into();
        })?;
        self.emit_step(RuntimeStepDto {
            id: "resolve_profile".into(),
            kind: "success".into(),
            message_key: "runtime.step.resolving".into(),
            message: Some("LOCAL PROFILE ....... FOUND".into()),
            minimum_duration_ms: None,
            operation_id: None,
            progress: Some(70),
        })?;
        self.commit(
            SessionState::Presenting {
                media_key: profile.media_id.clone(),
            },
            |snapshot| {
                snapshot.progress = 0;
                snapshot.progress_caption = "READY TO START".into();
            },
        )?;
        Ok(self.snapshot())
    }

    pub async fn complete_physical_presentation(&self) -> Result<SessionSnapshotDto, DomainError> {
        self.wait_for_cover_render().await?;
        let settings = self.runtime_settings().await?;
        let total_duration = match settings.presentation_duration.as_str() {
            "short" => 2_500,
            "cinematic" => 10_000,
            _ => PHYSICAL_LAUNCH_ANIMATION_MS,
        };
        let duration = if settings.reduce_motion {
            40
        } else {
            total_duration / 4
        };
        for (id, message, progress) in [
            ("boot_handshake", "CRT HANDSHAKE ........ OK", 20),
            ("mount_check", "MOUNT INTEGRITY ...... OK", 45),
            ("session_warm", "SESSION BUFFER ....... READY", 70),
            ("ready_pulse", "READY TO LAUNCH", 100),
        ] {
            if self.inner.lock().unwrap().state.label() != "presenting" {
                return Err(DomainError::MediaProfileInvalid);
            }
            self.emit_step(RuntimeStepDto {
                id: id.into(),
                kind: "flavor".into(),
                message_key: format!("runtime.step.{id}"),
                message: Some(message.into()),
                minimum_duration_ms: Some(duration),
                operation_id: None,
                progress: Some(progress),
            })?;
            tokio::time::sleep(Duration::from_millis(duration)).await;
        }
        self.wait_for_animation_render().await?;
        self.apply(SessionEvent::PresentationComplete, |snapshot| {
            snapshot.progress = 100;
            snapshot.progress_caption = "LAUNCHING GAME".into();
        })?;
        self.emit_step(RuntimeStepDto {
            id: "launch".into(),
            kind: "operation".into(),
            message_key: "runtime.step.launch".into(),
            message: Some("SENDING LAUNCH REQUEST".into()),
            minimum_duration_ms: None,
            operation_id: Some("launch".into()),
            progress: Some(100),
        })?;
        Ok(self.snapshot())
    }

    pub fn mark_cover_rendered(&self) {
        self.inner.lock().unwrap().cover_rendered = true;
        self.cover_ready.notify_waiters();
    }

    pub fn mark_animation_rendered(&self) {
        self.inner.lock().unwrap().animation_rendered = true;
        self.animation_ready.notify_waiters();
    }

    pub async fn wait_for_cover_render(&self) -> Result<(), DomainError> {
        loop {
            let notified = self.cover_ready.notified();
            if self.inner.lock().unwrap().cover_rendered {
                return Ok(());
            }
            tokio::time::timeout(COVER_RENDER_TIMEOUT, notified)
                .await
                .map_err(|_| DomainError::ArtworkInvalid)?;
        }
    }

    async fn wait_for_animation_render(&self) -> Result<(), DomainError> {
        loop {
            let notified = self.animation_ready.notified();
            if self.inner.lock().unwrap().animation_rendered {
                return Ok(());
            }
            tokio::time::timeout(Duration::from_secs(2), notified)
                .await
                .map_err(|_| DomainError::MediaIoFailed)?;
        }
    }

    pub fn complete_physical_launch(&self) -> Result<SessionSnapshotDto, DomainError> {
        self.apply(SessionEvent::LaunchSucceeded, |snapshot| {
            snapshot.progress = 98;
            snapshot.progress_caption = "GAME PROCESS DETECTED".into();
        })?;
        self.apply(SessionEvent::ProcessBound, |snapshot| {
            snapshot.progress = 100;
            snapshot.progress_caption = "SESSION RUNNING".into();
        })?;
        self.emit_step(RuntimeStepDto {
            id: "running".into(),
            kind: "success".into(),
            message_key: "runtime.step.running".into(),
            message: Some("GAME SESSION ......... RUNNING".into()),
            minimum_duration_ms: None,
            operation_id: None,
            progress: Some(100),
        })?;
        Ok(self.snapshot())
    }

    pub fn fail_physical_launch(&self) -> Result<SessionSnapshotDto, DomainError> {
        self.commit(
            SessionState::Failed {
                reason: DomainError::LaunchFailed,
            },
            |snapshot| {
                snapshot.progress_caption = "GAME LAUNCH FAILED".into();
                snapshot.error_code = Some("LAUNCH_FAILED".into());
            },
        )?;
        Ok(self.snapshot())
    }

    /// Simulated insert for E2E / demo: truthful state machine, no process kill.
    pub async fn simulate_insert(&self) -> Result<SessionSnapshotDto, DomainError> {
        let settings = self.runtime_settings().await?;
        let media_key = MediaKey::new("SIM-001").expect("static media key");
        let session_id = SessionId::new();
        let reduce = settings.reduce_motion;
        let flavor_ms = match settings.presentation_duration.as_str() {
            "short" => 120u64,
            "cinematic" => 800,
            _ => 350,
        };
        let flavor_ms = if reduce { 40 } else { flavor_ms };

        self.apply(SessionEvent::MediaInserted(media_key.clone()), |snap| {
            snap.session_id = Some(session_id.to_string());
            snap.media_key = Some(media_key.as_str().to_owned());
            snap.display_name = "NEON RUNNER".into();
            snap.provider_badge = "STEAM".into();
            snap.drive_label = "MEDIA INSERTED".into();
            snap.mount_point = Some("A:".into());
            snap.simulated = true;
            snap.progress = 5;
            snap.progress_caption = "MEDIA DETECTED".into();
            snap.error_code = None;
            snap.close_decision_required = false;
            snap.steps.clear();
        })?;

        self.emit_step(RuntimeStepDto {
            id: "detect".into(),
            kind: "operation".into(),
            message_key: "runtime.step.media_detected".into(),
            message: Some(format!("MEDIA DETECTED: {}", media_key.as_str())),
            minimum_duration_ms: None,
            operation_id: Some("validate".into()),
            progress: Some(8),
        })?;

        self.apply(SessionEvent::ValidationStarted, |snap| {
            snap.progress = 15;
            snap.progress_caption = "VALIDATING PROFILE".into();
        })?;
        self.emit_step(RuntimeStepDto {
            id: "read_ini".into(),
            kind: "operation".into(),
            message_key: "runtime.step.reading_profile".into(),
            message: Some("READING GAME.INI".into()),
            minimum_duration_ms: None,
            operation_id: Some("validate".into()),
            progress: Some(25),
        })?;
        self.apply(SessionEvent::ValidationPassed, |_| {})?;

        self.apply(SessionEvent::ResolutionPassed, |snap| {
            snap.progress = 40;
            snap.progress_caption = "PROFILE RESOLVED".into();
        })?;
        self.emit_step(RuntimeStepDto {
            id: "resolve_profile".into(),
            kind: "operation".into(),
            message_key: "runtime.step.resolving".into(),
            message: Some("RESOLVING LOCAL PROFILE".into()),
            minimum_duration_ms: None,
            operation_id: Some("resolve".into()),
            progress: Some(45),
        })?;

        for (id, message, progress) in [
            ("boot_handshake", "CRT HANDSHAKE", 65),
            ("mount_check", "MOUNT INTEGRITY CHECK", 75),
            ("session_warm", "WARMING SESSION BUFFERS", 85),
            ("ready_pulse", "INITIALIZING GAME SESSION", 95),
        ] {
            self.emit_step(RuntimeStepDto {
                id: id.into(),
                kind: if id == "ready_pulse" {
                    "progress".into()
                } else {
                    "flavor".into()
                },
                message_key: format!("runtime.step.{id}"),
                message: Some(message.into()),
                minimum_duration_ms: Some(flavor_ms),
                operation_id: None,
                progress: Some(progress),
            })?;
            tokio::time::sleep(Duration::from_millis(flavor_ms)).await;
            if self.inner.lock().unwrap().snapshot.state == "failed" {
                return Ok(self.snapshot());
            }
        }

        self.apply(SessionEvent::PresentationComplete, |snap| {
            snap.progress = 96;
            snap.progress_caption = "PRESENTATION COMPLETE".into();
        })?;

        // Simulated vertical slice reaches launching→running without a real process.
        self.apply(SessionEvent::LaunchSucceeded, |snap| {
            snap.progress = 97;
            snap.progress_caption = "LAUNCH ACKNOWLEDGED".into();
        })?;
        self.apply(SessionEvent::ProcessBound, |snap| {
            snap.progress = 100;
            snap.progress_caption = "SESSION RUNNING".into();
        })?;
        Ok(self.snapshot())
    }

    pub fn on_media_removed(&self) -> Result<SessionSnapshotDto, DomainError> {
        let event = SessionEvent::MediaRemoved;
        let current = self.inner.lock().unwrap().state.clone();
        if let Ok(next) = session::transition(&current, event.clone()) {
            self.commit(next, |snap| {
                snap.drive_label = "MEDIA REMOVED".into();
                snap.mount_point = None;
                if snap.state == "failed" || snap.state == "closing_requested" {
                    snap.progress_caption = "SEQUENCE CANCELLED".into();
                    snap.error_code = Some("MEDIA_REMOVED".into());
                }
            })?;
        }
        Ok(self.snapshot())
    }

    pub fn reset_after_media_removed(&self) -> Result<SessionSnapshotDto, DomainError> {
        let _ = self.on_media_removed();
        let snapshot = {
            let mut guard = self.inner.lock().unwrap();
            guard.state = SessionState::Idle;
            guard.snapshot = SessionSnapshotDto::default();
            guard.cover_rendered = false;
            guard.animation_rendered = false;
            guard.snapshot.clone()
        };
        self.sink.state_changed(&snapshot)?;
        Ok(snapshot)
    }

    /// Hide the runtime presentation after a Bound game process exits on its own.
    /// Media may still be in the drive; the next play requires eject + reinsert.
    pub fn reset_after_game_exited(&self) -> Result<SessionSnapshotDto, DomainError> {
        let current = self.inner.lock().unwrap().state.clone();
        if matches!(current, SessionState::Running { .. }) {
            let _ = self.apply(SessionEvent::ProcessExited, |snap| {
                snap.progress_caption = "GAME SESSION ENDED".into();
            });
        }
        let snapshot = {
            let mut guard = self.inner.lock().unwrap();
            guard.state = SessionState::Idle;
            guard.snapshot = SessionSnapshotDto::default();
            guard.cover_rendered = false;
            guard.animation_rendered = false;
            guard.snapshot.clone()
        };
        self.sink.state_changed(&snapshot)?;
        Ok(snapshot)
    }

    pub fn mark_close_decision(&self) -> Result<SessionSnapshotDto, DomainError> {
        let media_key = {
            let guard = self.inner.lock().unwrap();
            match &guard.state {
                SessionState::ClosingRequested { media_key }
                | SessionState::Running { media_key } => media_key.clone(),
                _ => return Ok(guard.snapshot.clone()),
            }
        };
        self.apply(SessionEvent::CloseTimedOut, |snap| {
            snap.close_decision_required = true;
            snap.progress_caption = "CLOSE DECISION REQUIRED".into();
        })?;
        if let Some(session_id) = self.snapshot().session_id.clone() {
            self.sink.close_decision_required(&session_id)?;
        }
        let _ = media_key;
        Ok(self.snapshot())
    }

    fn apply(
        &self,
        event: SessionEvent,
        mutate: impl FnOnce(&mut SessionSnapshotDto),
    ) -> Result<(), DomainError> {
        let next = {
            let guard = self.inner.lock().unwrap();
            session::transition(&guard.state, event)?
        };
        self.commit(next, mutate)
    }

    fn commit(
        &self,
        next: SessionState,
        mutate: impl FnOnce(&mut SessionSnapshotDto),
    ) -> Result<(), DomainError> {
        let snapshot = {
            let mut guard = self.inner.lock().unwrap();
            guard.state = next.clone();
            guard.snapshot.state = next.label().to_owned();
            if let Some(key) = media_key_of(&next) {
                guard.snapshot.media_key = Some(key.as_str().to_owned());
            }
            if matches!(next, SessionState::Failed { .. }) {
                guard.snapshot.error_code = Some("SESSION_FAILED".into());
            }
            if matches!(next, SessionState::CloseDecision { .. }) {
                guard.snapshot.close_decision_required = true;
            }
            mutate(&mut guard.snapshot);
            guard.snapshot.clone()
        };
        self.sink.state_changed(&snapshot)?;
        Ok(())
    }

    fn emit_step(&self, step: RuntimeStepDto) -> Result<(), DomainError> {
        {
            let mut guard = self.inner.lock().unwrap();
            if let Some(progress) = step.progress {
                guard.snapshot.progress = progress;
            }
            if let Some(message) = &step.message {
                guard.snapshot.progress_caption = message.clone();
            }
            guard.snapshot.steps.push(step.clone());
            let snapshot = guard.snapshot.clone();
            self.sink.state_changed(&snapshot)?;
        }
        self.sink.step(&step)?;
        Ok(())
    }
}

fn media_key_of(state: &SessionState) -> Option<&MediaKey> {
    match state {
        SessionState::MediaDetected { media_key }
        | SessionState::Validating { media_key }
        | SessionState::Resolving { media_key }
        | SessionState::Presenting { media_key }
        | SessionState::Launching { media_key }
        | SessionState::AwaitingProcess { media_key }
        | SessionState::Running { media_key }
        | SessionState::ClosingRequested { media_key }
        | SessionState::CloseDecision { media_key }
        | SessionState::ForcedClosing { media_key }
        | SessionState::RecoveryRequired { media_key } => Some(media_key),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::ProfileId;
    use crate::domain::media_profile::MediaProfile;
    use crate::infrastructure::database::fake_repos::FakeSettingsRepository;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Sink {
        states: Mutex<Vec<String>>,
    }

    impl RuntimeEventSink for Sink {
        fn state_changed(&self, snapshot: &SessionSnapshotDto) -> Result<(), DomainError> {
            self.states.lock().unwrap().push(snapshot.state.clone());
            Ok(())
        }
        fn step(&self, _step: &RuntimeStepDto) -> Result<(), DomainError> {
            Ok(())
        }
        fn close_decision_required(&self, _session_id: &str) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn simulate_insert_reaches_running_without_auto_kill() {
        let settings = Arc::new(FakeSettingsRepository::new());
        settings
            .set("presentation_duration", serde_json::json!("short"))
            .unwrap();
        settings
            .set("reduce_motion", serde_json::json!(true))
            .unwrap();
        let sink = Arc::new(Sink::default());
        let coordinator = RuntimeCoordinator::new(settings, sink.clone());
        let snapshot = coordinator.simulate_insert().await.unwrap();
        assert_eq!(snapshot.state, "running");
        assert_eq!(snapshot.progress, 100);
        assert!(snapshot.simulated);
        assert!(sink.states.lock().unwrap().contains(&"running".to_owned()));
    }

    #[tokio::test]
    async fn media_removal_cancels_physical_presentation_without_rearming_failure() {
        let settings = Arc::new(FakeSettingsRepository::new());
        settings
            .set("presentation_duration", serde_json::json!("normal"))
            .unwrap();
        settings
            .set("reduce_motion", serde_json::json!(true))
            .unwrap();
        let coordinator = RuntimeCoordinator::new(settings, Arc::new(Sink::default()));
        let profile = MediaProfile {
            media_id: MediaKey::new("RACE-1").unwrap(),
            media_kind: None,
            profile_id: ProfileId::new(),
            created_at: None,
            provider: MediaProvider::Steam,
            app_id: Some(10),
            display_name: "Race".into(),
            process_hints: vec![],
            cover_artwork_id: None,
            cover_cache_path: None,
            artwork_hint: None,
        };
        coordinator.present_valid_media(&profile, "A:").unwrap();
        let running = coordinator.clone();
        let presentation =
            tokio::spawn(async move { running.complete_physical_presentation().await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        coordinator.reset_after_media_removed().unwrap();
        assert!(presentation.await.unwrap().is_err());
        assert_eq!(coordinator.snapshot().state, "idle");
    }
}
