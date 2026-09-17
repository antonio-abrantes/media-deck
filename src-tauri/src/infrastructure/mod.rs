//! Infrastructure adapters — concrete implementations of domain ports.

pub mod artwork;
#[cfg(windows)]
pub mod autostart;
pub mod database;
pub mod devices;
#[cfg(test)]
pub mod fakes;
pub mod media;
pub mod processes;
pub mod providers;
pub mod windows;
