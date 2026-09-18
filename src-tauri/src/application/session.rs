//! Launch, bind and close orchestration for a game session.

use crate::domain::entities::{ClosePolicy, LaunchKind, LaunchProfile, MediaKey, SessionId};
use crate::domain::errors::DomainError;
use crate::domain::ports::{
    CloseResult, GameProvider, ProcessPort, ProcessSnapshot, SessionRecord, SessionRepository,
    TrackedProcessRecord,
};
use crate::domain::process_binding::{
    select_binding, BindingOutcome, HintOrigin, ProcessHint, ProcessRole, TrackedProcess,
};
use crate::domain::session::{self, SessionEvent, SessionState};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Spec default: observe newcomers for up to 60 seconds after launch.
pub const DEFAULT_OBSERVE_TIMEOUT: Duration = Duration::from_secs(60);

/// Spec default: wait 15 seconds after WM_CLOSE before asking the user.
pub const DEFAULT_CLOSE_TIMEOUT_SECS: u32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseDecision {
    Wait,
    Detach,
    Force,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationResult {
    Bound(Vec<TrackedProcess>),
    TimedOut,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOutcome {
    Bound(Vec<TrackedProcess>),
    /// Launch succeeded but identity was insufficient — never auto-kill.
    Unsupervised,
}

#[derive(Debug, Clone)]
struct LiveSession {
    media_key: MediaKey,
    tracked: Vec<TrackedProcess>,
    state: SessionState,
}

/// Coordinates launch, binding, close decisions and crash recovery.
#[derive(Clone)]
pub struct SessionService {
    processes: Arc<dyn ProcessPort>,
    sessions: Arc<dyn SessionRepository>,
    live: Arc<Mutex<HashMap<String, LiveSession>>>,
}

impl SessionService {
    pub fn new(processes: Arc<dyn ProcessPort>, sessions: Arc<dyn SessionRepository>) -> Self {
        Self {
            processes,
            sessions,
            live: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Mark interrupted live sessions as recovery-required without killing anything.
    pub async fn recover_on_startup(&self) -> Result<usize, DomainError> {
        let interrupted = self.sessions.find_interrupted().await?;
        let mut count = 0;
        for record in interrupted {
            let recovered = recover_interrupted_session(self.sessions.as_ref(), &record).await?;
            if recovered.state_label == "recovery_required" {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Launch through the provider, observe, persist identities and keep them in memory.
    pub async fn launch_and_track(
        &self,
        session_id: SessionId,
        media_key: MediaKey,
        provider: &dyn GameProvider,
        profile: &LaunchProfile,
        media_hints: &[String],
        timeout: Duration,
    ) -> Result<LaunchOutcome, DomainError> {
        let launch_requested_at = Utc::now().to_rfc3339();
        self.sessions
            .upsert(&SessionRecord {
                id: session_id,
                media_key: media_key.as_str().to_owned(),
                profile_id: profile.id,
                state_label: "awaiting_process".into(),
                launch_requested_at: Some(launch_requested_at),
                close_result: None,
            })
            .await?;

        let outcome = launch_and_supervise(
            provider,
            self.processes.as_ref(),
            profile,
            media_hints,
            timeout,
        )
        .await?;

        match &outcome {
            LaunchOutcome::Bound(tracked) => {
                self.sessions
                    .replace_processes(&session_id, &to_records(tracked))
                    .await?;
                let existing = self.sessions.find_by_id(&session_id).await?;
                self.sessions
                    .upsert(&SessionRecord {
                        id: session_id,
                        media_key: media_key.as_str().to_owned(),
                        profile_id: profile.id,
                        state_label: "running".into(),
                        launch_requested_at: existing.and_then(|r| r.launch_requested_at),
                        close_result: None,
                    })
                    .await?;
                self.live.lock().unwrap().insert(
                    session_id.to_string(),
                    LiveSession {
                        media_key: media_key.clone(),
                        tracked: tracked.clone(),
                        state: SessionState::Running {
                            media_key: media_key.clone(),
                        },
                    },
                );
            }
            LaunchOutcome::Unsupervised => {
                self.sessions.replace_processes(&session_id, &[]).await?;
                let existing = self.sessions.find_by_id(&session_id).await?;
                self.sessions
                    .upsert(&SessionRecord {
                        id: session_id,
                        media_key: media_key.as_str().to_owned(),
                        profile_id: profile.id,
                        state_label: "running".into(),
                        launch_requested_at: existing.and_then(|r| r.launch_requested_at),
                        close_result: Some("not_bound".into()),
                    })
                    .await?;
                self.live.lock().unwrap().insert(
                    session_id.to_string(),
                    LiveSession {
                        media_key: media_key.clone(),
                        tracked: Vec::new(),
                        state: SessionState::Running { media_key },
                    },
                );
            }
        }
        Ok(outcome)
    }

    /// True while this session still has an in-memory live record.
    pub fn is_live(&self, session_id: &SessionId) -> bool {
        self.live
            .lock()
            .unwrap()
            .contains_key(&session_id.to_string())
    }

    /// Poll until every strongly tracked identity is gone, or the live session ends.
    ///
    /// Used after a Bound launch so the runtime window can hide when the player
    /// quits the game without ejecting media. Unsupervised sessions never enter
    /// this wait (empty tracked list returns immediately).
    pub async fn wait_until_tracked_gone(
        &self,
        session_id: &SessionId,
    ) -> Result<(), DomainError> {
        let poll = Duration::from_millis(500);
        loop {
            let tracked = {
                let live = self.live.lock().unwrap();
                match live.get(&session_id.to_string()) {
                    Some(session) => session.tracked.clone(),
                    None => return Ok(()),
                }
            };
            if tracked.is_empty() {
                return Ok(());
            }
            let mut any_alive = false;
            for item in &tracked {
                if self.processes.revalidate(&item.snapshot).await? {
                    any_alive = true;
                    break;
                }
            }
            if !any_alive {
                return Ok(());
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// Finish a Running Bound session after the game process exited by itself.
    ///
    /// Returns `true` when this call owned the completion. Returns `false` when
    /// the live session is already gone or not in `Running` (e.g. media eject
    /// took over), so callers must not hide/reset again on a false result.
    pub async fn complete_after_spontaneous_exit(
        &self,
        session_id: &SessionId,
    ) -> Result<bool, DomainError> {
        let current = {
            let live = self.live.lock().unwrap();
            live.get(&session_id.to_string())
                .map(|session| session.state.clone())
        };
        let Some(state) = current else {
            return Ok(false);
        };
        if !matches!(state, SessionState::Running { .. }) {
            return Ok(false);
        }
        let completed = session::transition(&state, SessionEvent::ProcessExited)?;
        self.finish_session(session_id, &completed, Some("process_exited"))
            .await?;
        Ok(true)
    }

    /// Drop the in-memory live session without changing persisted close_result.
    /// Used when media removal already closed the game and reset the runtime.
    pub fn drop_live(&self, session_id: &SessionId) {
        self.live.lock().unwrap().remove(&session_id.to_string());
    }

    #[cfg(test)]
    pub fn seed_running_for_test(
        &self,
        session_id: SessionId,
        media_key: MediaKey,
        tracked: Vec<TrackedProcess>,
    ) {
        self.live.lock().unwrap().insert(
            session_id.to_string(),
            LiveSession {
                media_key: media_key.clone(),
                tracked,
                state: SessionState::Running { media_key },
            },
        );
    }

    /// Request graceful close for a tracked session. Empty tracked lists are no-ops.
    pub async fn request_close(
        &self,
        session_id: &SessionId,
        timeout_secs: u32,
    ) -> Result<CloseResult, DomainError> {
        let tracked = {
            let live = self.live.lock().unwrap();
            live.get(&session_id.to_string())
                .map(|session| session.tracked.clone())
                .unwrap_or_default()
        };
        if tracked.is_empty() {
            return Ok(CloseResult::AlreadyGone);
        }

        let result = request_close_all(self.processes.as_ref(), &tracked, timeout_secs).await?;
        if matches!(result, CloseResult::TimedOut) {
            let mut live = self.live.lock().unwrap();
            if let Some(session) = live.get_mut(&session_id.to_string()) {
                session.state = SessionState::CloseDecision {
                    media_key: session.media_key.clone(),
                };
            }
        }
        Ok(result)
    }

    /// Apply wait/detach/force. Force revalidates identities before kill.
    pub async fn resolve_close(
        &self,
        session_id: &SessionId,
        media_key: &str,
        decision: CloseDecision,
    ) -> Result<SessionState, DomainError> {
        let (current, tracked) = {
            let live = self.live.lock().unwrap();
            let session = live
                .get(&session_id.to_string())
                .ok_or(DomainError::ProcessNotBound)?;
            if session.media_key.as_str() != media_key {
                return Err(DomainError::ProcessNotBound);
            }
            (session.state.clone(), session.tracked.clone())
        };

        let next = apply_close_decision(&current, decision)?;
        match decision {
            CloseDecision::Force => {
                force_close_all(self.processes.as_ref(), &tracked).await?;
                let completed = session::transition(&next, SessionEvent::ForceCompleted)?;
                self.finish_session(session_id, &completed, Some("forced"))
                    .await?;
                Ok(completed)
            }
            CloseDecision::Detach => {
                self.finish_session(session_id, &next, Some("detached"))
                    .await?;
                Ok(next)
            }
            CloseDecision::Wait => {
                let mut live = self.live.lock().unwrap();
                if let Some(session) = live.get_mut(&session_id.to_string()) {
                    session.state = next.clone();
                }
                Ok(next)
            }
        }
    }

    async fn finish_session(
        &self,
        session_id: &SessionId,
        state: &SessionState,
        close_result: Option<&str>,
    ) -> Result<(), DomainError> {
        if let Some(existing) = self.sessions.find_by_id(session_id).await? {
            self.sessions
                .upsert(&SessionRecord {
                    id: *session_id,
                    media_key: existing.media_key,
                    profile_id: existing.profile_id,
                    state_label: state.label().to_owned(),
                    launch_requested_at: existing.launch_requested_at,
                    close_result: close_result.map(str::to_owned).or(existing.close_result),
                })
                .await?;
        }
        self.live.lock().unwrap().remove(&session_id.to_string());
        Ok(())
    }
}

async fn collect_tracked(
    processes: &dyn ProcessPort,
    main: ProcessSnapshot,
    children: Vec<ProcessSnapshot>,
    child_role: ProcessRole,
) -> Result<Vec<TrackedProcess>, DomainError> {
    let window = processes.find_main_window(&main).await?;
    let mut tracked = vec![TrackedProcess {
        snapshot: main,
        role: ProcessRole::Main,
        window_handle: window,
    }];
    for child in children {
        let child_window = processes.find_main_window(&child).await?;
        tracked.push(TrackedProcess {
            snapshot: child,
            role: child_role,
            window_handle: child_window,
        });
    }
    Ok(tracked)
}

/// Observe newcomers until a strong binding appears or the timeout elapses.
pub async fn observe_and_bind(
    processes: &dyn ProcessPort,
    baseline: &[ProcessSnapshot],
    expected_executable: Option<&str>,
    hints: &[ProcessHint],
    timeout: Duration,
) -> Result<ObservationResult, DomainError> {
    let deadline = Instant::now() + timeout;
    loop {
        let observed = processes.list_processes().await?;
        match select_binding(baseline, &observed, expected_executable, hints) {
            BindingOutcome::Bound { main, .. } => {
                let children = processes.list_children(&main).await?;
                let tracked =
                    collect_tracked(processes, main, children, ProcessRole::Child).await?;
                return Ok(ObservationResult::Bound(tracked));
            }
            BindingOutcome::Ambiguous { .. } => return Ok(ObservationResult::Ambiguous),
            BindingOutcome::None => {}
        }
        if Instant::now() >= deadline {
            return Ok(ObservationResult::TimedOut);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Launch through the selected provider, then bind a new process with strong identity.
pub async fn launch_and_supervise(
    provider: &dyn GameProvider,
    processes: &dyn ProcessPort,
    profile: &LaunchProfile,
    media_hints: &[String],
    timeout: Duration,
) -> Result<LaunchOutcome, DomainError> {
    let baseline = processes.snapshot_baseline().await?;
    provider.launch(profile).await?;

    let mut hints = profile
        .process_hints
        .iter()
        .cloned()
        .map(|value| ProcessHint {
            value,
            origin: HintOrigin::LocalProfile,
        })
        .collect::<Vec<_>>();
    hints.extend(media_hints.iter().cloned().map(|value| ProcessHint {
        value,
        origin: HintOrigin::PhysicalMedia,
    }));

    let expected = match &profile.launch_kind {
        LaunchKind::Executable => profile.executable_path.as_deref().map(|path| {
            std::fs::canonicalize(path)
                .ok()
                .map(|canonical| {
                    let text = canonical.to_string_lossy();
                    text.strip_prefix(r"\\?\")
                        .unwrap_or(text.as_ref())
                        .to_owned()
                })
                .unwrap_or_else(|| path.to_owned())
        }),
        LaunchKind::Steam { .. } => None,
    };
    let expected = expected.as_deref();

    match observe_and_bind(processes, &baseline, expected, &hints, timeout).await? {
        ObservationResult::Bound(tracked) => Ok(LaunchOutcome::Bound(tracked)),
        ObservationResult::TimedOut | ObservationResult::Ambiguous => {
            Ok(LaunchOutcome::Unsupervised)
        }
    }
}

/// Request graceful close for every tracked identity. Never kills by name.
pub async fn request_close_all(
    processes: &dyn ProcessPort,
    tracked: &[TrackedProcess],
    timeout_secs: u32,
) -> Result<CloseResult, DomainError> {
    let mut timed_out = false;
    let mut exited = false;
    for item in tracked {
        if !processes.revalidate(&item.snapshot).await? {
            continue;
        }
        match processes
            .request_close(&item.snapshot, timeout_secs)
            .await?
        {
            CloseResult::TimedOut => timed_out = true,
            CloseResult::Exited => exited = true,
            CloseResult::AlreadyGone => {}
        }
    }
    if timed_out {
        Ok(CloseResult::TimedOut)
    } else if exited {
        Ok(CloseResult::Exited)
    } else {
        Ok(CloseResult::AlreadyGone)
    }
}

/// Force-kill only after revalidating every identity.
pub async fn force_close_all(
    processes: &dyn ProcessPort,
    tracked: &[TrackedProcess],
) -> Result<(), DomainError> {
    for item in tracked {
        if processes.revalidate(&item.snapshot).await? {
            processes.force_kill(&item.snapshot).await?;
        }
    }
    Ok(())
}

pub fn apply_close_decision(
    state: &SessionState,
    decision: CloseDecision,
) -> Result<SessionState, DomainError> {
    let event = match decision {
        CloseDecision::Wait => SessionEvent::UserWait,
        CloseDecision::Detach => SessionEvent::UserDetach,
        CloseDecision::Force => SessionEvent::UserForce,
    };
    session::transition(state, event)
}

/// Mark an interrupted running session as recovery-required without killing anything.
pub async fn recover_interrupted_session(
    sessions: &dyn SessionRepository,
    record: &SessionRecord,
) -> Result<SessionRecord, DomainError> {
    if record.state_label != "running"
        && record.state_label != "awaiting_process"
        && record.state_label != "launching"
        && record.state_label != "closing_requested"
        && record.state_label != "close_decision"
        && record.state_label != "forced_closing"
    {
        return Ok(record.clone());
    }
    let media_key = MediaKey::new(record.media_key.clone())
        .unwrap_or_else(|_| MediaKey::new("RECOVERY").expect("static"));
    let recovered = SessionRecord {
        id: record.id,
        media_key: record.media_key.clone(),
        profile_id: record.profile_id,
        state_label: SessionState::RecoveryRequired { media_key }
            .label()
            .to_owned(),
        launch_requested_at: record.launch_requested_at.clone(),
        close_result: record.close_result.clone(),
    };
    sessions.upsert(&recovered).await?;
    Ok(recovered)
}

pub fn close_timeout_secs(policy: &ClosePolicy) -> u32 {
    match policy {
        ClosePolicy::WaitAndAsk { timeout_secs }
        | ClosePolicy::ForceAfterTimeout { timeout_secs } => *timeout_secs,
        ClosePolicy::Detach => 0,
    }
}

fn to_records(tracked: &[TrackedProcess]) -> Vec<TrackedProcessRecord> {
    tracked
        .iter()
        .map(|item| TrackedProcessRecord {
            pid: item.snapshot.pid,
            creation_time: item.snapshot.creation_time,
            executable_path: item.snapshot.executable_path.clone(),
            role: match item.role {
                ProcessRole::Main => "main",
                ProcessRole::Launcher => "launcher",
                ProcessRole::Child => "child",
                ProcessRole::Anticheat => "anticheat",
            }
            .to_owned(),
            window_handle: item.window_handle,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{GameId, LaunchKind, ProfileId};
    use crate::domain::ports::ProcessSnapshot;
    use crate::infrastructure::database::fake_repos::FakeSessionRepository;
    use crate::infrastructure::fakes::{FakeGameProvider, FakeProcessPort};
    use std::sync::Arc;

    #[tokio::test]
    async fn observe_binds_new_expected_process_and_ignores_baseline() {
        let port = FakeProcessPort::default();
        let baseline = vec![ProcessSnapshot {
            pid: 1,
            creation_time: 10,
            executable_path: r"C:\Windows\explorer.exe".into(),
        }];
        port.set_baseline(vec![
            baseline[0].clone(),
            ProcessSnapshot {
                pid: 99,
                creation_time: 50,
                executable_path: r"D:\Games\Speed.exe".into(),
            },
        ]);

        let result = observe_and_bind(
            &port,
            &baseline,
            Some(r"D:\Games\Speed.exe"),
            &[],
            Duration::from_millis(50),
        )
        .await
        .unwrap();

        match result {
            ObservationResult::Bound(tracked) => assert_eq!(tracked[0].snapshot.pid, 99),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn force_close_skips_invalid_identity() {
        let port = FakeProcessPort::default();
        let tracked = vec![TrackedProcess {
            snapshot: ProcessSnapshot {
                pid: 1,
                creation_time: 0,
                executable_path: String::new(),
            },
            role: ProcessRole::Main,
            window_handle: None,
        }];
        force_close_all(&port, &tracked).await.unwrap();
        assert!(port.killed_identities().is_empty());
    }

    #[test]
    fn close_decision_maps_to_state_machine() {
        let state = SessionState::CloseDecision {
            media_key: MediaKey::new("GAME-1").unwrap(),
        };
        let next = apply_close_decision(&state, CloseDecision::Detach).unwrap();
        assert!(matches!(next, SessionState::Detached));
    }

    #[tokio::test]
    async fn recovery_marks_running_without_kill() {
        let repo = FakeSessionRepository::new();
        let record = SessionRecord {
            id: SessionId::new(),
            media_key: "GAME-1".into(),
            profile_id: ProfileId::new(),
            state_label: "running".into(),
            launch_requested_at: Some("t0".into()),
            close_result: None,
        };
        repo.upsert(&record).unwrap();
        let service = SessionService::new(Arc::new(FakeProcessPort::default()), Arc::new(repo));
        assert_eq!(service.recover_on_startup().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn unsupervised_launch_never_enables_auto_kill() {
        let processes = FakeProcessPort::default();
        processes.set_baseline(vec![ProcessSnapshot {
            pid: 1,
            creation_time: 1,
            executable_path: r"C:\Windows\explorer.exe".into(),
        }]);
        let provider = FakeGameProvider::default();
        let profile =
            LaunchProfile::new(GameId::new(), "Local", LaunchKind::Executable, Utc::now());
        let outcome = launch_and_supervise(
            &provider,
            &processes,
            &profile,
            &[],
            Duration::from_millis(30),
        )
        .await
        .unwrap();
        assert_eq!(outcome, LaunchOutcome::Unsupervised);
        assert!(processes.killed_identities().is_empty());
    }

    #[tokio::test]
    async fn wait_until_tracked_gone_returns_when_process_disappears() {
        let processes = FakeProcessPort::default();
        let snapshot = ProcessSnapshot {
            pid: 42,
            creation_time: 7,
            executable_path: r"D:\Games\Bound.exe".into(),
        };
        processes.set_baseline(vec![snapshot.clone()]);
        let sessions = Arc::new(FakeSessionRepository::default());
        let service = SessionService::new(Arc::new(processes.clone()), sessions);
        let session_id = SessionId::new();
        let media_key = MediaKey::new("DISK-1").unwrap();
        service.seed_running_for_test(
            session_id,
            media_key,
            vec![TrackedProcess {
                snapshot,
                role: ProcessRole::Main,
                window_handle: Some(1),
            }],
        );

        let waiter = {
            let service = service.clone();
            tokio::spawn(async move { service.wait_until_tracked_gone(&session_id).await })
        };
        tokio::time::sleep(Duration::from_millis(80)).await;
        processes.set_baseline(Vec::new());
        waiter.await.unwrap().unwrap();
        assert!(service
            .complete_after_spontaneous_exit(&session_id)
            .await
            .unwrap());
        assert!(!service.is_live(&session_id));
        assert!(!service
            .complete_after_spontaneous_exit(&session_id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn spontaneous_exit_ignored_when_session_not_running() {
        let processes = FakeProcessPort::default();
        let sessions = Arc::new(FakeSessionRepository::default());
        let service = SessionService::new(Arc::new(processes), sessions);
        let session_id = SessionId::new();
        assert!(!service
            .complete_after_spontaneous_exit(&session_id)
            .await
            .unwrap());
    }
}
