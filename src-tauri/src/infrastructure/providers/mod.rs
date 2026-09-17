//! Game launch providers.

#[cfg(windows)]
pub mod executable;
#[cfg(windows)]
pub mod steam;

#[cfg(windows)]
pub use executable::ExecutableProvider;
#[cfg(windows)]
pub use steam::SteamProvider;
