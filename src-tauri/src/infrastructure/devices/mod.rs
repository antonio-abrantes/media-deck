//! Physical device discovery and native media-change notifications.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    unit_mask_to_mount_points, NativeDeviceEvent, NativeEventKind, Win32DeviceAdapter,
    Win32DeviceEventSource,
};
