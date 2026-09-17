//! Stable, time-ordered identifiers for all MediaDeck aggregates.
//!
//! Uses UUID v7, which embeds a millisecond-precision timestamp in the most
//! significant bits, guaranteeing monotonic insertion order in SQLite
//! (stored as TEXT) without a separate `created_at` sort column.
//!
//! IDs are never zeroed by default. Always construct via [`MediaDeckId::new`].

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Injectable source of aggregate identifiers.
pub trait IdGenerator: Send + Sync {
    fn next_id(&self) -> MediaDeckId;
}

/// Production UUID v7 generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemIdGenerator;

impl IdGenerator for SystemIdGenerator {
    fn next_id(&self) -> MediaDeckId {
        MediaDeckId(Uuid::now_v7())
    }
}

/// Opaque, stable identifier shared by all MediaDeck entities.
///
/// Backed by UUID v7 for time-ordered generation. Serialize/deserialize as
/// a hyphenated lowercase string (e.g. `"01939f3a-..."`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaDeckId(Uuid);

impl Default for MediaDeckId {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaDeckId {
    /// Generate a new monotonic UUID v7 identifier.
    pub fn new() -> Self {
        SystemIdGenerator.next_id()
    }

    /// Generate an identifier through an injected source.
    pub fn generate_with(generator: &dyn IdGenerator) -> Self {
        generator.next_id()
    }

    /// Wrap an existing UUID (used when loading from the database).
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Return the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Return the hyphenated string representation (canonical storage format).
    pub fn as_str_hyphenated(&self) -> String {
        self.0.hyphenated().to_string()
    }
}

impl fmt::Display for MediaDeckId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl FromStr for MediaDeckId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

// Default delegates to Self::new() and never produces a nil UUID.

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::str::FromStr;
    use std::sync::Mutex;

    struct SequenceIdGenerator {
        ids: Mutex<VecDeque<MediaDeckId>>,
    }

    impl SequenceIdGenerator {
        fn from_strings(ids: &[&str]) -> Self {
            Self {
                ids: Mutex::new(
                    ids.iter()
                        .map(|id| MediaDeckId::from_str(id).expect("valid fixture UUID"))
                        .collect(),
                ),
            }
        }
    }

    impl IdGenerator for SequenceIdGenerator {
        fn next_id(&self) -> MediaDeckId {
            self.ids
                .lock()
                .expect("sequence ID lock poisoned")
                .pop_front()
                .expect("sequence ID fixture exhausted")
        }
    }

    #[test]
    fn new_produces_different_ids() {
        let a = MediaDeckId::new();
        let b = MediaDeckId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn ids_are_temporally_ordered() {
        let ids: Vec<MediaDeckId> = (0..20).map(|_| MediaDeckId::new()).collect();
        assert!(
            ids.windows(2).all(|pair| pair[0] < pair[1]),
            "UUID v7 generation must preserve creation order"
        );
    }

    #[test]
    fn display_and_parse_round_trip() {
        let id = MediaDeckId::new();
        let s = id.to_string();
        let parsed = MediaDeckId::from_str(&s).expect("must parse back");
        assert_eq!(id, parsed);
    }

    #[test]
    fn serde_round_trip() {
        let id = MediaDeckId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: MediaDeckId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn as_str_hyphenated_matches_display() {
        let id = MediaDeckId::new();
        assert_eq!(id.as_str_hyphenated(), id.to_string());
    }

    #[test]
    fn injected_generator_is_deterministic() {
        let generator = SequenceIdGenerator::from_strings(&[
            "018cc251-f400-7000-8000-000000000001",
            "018cc251-f400-7000-8000-000000000002",
        ]);

        assert_eq!(
            MediaDeckId::generate_with(&generator).to_string(),
            "018cc251-f400-7000-8000-000000000001"
        );
        assert_eq!(
            MediaDeckId::generate_with(&generator).to_string(),
            "018cc251-f400-7000-8000-000000000002"
        );
    }
}
