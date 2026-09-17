//! Pure session state machine for MediaDeck.
//!
//! This module contains zero async code, zero infrastructure dependencies,
//! and zero side effects. It is a pure function from (State, Event) → State.
//!
//! The state diagram is specified in TECHNICAL_SPEC.md §7.
//!
//! Design rules:
//!   - `transition()` never panics; invalid inputs return `Err(DomainError)`.
//!   - Duplicate `MediaInserted` events in non-Idle states are **idempotent**
//!     (they return the current state without error) to handle OS spurious events.
//!   - Media removal during `Presenting`, `Launching`, or `AwaitingProcess`
//!     cancels the operation and transitions to `Failed`.
//!   - `RecoveryRequired` is a terminal sticky state that only exits through
//!     explicit `UserAcknowledge` with human intervention.

use crate::domain::entities::MediaKey;
use crate::domain::errors::DomainError;
use serde::{Deserialize, Serialize};

// ─── States ───────────────────────────────────────────────────────────────────

/// Every possible state the session coordinator can be in.
///
/// The `#[non_exhaustive]` attribute is intentionally absent — callers (tests)
/// must handle all variants. Adding a state is a breaking, documented change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// No media present, no session active. The normal resting state.
    Idle,

    /// A media insertion event was received; awaiting validation start.
    MediaDetected { media_key: MediaKey },

    /// Reading and parsing `GAME.INI`; validating schema and content.
    Validating { media_key: MediaKey },

    /// Resolving the game from the profile ID against the local library.
    Resolving { media_key: MediaKey },

    /// Showing the retro launch sequence to the user.
    Presenting { media_key: MediaKey },

    /// Sending the launch command to the provider.
    Launching { media_key: MediaKey },

    /// Waiting for the game process to appear (up to 60 s).
    AwaitingProcess { media_key: MediaKey },

    /// Game is running and the session is being supervised.
    Running { media_key: MediaKey },

    /// Media was removed; `WM_CLOSE` was sent; waiting for the process to exit.
    ClosingRequested { media_key: MediaKey },

    /// Close timed out; waiting for user decision: wait / detach / force.
    CloseDecision { media_key: MediaKey },

    /// User chose "force": actively terminating the process tree.
    ForcedClosing { media_key: MediaKey },

    /// The game exited cleanly (or was detached). Session bookkeeping done.
    Completed,

    /// The user chose to detach the session without killing the process.
    Detached,

    /// An unrecoverable error stopped the sequence.
    Failed { reason: DomainError },

    /// App restarted with a session that was `Running`. Requires human
    /// acknowledgement before any action; never auto-kills.
    RecoveryRequired { media_key: MediaKey },
}

impl SessionState {
    /// Short label for logging and IPC (matches database `state` column values).
    pub fn label(&self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::MediaDetected { .. } => "media_detected",
            SessionState::Validating { .. } => "validating",
            SessionState::Resolving { .. } => "resolving",
            SessionState::Presenting { .. } => "presenting",
            SessionState::Launching { .. } => "launching",
            SessionState::AwaitingProcess { .. } => "awaiting_process",
            SessionState::Running { .. } => "running",
            SessionState::ClosingRequested { .. } => "closing_requested",
            SessionState::CloseDecision { .. } => "close_decision",
            SessionState::ForcedClosing { .. } => "forced_closing",
            SessionState::Completed => "completed",
            SessionState::Detached => "detached",
            SessionState::Failed { .. } => "failed",
            SessionState::RecoveryRequired { .. } => "recovery_required",
        }
    }

    /// Whether this is a terminal/resting state (ready for cleanup or idle).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SessionState::Idle
                | SessionState::Completed
                | SessionState::Detached
                | SessionState::Failed { .. }
                | SessionState::RecoveryRequired { .. }
        )
    }
}

// ─── Events ───────────────────────────────────────────────────────────────────

