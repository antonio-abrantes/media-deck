//! Process supervision adapters.

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{validate_launch_target, Win32ProcessAdapter};
