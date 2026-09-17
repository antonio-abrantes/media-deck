use crate::domain::entities::{DriveType, MediaDevice};
use crate::domain::errors::DomainError;
use crate::domain::ports::{DevicePort, PortResult};
use chrono::Utc;
use std::ffi::c_void;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use tokio::sync::mpsc as tokio_mpsc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, QueryDosDeviceW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WindowsProgramming::{
    DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    TranslateMessage, CREATESTRUCTW, DBTF_MEDIA, DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE,
    DBT_DEVTYP_VOLUME, DEV_BROADCAST_HDR, DEV_BROADCAST_VOLUME, GWLP_USERDATA, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CREATE, WM_DESTROY, WM_DEVICECHANGE, WNDCLASSW,
};

const STOP_MESSAGE: u32 = WM_APP + 0x4D;
const CLASS_NAME: PCWSTR = w!("MediaDeckDeviceWatcherWindow");
const WINDOW_NAME: PCWSTR = w!("MediaDeck Device Watcher");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeEventKind {
    Arrival,
    Removal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDeviceEvent {
    pub kind: NativeEventKind,
    pub mount_point: String,
}

/// Convert a `DEV_BROADCAST_VOLUME` unit mask into all affected mount points.
/// Bit zero represents A:, bit six represents G:, and so on.
pub fn unit_mask_to_mount_points(mask: u32) -> Vec<String> {
    (0_u8..26)
        .filter(|index| mask & (1_u32 << index) != 0)
        .map(|index| format!("{}:", char::from(b'A' + index)))
        .collect()
}

#[derive(Default)]
pub struct Win32DeviceAdapter;

impl Win32DeviceAdapter {
    fn normalize_mount_point(mount_point: &str) -> Result<String, DomainError> {
        let bytes = mount_point.as_bytes();
        if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
            return Err(DomainError::MediaIoFailed);
        }
        Ok(format!("{}:", char::from(bytes[0].to_ascii_uppercase())))
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn inspect_sync(&self, mount_point: &str) -> Result<MediaDevice, DomainError> {
        let mount_point = Self::normalize_mount_point(mount_point)?;
        let root = Self::wide_null(&format!("{mount_point}\\"));
        let raw_drive_type = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
        let drive_type = match raw_drive_type {
            DRIVE_REMOVABLE => DriveType::Removable,
            DRIVE_CDROM => DriveType::CdRom,
            DRIVE_FIXED => DriveType::Fixed,
            DRIVE_REMOTE => DriveType::Remote,
            DRIVE_RAMDISK => DriveType::RamDisk,
            _ => DriveType::Unknown,
        };

        let query = Self::wide_null(&mount_point);
        let mut target = vec![0_u16; 1024];
        let target_length = unsafe { QueryDosDeviceW(PCWSTR(query.as_ptr()), Some(&mut target)) };
        let interface_path = if target_length == 0 {
            None
        } else {
            let first_nul = target
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(target_length as usize);
            Some(String::from_utf16_lossy(&target[..first_nul]).to_ascii_lowercase())
        };

        let mut volume_name = vec![0_u16; 261];
        let mut serial = 0_u32;
        let volume_ready = unsafe {
            GetVolumeInformationW(
                PCWSTR(root.as_ptr()),
                Some(&mut volume_name),
                Some(&mut serial),
                None,
                None,
                None,
            )
        }
        .is_ok();
        let label = volume_name
            .iter()
            .position(|value| *value == 0)
            .map(|end| String::from_utf16_lossy(&volume_name[..end]))
            .filter(|value| !value.is_empty());
        let kind_name = match drive_type {
            DriveType::CdRom => "CD/DVD",
            DriveType::Removable => "Removable",
            DriveType::Fixed => "Fixed",
            DriveType::Remote => "Network",
            DriveType::RamDisk => "RAM disk",
            DriveType::Unknown => "Unknown",
        };

        let now = Utc::now();
        let mut device = MediaDevice::new(
            label.unwrap_or_else(|| format!("{kind_name} Drive {mount_point}")),
            drive_type,
            now,
        );
        device.interface_path = interface_path;
        device.current_mount_point = Some(mount_point);
        device.last_seen_at = Some(now);
        device.capabilities = serde_json::json!({
            "read": true,
            "write": matches!(device.drive_type, DriveType::Removable),
            "erase": false,
            "ready": volume_ready,
            "identity_source": "query_dos_device",
            "volume_serial": volume_ready.then(|| format!("{serial:08X}")),
        });
        Ok(device)
    }
}

impl DevicePort for Win32DeviceAdapter {
    fn list_drives(&self) -> PortResult<'_, Vec<MediaDevice>> {
        Box::pin(async move {
            let mask = unsafe { GetLogicalDrives() };
            if mask == 0 {
                return Err(DomainError::MediaIoFailed);
            }
            let mut drives = Vec::new();
            for mount_point in unit_mask_to_mount_points(mask) {
                drives.push(self.inspect_sync(&mount_point)?);
            }
            Ok(drives)
        })
    }

    fn inspect_drive<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, MediaDevice> {
        Box::pin(async move { self.inspect_sync(mount_point) })
    }

    fn is_ready<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, bool> {
        Box::pin(async move {
            let mount_point = Self::normalize_mount_point(mount_point)?;
            let root = Self::wide_null(&format!("{mount_point}\\"));
            Ok(unsafe {
                GetVolumeInformationW(PCWSTR(root.as_ptr()), None, None, None, None, None)
            }
            .is_ok())
        })
    }
}