/// Every event that can drive the state machine forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// OS reported media insertion on a configured device.
    MediaInserted(MediaKey),
    /// Validation of the detected media started.
    ValidationStarted,
    /// Validation of `GAME.INI` succeeded.
    ValidationPassed,
    /// Validation failed.
    ValidationFailed(DomainError),
    /// Game resolution against the local library succeeded.
    ResolutionPassed,
    /// Game resolution failed.
    ResolutionFailed(DomainError),
    /// Retro presentation sequence completed.
    PresentationComplete,
    /// Provider confirmed the game started (process observed).
    LaunchSucceeded,
    /// Provider failed to launch the game.
    LaunchFailed(DomainError),
    /// Process was bound with sufficient identity.
    ProcessBound,
    /// Process association timed out or remained ambiguous.
    ProcessBindingFailed(DomainError),
    /// OS reported media removal on the monitored device.
    MediaRemoved,
    /// Supervised process exited on its own.
    ProcessExited,
    /// Close timeout elapsed without the process exiting.
    CloseTimedOut,
    /// User chose to keep waiting.
    UserWait,
    /// User chose to detach the session.
    UserDetach,
    /// User chose to force-kill the process.
    UserForce,
    /// Force-kill completed (process is gone).
    ForceCompleted,
    /// User acknowledged a `Failed` or `RecoveryRequired` state.
    Acknowledged,
}

impl SessionEvent {
    /// Short label for logging.
    pub fn label(&self) -> &'static str {
        match self {
            SessionEvent::MediaInserted(_) => "MediaInserted",
            SessionEvent::ValidationStarted => "ValidationStarted",
            SessionEvent::ValidationPassed => "ValidationPassed",
            SessionEvent::ValidationFailed(_) => "ValidationFailed",
            SessionEvent::ResolutionPassed => "ResolutionPassed",
            SessionEvent::ResolutionFailed(_) => "ResolutionFailed",
            SessionEvent::PresentationComplete => "PresentationComplete",
            SessionEvent::LaunchSucceeded => "LaunchSucceeded",
            SessionEvent::LaunchFailed(_) => "LaunchFailed",
            SessionEvent::ProcessBound => "ProcessBound",
            SessionEvent::ProcessBindingFailed(_) => "ProcessBindingFailed",
            SessionEvent::MediaRemoved => "MediaRemoved",
            SessionEvent::ProcessExited => "ProcessExited",
            SessionEvent::CloseTimedOut => "CloseTimedOut",
            SessionEvent::UserWait => "UserWait",
            SessionEvent::UserDetach => "UserDetach",
            SessionEvent::UserForce => "UserForce",
            SessionEvent::ForceCompleted => "ForceCompleted",
            SessionEvent::Acknowledged => "Acknowledged",
        }
    }
}

// ─── Transition function ──────────────────────────────────────────────────────

