//! Windows process enumeration, association helpers and safe close.

use crate::domain::errors::DomainError;
use crate::domain::ports::{CloseResult, PortResult, ProcessPort, ProcessSnapshot};
use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, MAX_PATH, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GW_OWNER,
    WM_CLOSE,
};

#[derive(Default)]
pub struct Win32ProcessAdapter;

impl Win32ProcessAdapter {
    fn enumerate_sync() -> Result<Vec<ProcessSnapshot>, DomainError> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(map_windows_error)?;
        let _guard = HandleGuard(snapshot);

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut processes = Vec::new();
        let mut ok = unsafe { Process32FirstW(_guard.0, &mut entry) }.is_ok();
        while ok {
            if entry.th32ProcessID != 0 {
                if let Ok(Some(snapshot)) = Self::inspect_pid(entry.th32ProcessID) {
                    processes.push(snapshot);
                }
            }
            ok = unsafe { Process32NextW(_guard.0, &mut entry) }.is_ok();
        }
        Ok(processes)
    }

    fn inspect_pid(pid: u32) -> Result<Option<ProcessSnapshot>, DomainError> {
        let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            Ok(handle) => handle,
            Err(_) => return Ok(None),
        };
        let _guard = HandleGuard(handle);

        let mut creation = Default::default();
        let mut exit = Default::default();
        let mut kernel = Default::default();
        let mut user = Default::default();
        unsafe { GetProcessTimes(_guard.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            .map_err(map_windows_error)?;

        let mut buffer = vec![0u16; MAX_PATH as usize];
        let mut size = buffer.len() as u32;
        let path_result = unsafe {
            QueryFullProcessImageNameW(
                _guard.0,
                Default::default(),
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };
        let executable_path = if path_result.is_ok() && size > 0 {
            OsString::from_wide(&buffer[..size as usize])
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };

        if executable_path.is_empty() {
            return Ok(None);
        }

        Ok(Some(ProcessSnapshot {
            pid,
            creation_time: filetime_to_u64(creation),
            executable_path,
        }))
    }

    fn revalidate_sync(snapshot: &ProcessSnapshot) -> Result<bool, DomainError> {
        match Self::inspect_pid(snapshot.pid)? {
            Some(current) => Ok(current.pid == snapshot.pid
                && current.creation_time == snapshot.creation_time
                && current
                    .executable_path
                    .eq_ignore_ascii_case(&snapshot.executable_path)),
            None => Ok(false),
        }
    }

    fn find_main_window_sync(snapshot: &ProcessSnapshot) -> Result<Option<u64>, DomainError> {
        if !Self::revalidate_sync(snapshot)? {
            return Ok(None);
        }
        let mut found: Option<HWND> = None;
        let mut state = EnumState {
            pid: snapshot.pid,
            found: &mut found,
        };
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_callback),
                LPARAM(&mut state as *mut EnumState as isize),
            );
        }
        Ok(found.map(|hwnd| hwnd.0 as u64))
    }

    fn list_children_sync(snapshot: &ProcessSnapshot) -> Result<Vec<ProcessSnapshot>, DomainError> {
        let all = Self::enumerate_sync()?;
        let parents = parent_map()?;
        Ok(all
            .into_iter()
            .filter(|candidate| parents.get(&candidate.pid) == Some(&snapshot.pid))
            .collect())
    }

    fn request_close_sync(
        snapshot: &ProcessSnapshot,
        timeout_secs: u32,
    ) -> Result<CloseResult, DomainError> {
        if !Self::revalidate_sync(snapshot)? {
            return Ok(CloseResult::AlreadyGone);
        }
        if let Some(hwnd) = Self::find_main_window_sync(snapshot)? {
            let window = HWND(hwnd as *mut std::ffi::c_void);
            unsafe { PostMessageW(Some(window), WM_CLOSE, Default::default(), LPARAM(0)) }
                .map_err(map_windows_error)?;
        } else {
            // No window: wait only; do not force here.
        }

        let deadline = Instant::now() + Duration::from_secs(u64::from(timeout_secs.max(1)));
        while Instant::now() < deadline {
            if !Self::revalidate_sync(snapshot)? {
                return Ok(CloseResult::Exited);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if Self::revalidate_sync(snapshot)? {
            Ok(CloseResult::TimedOut)
        } else {
            Ok(CloseResult::Exited)
        }
    }

    fn force_kill_sync(snapshot: &ProcessSnapshot) -> Result<(), DomainError> {
        if !Self::revalidate_sync(snapshot)? {
            return Err(DomainError::ProcessNotBound);
        }
        let handle = unsafe {
            OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                snapshot.pid,
            )
        }
        .map_err(|_| DomainError::ProcessNotBound)?;
        let _guard = HandleGuard(handle);
        if !Self::revalidate_sync(snapshot)? {
            return Err(DomainError::ProcessNotBound);
        }
        unsafe { TerminateProcess(_guard.0, 1) }.map_err(map_windows_error)?;
        let wait = unsafe { WaitForSingleObject(_guard.0, 5_000) };
        if wait == WAIT_TIMEOUT {
            tracing::warn!(pid = snapshot.pid, "TerminateProcess wait timed out");
            return Err(DomainError::LaunchFailed);
        }
        std::thread::sleep(Duration::from_millis(50));
        if Self::revalidate_sync(snapshot)? {
            return Err(DomainError::LaunchFailed);
        }
        Ok(())
    }
}

