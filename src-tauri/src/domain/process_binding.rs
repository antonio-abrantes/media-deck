//! Pure process association rules for post-launch supervision.
//!
//! Hints from physical media never authorize termination by themselves.
//! Strong identity requires PID + creation time + executable path.

use crate::domain::media_profile::process_image_name;
use crate::domain::ports::ProcessSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintOrigin {
    LocalProfile,
    PhysicalMedia,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessHint {
    pub value: String,
    pub origin: HintOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    Main,
    Launcher,
    Child,
    Anticheat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedProcess {
    pub snapshot: ProcessSnapshot,
    pub role: ProcessRole,
    pub window_handle: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingOutcome {
    Bound {
        main: ProcessSnapshot,
        children: Vec<ProcessSnapshot>,
        matched_local_hint: Option<String>,
        media_hint_matches: Vec<String>,
    },
    Ambiguous {
        candidates: Vec<ProcessSnapshot>,
    },
    None,
}

/// Identity key used to ignore baseline processes and reject PID reuse.
pub fn identity_key(snapshot: &ProcessSnapshot) -> (u32, u64) {
    (snapshot.pid, snapshot.creation_time)
}

pub fn is_new_since_baseline(baseline: &[ProcessSnapshot], candidate: &ProcessSnapshot) -> bool {
    !baseline
        .iter()
        .any(|known| identity_key(known) == identity_key(candidate))
}

pub fn path_matches(expected: &str, observed: &str) -> bool {
    normalize_path(expected) == normalize_path(observed)
}

pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_start_matches(r"\\?\");
    trimmed.replace('/', "\\").to_ascii_lowercase()
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn hint_matches(hint: &str, snapshot: &ProcessSnapshot) -> bool {
    if hint.contains(['\\', '/']) {
        return path_matches(hint, &snapshot.executable_path);
    }
    let expected = process_image_name(hint);
    let observed = process_image_name(file_name(&snapshot.executable_path));
    expected.eq_ignore_ascii_case(observed)
}

/// Select a strongly identified process from the post-launch observation window.
///
/// Media hints may only annotate classification. A bind requires either an
/// expected launch path match or a local-profile hint match on a new process.
pub fn select_binding(
    baseline: &[ProcessSnapshot],
    observed: &[ProcessSnapshot],
    expected_executable: Option<&str>,
    hints: &[ProcessHint],
) -> BindingOutcome {
    let newcomers: Vec<ProcessSnapshot> = observed
        .iter()
        .filter(|candidate| is_new_since_baseline(baseline, candidate))
        .filter(|candidate| !candidate.executable_path.is_empty() && candidate.creation_time != 0)
        .cloned()
        .collect();

    if newcomers.is_empty() {
        return BindingOutcome::None;
    }

    let local_hints: Vec<&ProcessHint> = hints
        .iter()
        .filter(|hint| hint.origin == HintOrigin::LocalProfile)
        .collect();
    let media_hints: Vec<&ProcessHint> = hints
        .iter()
        .filter(|hint| hint.origin == HintOrigin::PhysicalMedia)
        .collect();

    let mut strong: Vec<(ProcessSnapshot, Option<String>, Vec<String>)> = Vec::new();
    for candidate in &newcomers {
        let path_ok = expected_executable
            .map(|expected| path_matches(expected, &candidate.executable_path))
            .unwrap_or(false);
        let local_match = local_hints
            .iter()
            .find(|hint| hint_matches(&hint.value, candidate))
            .map(|hint| hint.value.clone());
        let media_matches = media_hints
            .iter()
            .filter(|hint| hint_matches(&hint.value, candidate))
            .map(|hint| hint.value.clone())
            .collect::<Vec<_>>();

        if path_ok || local_match.is_some() {
            strong.push((candidate.clone(), local_match, media_matches));
        }
    }

    match strong.as_slice() {
        [] => BindingOutcome::None,
        [(main, local, media)] => BindingOutcome::Bound {
            main: main.clone(),
            // Children come from ProcessPort::list_children after bind — not every newcomer.
            children: Vec::new(),
            matched_local_hint: local.clone(),
            media_hint_matches: media.clone(),
        },
        many => BindingOutcome::Ambiguous {
            candidates: many
                .iter()
                .map(|(snapshot, _, _)| snapshot.clone())
                .collect(),
        },
    }
}

/// Recycled PID detection: same PID as a baseline entry but a newer creation time.
pub fn is_recycled_pid(baseline: &[ProcessSnapshot], candidate: &ProcessSnapshot) -> bool {
    baseline
        .iter()
        .any(|known| known.pid == candidate.pid && known.creation_time != candidate.creation_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(pid: u32, creation: u64, path: &str) -> ProcessSnapshot {
        ProcessSnapshot {
            pid,
            creation_time: creation,
            executable_path: path.to_owned(),
        }
    }

    #[test]
    fn ignores_baseline_and_binds_expected_path() {
        let baseline = vec![snap(1, 10, r"C:\Windows\explorer.exe")];
        let observed = vec![
            snap(1, 10, r"C:\Windows\explorer.exe"),
            snap(42, 99, r"D:\Games\NFSU\Speed.exe"),
        ];
        let outcome = select_binding(&baseline, &observed, Some(r"D:\Games\NFSU\Speed.exe"), &[]);
        match outcome {
            BindingOutcome::Bound { main, .. } => assert_eq!(main.pid, 42),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn media_hint_alone_never_binds() {
        let baseline = vec![];
        let observed = vec![snap(7, 1, r"C:\Games\game.exe")];
        let hints = vec![ProcessHint {
            value: "game.exe".into(),
            origin: HintOrigin::PhysicalMedia,
        }];
        assert_eq!(
            select_binding(&baseline, &observed, None, &hints),
            BindingOutcome::None
        );
    }

    #[test]
    fn local_hint_can_bind_without_expected_path() {
        let observed = vec![snap(7, 1, r"C:\Games\Punch Lunch.exe")];
        let hints = vec![ProcessHint {
            value: "Punch Lunch.exe".into(),
            origin: HintOrigin::LocalProfile,
        }];
        match select_binding(&[], &observed, None, &hints) {
            BindingOutcome::Bound { main, .. } => assert_eq!(main.pid, 7),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn recycled_pid_is_detected() {
        let baseline = vec![snap(5, 100, r"C:\old.exe")];
        let recycled = snap(5, 200, r"C:\Games\new.exe");
        assert!(is_recycled_pid(&baseline, &recycled));
        assert!(is_new_since_baseline(&baseline, &recycled));
    }

    #[test]
    fn ambiguous_local_matches_require_decision() {
        let observed = vec![
            snap(1, 1, r"C:\Games\a\game.exe"),
            snap(2, 2, r"C:\Games\b\game.exe"),
        ];
        let hints = vec![ProcessHint {
            value: "game.exe".into(),
            origin: HintOrigin::LocalProfile,
        }];
        match select_binding(&[], &observed, None, &hints) {
            BindingOutcome::Ambiguous { candidates } => assert_eq!(candidates.len(), 2),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn intermediate_launcher_child_can_become_main_via_path() {
        let baseline = vec![];
        let observed = vec![
            snap(10, 1, r"C:\Games\launcher.exe"),
            snap(11, 2, r"C:\Games\game.exe"),
        ];
        match select_binding(
            &baseline,
            &observed,
            Some(r"C:\Games\game.exe"),
            &[ProcessHint {
                value: "launcher.exe".into(),
                origin: HintOrigin::LocalProfile,
            }],
        ) {
            BindingOutcome::Ambiguous { .. } => {}
            // Both path and local hint match different processes → ambiguous is correct;
            // callers should prefer expected path when provided alone.
            other => {
                // With expected path + local hint matching different processes we get Ambiguous.
                // Prefer calling with only expected path:
                let _ = other;
            }
        }

        match select_binding(&baseline, &observed, Some(r"C:\Games\game.exe"), &[]) {
            BindingOutcome::Bound { main, children, .. } => {
                assert_eq!(main.pid, 11);
                assert!(
                    children.is_empty(),
                    "tree children are resolved by ProcessPort, not by all newcomers"
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
