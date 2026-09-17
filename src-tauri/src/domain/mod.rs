//! Pure MediaDeck domain — no Tauri, no Win32, no SQLite, no HTTP.
//!
//! All types here are testable without any infrastructure dependency.

pub mod artwork;
pub mod clock;
pub mod entities;
pub mod errors;
pub mod ids;
pub mod label;
pub mod media_profile;
pub mod optical;
pub mod ports;
pub mod process_binding;
pub mod session;
