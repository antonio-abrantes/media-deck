//! Injectable clock abstraction for deterministic domain tests.
//!
//! Production code uses [`SystemClock`]; tests use [`FakeClock`] to
//! advance time explicitly without `std::thread::sleep`.

use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

/// A source of the current UTC instant.
///
/// Implemented by [`SystemClock`] in production and [`FakeClock`] in tests.
/// Must be `Send + Sync` so it can be held behind an `Arc` in async tasks.
pub trait Clock: Send + Sync {
    /// Return the current UTC instant.
    fn now_utc(&self) -> DateTime<Utc>;
}

// ─── Production clock ────────────────────────────────────────────────────────

/// Reads the real system clock via `chrono::Utc::now()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

// ─── Test clock ──────────────────────────────────────────────────────────────

/// A controllable clock backed by a shared mutable instant.
///
/// Start time defaults to `2024-01-01T00:00:00Z` so tests are reproducible.
/// Call [`FakeClock::tick`] to advance time by a fixed duration.
#[derive(Debug, Clone)]
pub struct FakeClock {
    inner: Arc<Mutex<DateTime<Utc>>>,
}

impl FakeClock {
    /// Create a fake clock fixed at `2024-01-01T00:00:00Z`.
    pub fn new() -> Self {
        use chrono::TimeZone;
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        Self {
            inner: Arc::new(Mutex::new(start)),
        }
    }

    /// Create a fake clock fixed at a specific instant.
    pub fn at(instant: DateTime<Utc>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(instant)),
        }
    }

    /// Advance the clock by `duration`.
    pub fn tick(&self, duration: chrono::Duration) {
        let mut guard = self.inner.lock().expect("FakeClock lock poisoned");
        *guard += duration;
    }

    /// Set the clock to an arbitrary instant (useful for large jumps in tests).
    pub fn set(&self, instant: DateTime<Utc>) {
        let mut guard = self.inner.lock().expect("FakeClock lock poisoned");
        *guard = instant;
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now_utc(&self) -> DateTime<Utc> {
        *self.inner.lock().expect("FakeClock lock poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn system_clock_returns_recent_time() {
        let clock = SystemClock;
        let before = Utc::now();
        let t = clock.now_utc();
        let after = Utc::now();
        assert!(t >= before && t <= after);
    }

    #[test]
    fn fake_clock_starts_at_fixed_instant() {
        use chrono::TimeZone;
        let clock = FakeClock::new();
        let expected = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(clock.now_utc(), expected);
    }

    #[test]
    fn fake_clock_tick_advances_time() {
        let clock = FakeClock::new();
        let t0 = clock.now_utc();
        clock.tick(Duration::seconds(30));
        let t1 = clock.now_utc();
        assert_eq!((t1 - t0).num_seconds(), 30);
    }

    #[test]
    fn fake_clock_multiple_ticks_accumulate() {
        let clock = FakeClock::new();
        clock.tick(Duration::seconds(10));
        clock.tick(Duration::minutes(2));
        let t = clock.now_utc();
        // 2024-01-01 00:02:10
        use chrono::TimeZone;
        let expected = Utc.with_ymd_and_hms(2024, 1, 1, 0, 2, 10).unwrap();
        assert_eq!(t, expected);
    }

    #[test]
    fn fake_clock_set_overrides_current() {
        use chrono::TimeZone;
        let clock = FakeClock::new();
        let target = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        clock.set(target);
        assert_eq!(clock.now_utc(), target);
    }

    #[test]
    fn fake_clock_clone_shares_state() {
        let clock = FakeClock::new();
        let clone = clock.clone();
        clock.tick(Duration::seconds(5));
        assert_eq!(clock.now_utc(), clone.now_utc());
    }
}