/// Apply `event` to `state` and return the next state.
///
/// # Idempotency
///
/// `MediaInserted` in any non-`Idle` state is **silently ignored** (returns
/// `Ok(current_state.clone())`) to handle OS duplicate events gracefully.
///
/// # Errors
///
/// Returns [`DomainError::InvalidTransition`] if the event is not valid in
/// the current state.
pub fn transition(state: &SessionState, event: SessionEvent) -> Result<SessionState, DomainError> {
    use SessionEvent as E;
    use SessionState as S;

    match (state, &event) {
        // ── Idle ──────────────────────────────────────────────────────────────
        (S::Idle, E::MediaInserted(key)) => Ok(S::MediaDetected {
            media_key: key.clone(),
        }),

        // ── MediaDetected ─────────────────────────────────────────────────────
        (S::MediaDetected { media_key }, E::ValidationStarted) => Ok(S::Validating {
            media_key: media_key.clone(),
        }),
        (S::MediaDetected { .. }, E::ValidationFailed(err)) => Ok(S::Failed {
            reason: err.clone(),
        }),
        (S::MediaDetected { .. }, E::MediaRemoved) => Ok(S::Idle),

        // ── Validating ────────────────────────────────────────────────────────
        (S::Validating { media_key }, E::ValidationPassed) => Ok(S::Resolving {
            media_key: media_key.clone(),
        }),
        (S::Validating { .. }, E::ValidationFailed(err)) => Ok(S::Failed {
            reason: err.clone(),
        }),
        // Media removed during validation: cancel, do not associate processes.
        (S::Validating { .. }, E::MediaRemoved) => Ok(S::Failed {
            reason: DomainError::MediaChanged,
        }),

        // ── Resolving ─────────────────────────────────────────────────────────
        (S::Resolving { media_key }, E::ResolutionPassed) => Ok(S::Presenting {
            media_key: media_key.clone(),
        }),
        (S::Resolving { .. }, E::ResolutionFailed(err)) => Ok(S::Failed {
            reason: err.clone(),
        }),
        (S::Resolving { .. }, E::MediaRemoved) => Ok(S::Failed {
            reason: DomainError::MediaChanged,
        }),

        // ── Presenting ────────────────────────────────────────────────────────
        (S::Presenting { media_key }, E::PresentationComplete) => Ok(S::Launching {
            media_key: media_key.clone(),
        }),
        // Media removed during presentation: cancel, do not associate processes.
        (S::Presenting { .. }, E::MediaRemoved) => Ok(S::Failed {
            reason: DomainError::MediaChanged,
        }),

        // ── Launching ─────────────────────────────────────────────────────────
        (S::Launching { media_key }, E::LaunchSucceeded) => Ok(S::AwaitingProcess {
            media_key: media_key.clone(),
        }),
        (S::Launching { .. }, E::LaunchFailed(err)) => Ok(S::Failed {
            reason: err.clone(),
        }),
        // Media removed during launching: cancel before process is bound.
        (S::Launching { .. }, E::MediaRemoved) => Ok(S::Failed {
            reason: DomainError::MediaChanged,
        }),

        // ── AwaitingProcess ───────────────────────────────────────────────────
        (S::AwaitingProcess { media_key }, E::ProcessBound) => Ok(S::Running {
            media_key: media_key.clone(),
        }),
        (S::AwaitingProcess { .. }, E::ProcessBindingFailed(err)) => Ok(S::Failed {
            reason: err.clone(),
        }),
        // Media removed before process bound: cancel, never auto-kill.
        (S::AwaitingProcess { .. }, E::MediaRemoved) => Ok(S::Failed {
            reason: DomainError::MediaChanged,
        }),

        // ── Running ───────────────────────────────────────────────────────────
        (S::Running { media_key }, E::MediaRemoved) => Ok(S::ClosingRequested {
            media_key: media_key.clone(),
        }),
        (S::Running { .. }, E::ProcessExited) => Ok(S::Completed),

        // ── ClosingRequested ──────────────────────────────────────────────────
        (S::ClosingRequested { .. }, E::ProcessExited) => Ok(S::Completed),
        (S::ClosingRequested { media_key }, E::CloseTimedOut) => Ok(S::CloseDecision {
            media_key: media_key.clone(),
        }),
        // Another media removal in this state is a no-op (already closing).
        (S::ClosingRequested { .. }, E::MediaRemoved) => Ok(state.clone()),

        // ── CloseDecision ─────────────────────────────────────────────────────
        (S::CloseDecision { media_key }, E::UserWait) => Ok(S::ClosingRequested {
            media_key: media_key.clone(),
        }),
        (S::CloseDecision { .. }, E::UserDetach) => Ok(S::Detached),
        (S::CloseDecision { media_key }, E::UserForce) => Ok(S::ForcedClosing {
            media_key: media_key.clone(),
        }),
        (S::CloseDecision { .. }, E::MediaRemoved) => Ok(state.clone()),

        // ── ForcedClosing ─────────────────────────────────────────────────────
        (S::ForcedClosing { .. }, E::ForceCompleted) => Ok(S::Completed),
        (S::ForcedClosing { .. }, E::ProcessExited) => Ok(S::Completed),
        (S::ForcedClosing { .. }, E::MediaRemoved) => Ok(state.clone()),

        // ── Terminal states → back to Idle ────────────────────────────────────
        (S::Completed, E::Acknowledged) => Ok(S::Idle),
        (S::Detached, E::Acknowledged) => Ok(S::Idle),
        (S::Failed { .. }, E::Acknowledged) => Ok(S::Idle),

        // ── RecoveryRequired ──────────────────────────────────────────────────
        // Only exits through explicit human acknowledgement.
        (S::RecoveryRequired { .. }, E::Acknowledged) => Ok(S::Idle),

        // ── Idempotent: duplicate MediaInserted in any non-Idle state ─────────
        (_, E::MediaInserted(_)) => Ok(state.clone()),

        // ── Invalid transition ────────────────────────────────────────────────
        _ => Err(DomainError::InvalidTransition {
            state: state.label().to_owned(),
            event: event.label().to_owned(),
        }),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> MediaKey {
        MediaKey::new(s).expect("valid test key")
    }

    fn assert_invalid(from: &SessionState, event: SessionEvent) {
        let result = transition(from, event);
        assert!(
            matches!(result, Err(DomainError::InvalidTransition { .. })),
            "expected InvalidTransition from state '{}', got {:?}",
            from.label(),
            result
        );
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[test]
    fn happy_path_idle_to_running() {
        let s0 = SessionState::Idle;
        let s1 = transition(&s0, SessionEvent::MediaInserted(key("NR-001"))).unwrap();
        assert_eq!(s1.label(), "media_detected");

        let s2 = transition(&s1, SessionEvent::ValidationStarted).unwrap();
        assert_eq!(s2.label(), "validating");

        let s3 = transition(&s2, SessionEvent::ValidationPassed).unwrap();
        assert_eq!(s3.label(), "resolving");

        let s4 = transition(&s3, SessionEvent::ResolutionPassed).unwrap();
        assert_eq!(s4.label(), "presenting");

        let s5 = transition(&s4, SessionEvent::PresentationComplete).unwrap();
        assert_eq!(s5.label(), "launching");

        let s6 = transition(&s5, SessionEvent::LaunchSucceeded).unwrap();
        assert_eq!(s6.label(), "awaiting_process");

        let s7 = transition(&s6, SessionEvent::ProcessBound).unwrap();
        assert_eq!(s7.label(), "running");
    }

    #[test]
    fn graceful_close_path() {
        let running = SessionState::Running {
            media_key: key("NR-001"),
        };
        let closing = transition(&running, SessionEvent::MediaRemoved).unwrap();
        assert_eq!(closing.label(), "closing_requested");

        let completed = transition(&closing, SessionEvent::ProcessExited).unwrap();
        assert_eq!(completed.label(), "completed");

        let idle = transition(&completed, SessionEvent::Acknowledged).unwrap();
        assert_eq!(idle.label(), "idle");
    }

    #[test]
    fn close_timeout_to_decision_to_force() {
        let closing = SessionState::ClosingRequested {
            media_key: key("CP77-001"),
        };
        let decision = transition(&closing, SessionEvent::CloseTimedOut).unwrap();
        assert_eq!(decision.label(), "close_decision");

        let forcing = transition(&decision, SessionEvent::UserForce).unwrap();
        assert_eq!(forcing.label(), "forced_closing");

        let done = transition(&forcing, SessionEvent::ForceCompleted).unwrap();
        assert_eq!(done.label(), "completed");
    }

    #[test]
    fn close_timeout_then_user_wait_resumes_closing() {
        let closing = SessionState::ClosingRequested {
            media_key: key("CP77-001"),
        };
        let decision = transition(&closing, SessionEvent::CloseTimedOut).unwrap();
        let back_to_closing = transition(&decision, SessionEvent::UserWait).unwrap();
        assert_eq!(back_to_closing.label(), "closing_requested");
    }

    #[test]
    fn user_detach_exits_cleanly() {
        let decision = SessionState::CloseDecision {
            media_key: key("X"),
        };
        let detached = transition(&decision, SessionEvent::UserDetach).unwrap();
        assert_eq!(detached.label(), "detached");
        let idle = transition(&detached, SessionEvent::Acknowledged).unwrap();
        assert_eq!(idle.label(), "idle");
    }

    // ── Validation/resolution failures ───────────────────────────────────────

    #[test]
    fn validation_failure_goes_to_failed() {
        let states = vec![
            SessionState::MediaDetected {
                media_key: key("X"),
            },
            SessionState::Validating {
                media_key: key("X"),
            },
        ];
        for s in &states {
            let result = transition(
                s,
                SessionEvent::ValidationFailed(DomainError::MediaProfileInvalid),
            )
            .unwrap();
            assert_eq!(
                result.label(),
                "failed",
                "expected failed from {}",
                s.label()
            );
        }
    }

    #[test]
    fn resolution_failure_goes_to_failed() {
        let s = SessionState::Resolving {
            media_key: key("X"),
        };
        let result = transition(
            &s,
            SessionEvent::ResolutionFailed(DomainError::GameNotInstalled),
        )
        .unwrap();
        assert_eq!(result.label(), "failed");
    }

    #[test]
    fn launch_failure_goes_to_failed() {
        let state = SessionState::Launching {
            media_key: key("X"),
        };
        let result = transition(
            &state,
            SessionEvent::LaunchFailed(DomainError::LaunchFailed),
        )
        .unwrap();
        assert_eq!(
            result,
            SessionState::Failed {
                reason: DomainError::LaunchFailed
            }
        );
    }

    #[test]
    fn process_binding_timeout_goes_to_failed() {
        let state = SessionState::AwaitingProcess {
            media_key: key("X"),
        };
        let result = transition(
            &state,
            SessionEvent::ProcessBindingFailed(DomainError::ProcessNotBound),
        )
        .unwrap();
        assert_eq!(
            result,
            SessionState::Failed {
                reason: DomainError::ProcessNotBound
            }
        );
    }

    // ── Media removed cancellation ───────────────────────────────────────────

    #[test]
    fn media_removed_during_presenting_goes_to_failed() {
        let s = SessionState::Presenting {
            media_key: key("X"),
        };
        let result = transition(&s, SessionEvent::MediaRemoved).unwrap();
        assert_eq!(result.label(), "failed");
    }

    #[test]
    fn media_removed_is_covered_in_every_intermediate_state() {
        let cases = [
            (
                SessionState::MediaDetected {
                    media_key: key("X"),
                },
                "idle",
            ),
            (
                SessionState::Validating {
                    media_key: key("X"),
                },
                "failed",
            ),
            (
                SessionState::Resolving {
                    media_key: key("X"),
                },
                "failed",
            ),
            (
                SessionState::Presenting {
                    media_key: key("X"),
                },
                "failed",
            ),
            (
                SessionState::Launching {
                    media_key: key("X"),
                },
                "failed",
            ),
            (
                SessionState::AwaitingProcess {
                    media_key: key("X"),
                },
                "failed",
            ),
        ];

        for (state, expected) in cases {
            let result = transition(&state, SessionEvent::MediaRemoved).unwrap();
            assert_eq!(result.label(), expected, "state {}", state.label());
        }
    }

    #[test]
    fn process_opened_after_cancellation_is_rejected() {
        let failed = SessionState::Failed {
            reason: DomainError::MediaChanged,
        };
        assert_invalid(&failed, SessionEvent::ProcessBound);
    }

    #[test]
    fn media_removed_during_launching_goes_to_failed() {
        let s = SessionState::Launching {
            media_key: key("X"),
        };
        let result = transition(&s, SessionEvent::MediaRemoved).unwrap();
        assert_eq!(result.label(), "failed");
    }

    #[test]
    fn media_removed_during_awaiting_process_goes_to_failed() {
        let s = SessionState::AwaitingProcess {
            media_key: key("X"),
        };
        let result = transition(&s, SessionEvent::MediaRemoved).unwrap();
        assert_eq!(result.label(), "failed");
    }

    #[test]
    fn media_removed_during_closing_requested_is_noop() {
        let s = SessionState::ClosingRequested {
            media_key: key("X"),
        };
        let result = transition(&s, SessionEvent::MediaRemoved).unwrap();
        assert_eq!(result.label(), "closing_requested"); // stays
    }

    #[test]
    fn duplicate_media_removed_is_idempotent_after_close_started() {
        let states = [
            SessionState::ClosingRequested {
                media_key: key("X"),
            },
            SessionState::CloseDecision {
                media_key: key("X"),
            },
            SessionState::ForcedClosing {
                media_key: key("X"),
            },
        ];

        for state in states {
            let result = transition(&state, SessionEvent::MediaRemoved).unwrap();
            assert_eq!(result, state);
        }
    }

    // ── Idempotency ───────────────────────────────────────────────────────────

    #[test]
    fn duplicate_media_inserted_is_idempotent_in_all_non_idle_states() {
        let non_idle_states = vec![
            SessionState::MediaDetected {
                media_key: key("X"),
            },
            SessionState::Validating {
                media_key: key("X"),
            },
            SessionState::Resolving {
                media_key: key("X"),
            },
            SessionState::Presenting {
                media_key: key("X"),
            },
            SessionState::Launching {
                media_key: key("X"),
            },
            SessionState::AwaitingProcess {
                media_key: key("X"),
            },
            SessionState::Running {
                media_key: key("X"),
            },
            SessionState::ClosingRequested {
                media_key: key("X"),
            },
        ];
        for s in &non_idle_states {
            let event = SessionEvent::MediaInserted(key("ANOTHER-KEY"));
            let result = transition(s, event).unwrap();
            assert_eq!(
                result.label(),
                s.label(),
                "expected idempotent: state '{}' unchanged",
                s.label()
            );
        }
    }

    // ── RecoveryRequired ──────────────────────────────────────────────────────

    #[test]
    fn recovery_required_only_exits_via_acknowledge() {
        let s = SessionState::RecoveryRequired {
            media_key: key("X"),
        };
        assert!(
            s.is_terminal(),
            "recovery is sticky until human acknowledgement"
        );
        // MediaInserted is idempotent
        let still_recovery = transition(&s, SessionEvent::MediaInserted(key("Y"))).unwrap();
        assert_eq!(still_recovery.label(), "recovery_required");
        // Acknowledge exits
        let idle = transition(&s, SessionEvent::Acknowledged).unwrap();
        assert_eq!(idle.label(), "idle");
    }

    // ── Invalid transitions ───────────────────────────────────────────────────

    #[test]
    fn idle_rejects_events_other_than_media_inserted() {
        let idle = SessionState::Idle;
        assert_invalid(&idle, SessionEvent::ValidationPassed);
        assert_invalid(&idle, SessionEvent::ProcessBound);
        assert_invalid(&idle, SessionEvent::MediaRemoved);
        assert_invalid(&idle, SessionEvent::UserForce);
    }

    #[test]
    fn running_rejects_non_removal_events() {
        let s = SessionState::Running {
            media_key: key("X"),
        };
        assert_invalid(&s, SessionEvent::ValidationPassed);
        assert_invalid(&s, SessionEvent::LaunchSucceeded);
        assert_invalid(&s, SessionEvent::UserForce);
    }

    #[test]
    fn completed_only_accepts_acknowledged() {
        let s = SessionState::Completed;
        assert_invalid(&s, SessionEvent::MediaRemoved);
        assert_invalid(&s, SessionEvent::ProcessExited);
        let idle = transition(&s, SessionEvent::Acknowledged).unwrap();
        assert_eq!(idle.label(), "idle");
    }

    // ── Label coverage ────────────────────────────────────────────────────────

    #[test]
    fn all_state_labels_are_non_empty_snake_case() {
        let states = vec![
            SessionState::Idle,
            SessionState::MediaDetected {
                media_key: key("X"),
            },
            SessionState::Validating {
                media_key: key("X"),
            },
            SessionState::Resolving {
                media_key: key("X"),
            },
            SessionState::Presenting {
                media_key: key("X"),
            },
            SessionState::Launching {
                media_key: key("X"),
            },
            SessionState::AwaitingProcess {
                media_key: key("X"),
            },
            SessionState::Running {
                media_key: key("X"),
            },
            SessionState::ClosingRequested {
                media_key: key("X"),
            },
            SessionState::CloseDecision {
                media_key: key("X"),
            },
            SessionState::ForcedClosing {
                media_key: key("X"),
            },
            SessionState::Completed,
            SessionState::Detached,
            SessionState::Failed {
                reason: DomainError::MediaNotReady,
            },
            SessionState::RecoveryRequired {
                media_key: key("X"),
            },
        ];
        for s in &states {
            let label = s.label();
            assert!(!label.is_empty());
            assert!(label.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }
}
