//! Phase 5 process supervisor integration tests against real Win32 APIs.
#![cfg(windows)]

use chrono::Utc;
use media_deck_lib::application::session::{
    force_close_all, launch_and_supervise, observe_and_bind, request_close_all, LaunchOutcome,
    ObservationResult, DEFAULT_CLOSE_TIMEOUT_SECS,
};
use media_deck_lib::domain::entities::{GameId, LaunchKind, LaunchProfile};
use media_deck_lib::domain::ports::{CloseResult, GameProvider, ProcessPort};
use media_deck_lib::domain::process_binding::{is_recycled_pid, ProcessRole};
use media_deck_lib::infrastructure::processes::Win32ProcessAdapter;
use media_deck_lib::infrastructure::providers::ExecutableProvider;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static UNIQUE: AtomicU64 = AtomicU64::new(1);

fn fixture_source() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("process-fixture.exe");
    assert!(
        path.is_file(),
        "process-fixture.exe missing at {} — build with `cargo build --bin process-fixture --features process-fixture`",
        path.display()
    );
    path
}

/// Copy the shared fixture to a unique executable so parallel tests cannot collide.
fn unique_fixture(label: &str) -> PathBuf {
    let source = fixture_source();
    let id = UNIQUE.fetch_add(1, Ordering::SeqCst);
    let target = source.with_file_name(format!(
        "process-fixture-{}-{}-{}.exe",
        label,
        std::process::id(),
        id
    ));
    fs::copy(&source, &target).expect("copy unique fixture");
    target
}

fn spawn_fixture(exe: &Path, mode: &str) -> Child {
    Command::new(exe)
        .arg(mode)
        .spawn()
        .unwrap_or_else(|error| panic!("spawn fixture {mode}: {error}"))
}

fn executable_profile(exe: &Path, mode: &str) -> LaunchProfile {
    LaunchProfile::new(GameId::new(), "Fixture", LaunchKind::Executable, Utc::now())
        .with_executable_target(
            exe.to_string_lossy().into_owned(),
            Some(exe.parent().unwrap().to_string_lossy().into_owned()),
            vec![mode.into()],
            vec![exe.file_name().unwrap().to_string_lossy().into_owned()],
        )
        .expect("profile")
}

fn cleanup(exe: &Path, child: Option<&mut Child>) {
    if let Some(child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = fs::remove_file(exe);
}

#[tokio::test]
async fn baseline_pids_are_never_bound_or_killed() {
    let adapter = Win32ProcessAdapter;
    let baseline = adapter.snapshot_baseline().await.expect("baseline");
    let self_pid = std::process::id();
    assert!(baseline.iter().any(|snap| snap.pid == self_pid));

    let exe = unique_fixture("baseline");
    let mut child = spawn_fixture(&exe, "responsive");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let result = observe_and_bind(
        &adapter,
        &baseline,
        Some(&exe.to_string_lossy()),
        &[],
        Duration::from_secs(5),
    )
    .await
    .expect("observe");

    match result {
        ObservationResult::Bound(tracked) => {
            assert!(tracked.iter().all(|item| item.snapshot.pid != self_pid));
            assert!(tracked
                .iter()
                .any(|item| item.role == ProcessRole::Main && item.window_handle.is_some()));
            let _ = request_close_all(&adapter, &tracked, 5).await;
        }
        other => {
            cleanup(&exe, Some(&mut child));
            panic!("expected bind, got {other:?}");
        }
    }

    cleanup(&exe, Some(&mut child));
}

#[tokio::test]
async fn responsive_fixture_exits_on_wm_close() {
    let adapter = Win32ProcessAdapter;
    let provider = ExecutableProvider;
    let exe = unique_fixture("responsive");
    let profile = executable_profile(&exe, "responsive");

    let outcome = launch_and_supervise(&provider, &adapter, &profile, &[], Duration::from_secs(8))
        .await
        .expect("launch");

    let LaunchOutcome::Bound(tracked) = outcome else {
        cleanup(&exe, None);
        panic!("expected bound fixture");
    };
    let result = request_close_all(&adapter, &tracked, DEFAULT_CLOSE_TIMEOUT_SECS)
        .await
        .expect("close");
    cleanup(&exe, None);
    assert!(
        matches!(result, CloseResult::Exited | CloseResult::AlreadyGone),
        "responsive fixture should exit on WM_CLOSE, got {result:?}"
    );
}

#[tokio::test]
async fn ignore_close_fixture_times_out_without_auto_kill() {
    let adapter = Win32ProcessAdapter;
    let provider = ExecutableProvider;
    let exe = unique_fixture("ignore");
    let profile = executable_profile(&exe, "ignore-close");

    let outcome = launch_and_supervise(&provider, &adapter, &profile, &[], Duration::from_secs(8))
        .await
        .expect("launch");

    let LaunchOutcome::Bound(tracked) = outcome else {
        cleanup(&exe, None);
        panic!("expected bound fixture");
    };
    let result = request_close_all(&adapter, &tracked, 2)
        .await
        .expect("close");
    assert_eq!(result, CloseResult::TimedOut);

    force_close_all(&adapter, &tracked).await.expect("force");
    for item in &tracked {
        assert!(!adapter.revalidate(&item.snapshot).await.unwrap());
    }
    cleanup(&exe, None);
}

#[tokio::test]
async fn intermediate_launcher_binds_game_child_by_path() {
    let adapter = Win32ProcessAdapter;
    let provider = ExecutableProvider;
    let launcher = unique_fixture("child");
    let game = fixture_source();
    let profile = LaunchProfile::new(GameId::new(), "Fixture", LaunchKind::Executable, Utc::now())
        .with_executable_target(
            launcher.to_string_lossy().into_owned(),
            Some(launcher.parent().unwrap().to_string_lossy().into_owned()),
            vec!["child".into()],
            // Local hint identifies the real game process started by the launcher.
            vec![game.file_name().unwrap().to_string_lossy().into_owned()],
        )
        .expect("profile");

    // Observe against the game path rather than the short-lived launcher.
    let baseline = adapter.snapshot_baseline().await.expect("baseline");
    provider.launch(&profile).await.expect("launch launcher");
    let result = observe_and_bind(
        &adapter,
        &baseline,
        Some(&game.to_string_lossy()),
        &[],
        Duration::from_secs(8),
    )
    .await
    .expect("observe");

    match result {
        ObservationResult::Bound(tracked) => {
            assert!(
                tracked.iter().any(|item| item.role == ProcessRole::Main),
                "child game must become the main tracked process"
            );
            let _ = force_close_all(&adapter, &tracked).await;
            cleanup(&launcher, None);
        }
        other => {
            cleanup(&launcher, None);
            panic!("launcher child should bind via expected path, got {other:?}");
        }
    }
}

#[test]
fn recycled_pid_helper_rejects_same_pid_new_creation() {
    use media_deck_lib::domain::ports::ProcessSnapshot;
    let baseline = vec![ProcessSnapshot {
        pid: 7,
        creation_time: 100,
        executable_path: r"C:\old.exe".into(),
    }];
    let recycled = ProcessSnapshot {
        pid: 7,
        creation_time: 200,
        executable_path: r"C:\Games\new.exe".into(),
    };
    assert!(is_recycled_pid(&baseline, &recycled));
}
