//! Application data directory management.
//!
//! All MediaDeck user data lives under `%LOCALAPPDATA%\MediaDeck\` on Windows.
//! Sub-directories are created on first launch; the DB file sits at the root.
//!
//! Layout (as per TECHNICAL_SPEC.md §15):
//! ```text
//! %LOCALAPPDATA%\MediaDeck\
//! ├── media-deck.db
//! ├── artwork\
//! ├── labels\
//! ├── exports\
//! ├── logs\
//! ├── backups\
//! └── temp\
//! ```

pub mod dirs;
pub mod tracing_setup;