struct WindowContext {
    sender: tokio_mpsc::UnboundedSender<NativeDeviceEvent>,
}

/// Dedicated hidden top-level window used to receive `WM_DEVICECHANGE`.
///
/// A top-level window is intentional: optical media notifications are not
/// reliably delivered to message-only windows on the interactive session.
pub struct Win32DeviceEventSource {
    window: isize,
    thread: Option<JoinHandle<()>>,
}

impl Win32DeviceEventSource {
    pub fn start(
        sender: tokio_mpsc::UnboundedSender<NativeDeviceEvent>,
    ) -> Result<Self, DomainError> {
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("mediadeck-device-events".to_owned())
            .spawn(move || run_window_loop(sender, startup_tx))
            .map_err(|_| DomainError::MediaIoFailed)?;
        let window = startup_rx
            .recv()
            .map_err(|_| DomainError::MediaIoFailed)??;
        Ok(Self {
            window,
            thread: Some(thread),
        })
    }
}

impl Drop for Win32DeviceEventSource {
    fn drop(&mut self) {
        if self.window != 0 {
            let hwnd = HWND(self.window as *mut c_void);
            let _ = unsafe { PostMessageW(Some(hwnd), STOP_MESSAGE, WPARAM(0), LPARAM(0)) };
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_window_loop(
    sender: tokio_mpsc::UnboundedSender<NativeDeviceEvent>,
    startup_tx: mpsc::SyncSender<Result<isize, DomainError>>,
) {
    let module = match unsafe { GetModuleHandleW(None) } {
        Ok(module) => module,
        Err(_) => {
            let _ = startup_tx.send(Err(DomainError::MediaIoFailed));
            return;
        }
    };
    let instance = HINSTANCE(module.0);
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };

    if unsafe { RegisterClassW(&class) } == 0 {
        let _ = startup_tx.send(Err(DomainError::MediaIoFailed));
        return;
    }

    let context = Box::new(WindowContext { sender });
    let context_ptr = (&*context as *const WindowContext).cast::<c_void>();
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            WINDOW_NAME,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            Some(context_ptr),
        )
    };
    let window = match window {
        Ok(window) => window,
        Err(_) => {
            let _ = startup_tx.send(Err(DomainError::MediaIoFailed));
            return;
        }
    };
    if startup_tx.send(Ok(window.0 as isize)).is_err() {
        let _ = unsafe { DestroyWindow(window) };
        return;
    }

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        let _ = unsafe { TranslateMessage(&message) };
        unsafe { DispatchMessageW(&message) };
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
            LRESULT(0)
        }
        WM_DEVICECHANGE => {
            handle_device_change(window, wparam, lparam);
            LRESULT(0)
        }
        STOP_MESSAGE => {
            let _ = unsafe { DestroyWindow(window) };
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn handle_device_change(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    let kind = match wparam.0 as u32 {
        DBT_DEVICEARRIVAL => NativeEventKind::Arrival,
        DBT_DEVICEREMOVECOMPLETE => NativeEventKind::Removal,
        _ => return,
    };
    if lparam.0 == 0 {
        return;
    }
    let header = unsafe { &*(lparam.0 as *const DEV_BROADCAST_HDR) };
    if header.dbch_devicetype != DBT_DEVTYP_VOLUME {
        return;
    }
    let volume = unsafe { &*(lparam.0 as *const DEV_BROADCAST_VOLUME) };
    if volume.dbcv_flags.0 & DBTF_MEDIA.0 == 0 {
        return;
    }
    let context = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const WindowContext;
    if context.is_null() {
        return;
    }
    for mount_point in unit_mask_to_mount_points(volume.dbcv_unitmask) {
        let _ = unsafe { &*context }
            .sender
            .send(NativeDeviceEvent { kind, mount_point });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_mask_maps_a_and_g_without_assuming_floppy_letter() {
        assert_eq!(unit_mask_to_mount_points(1), vec!["A:"]);
        assert_eq!(unit_mask_to_mount_points(1 << 6), vec!["G:"]);
        assert_eq!(
            unit_mask_to_mount_points((1 << 1) | (1 << 25)),
            vec!["B:", "Z:"]
        );
    }

    #[test]
    fn mount_point_validation_accepts_any_drive_letter() {
        assert_eq!(
            Win32DeviceAdapter::normalize_mount_point("g:"),
            Ok("G:".to_owned())
        );
        assert_eq!(
            Win32DeviceAdapter::normalize_mount_point("G:\\"),
            Err(DomainError::MediaIoFailed)
        );
    }

    #[test]
    fn hidden_window_message_loop_starts_and_stops() {
        let (sender, _receiver) = tokio_mpsc::unbounded_channel();
        let source = Win32DeviceEventSource::start(sender).expect("hidden window");
        drop(source);
    }

    #[tokio::test]
    async fn adapter_enumerates_host_logical_drives() {
        let drives = Win32DeviceAdapter
            .list_drives()
            .await
            .expect("logical drives");
        assert!(!drives.is_empty());
        assert!(drives
            .iter()
            .all(|drive| drive.current_mount_point.is_some()));
    }
}