impl ProcessPort for Win32ProcessAdapter {
    fn snapshot_baseline(&self) -> PortResult<'_, Vec<ProcessSnapshot>> {
        Box::pin(async move { Self::enumerate_sync() })
    }

    fn list_processes(&self) -> PortResult<'_, Vec<ProcessSnapshot>> {
        Box::pin(async move { Self::enumerate_sync() })
    }

    fn revalidate<'a>(&'a self, snapshot: &'a ProcessSnapshot) -> PortResult<'a, bool> {
        Box::pin(async move { Self::revalidate_sync(snapshot) })
    }

    fn find_main_window<'a>(
        &'a self,
        snapshot: &'a ProcessSnapshot,
    ) -> PortResult<'a, Option<u64>> {
        Box::pin(async move { Self::find_main_window_sync(snapshot) })
    }

    fn list_children<'a>(
        &'a self,
        snapshot: &'a ProcessSnapshot,
    ) -> PortResult<'a, Vec<ProcessSnapshot>> {
        Box::pin(async move { Self::list_children_sync(snapshot) })
    }

    fn request_close<'a>(
        &'a self,
        snapshot: &'a ProcessSnapshot,
        timeout_secs: u32,
    ) -> PortResult<'a, CloseResult> {
        Box::pin(async move { Self::request_close_sync(snapshot, timeout_secs) })
    }

    fn force_kill<'a>(&'a self, snapshot: &'a ProcessSnapshot) -> PortResult<'a, ()> {
        Box::pin(async move { Self::force_kill_sync(snapshot) })
    }
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct EnumState<'a> {
    pid: u32,
    found: &'a mut Option<HWND>,
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut EnumState);
    let mut window_pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut window_pid)) };
    if window_pid != state.pid {
        return BOOL(1);
    }
    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
    let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or(HWND::default());
    if visible && owner.0.is_null() {
        *state.found = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

fn parent_map() -> Result<HashMap<u32, u32>, DomainError> {
    let snapshot =
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(map_windows_error)?;
    let _guard = HandleGuard(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut map = HashMap::new();
    let mut ok = unsafe { Process32FirstW(_guard.0, &mut entry) }.is_ok();
    while ok {
        map.insert(entry.th32ProcessID, entry.th32ParentProcessID);
        ok = unsafe { Process32NextW(_guard.0, &mut entry) }.is_ok();
    }
    Ok(map)
}

fn filetime_to_u64(value: windows::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

fn map_windows_error(error: windows::core::Error) -> DomainError {
    tracing::error!(error = %error, "windows process API failed");
    DomainError::LaunchFailed
}

/// Canonicalise a local executable path and reject unsafe volume classes.
pub fn validate_launch_target(path: &str) -> Result<PathBuf, DomainError> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::Win32::System::WindowsProgramming::{
        DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
    };

    if path.starts_with(r"\\") || path.chars().nth(1).is_none_or(|c| c != ':') {
        return Err(DomainError::LaunchProfileInvalid);
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| DomainError::LaunchProfileInvalid)?;
    let canonical = strip_verbatim_prefix(canonical);
    if !canonical.is_file()
        || canonical
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("exe"))
    {
        return Err(DomainError::LaunchProfileInvalid);
    }
    let drive = canonical
        .components()
        .next()
        .and_then(|component| match component {
            std::path::Component::Prefix(prefix) => {
                let raw = prefix.as_os_str().to_string_lossy();
                raw.chars().next().map(|letter| format!("{letter}:\\"))
            }
            _ => None,
        })
        .ok_or(DomainError::LaunchProfileInvalid)?;
    let wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
    match drive_type {
        DRIVE_FIXED => Ok(canonical),
        DRIVE_REMOVABLE | DRIVE_CDROM | DRIVE_REMOTE | DRIVE_RAMDISK => {
            Err(DomainError::LaunchProfileInvalid)
        }
        _ => Err(DomainError::LaunchProfileInvalid),
    }
}

fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_includes_current_process() {
        let adapter = Win32ProcessAdapter;
        let snaps = adapter.snapshot_baseline().await.expect("snapshot");
        let self_pid = std::process::id();
        assert!(
            snaps.iter().any(|snap| snap.pid == self_pid),
            "current test process must appear in the baseline"
        );
    }

    #[tokio::test]
    async fn revalidate_accepts_live_self_and_rejects_fake() {
        let adapter = Win32ProcessAdapter;
        let self_pid = std::process::id();
        let live = adapter
            .list_processes()
            .await
            .unwrap()
            .into_iter()
            .find(|snap| snap.pid == self_pid)
            .expect("self");
        assert!(adapter.revalidate(&live).await.unwrap());

        let fake = ProcessSnapshot {
            pid: live.pid,
            creation_time: live.creation_time.wrapping_add(1),
            executable_path: live.executable_path.clone(),
        };
        assert!(!adapter.revalidate(&fake).await.unwrap());
    }

    #[test]
    fn validate_launch_target_rejects_unc_and_non_exe() {
        assert!(validate_launch_target(r"\\server\share\game.exe").is_err());
        assert!(validate_launch_target(r"C:\Windows\System32\drivers\etc\hosts").is_err());
    }
}
