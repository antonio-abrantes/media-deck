//! Windows integration remains isolated in this module.

#[cfg(windows)]
use crate::domain::errors::DomainError;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutReview {
    pub target_path: String,
    pub working_directory: Option<String>,
    pub arguments: String,
}

/// Reads a Shell Link through the native COM parser. The link is never
/// executed and `Resolve` is deliberately not called (it may show UI/search).
#[cfg(windows)]
pub fn inspect_shortcut(raw_path: &str) -> Result<ShortcutReview, DomainError> {
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};

    let link_path =
        std::fs::canonicalize(raw_path).map_err(|_| DomainError::LaunchProfileInvalid)?;
    if !link_path.is_file()
        || link_path.to_string_lossy().starts_with(r"\\")
        || !link_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("lnk"))
    {
        return Err(DomainError::LaunchProfileInvalid);
    }

    let wide = link_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _com_guard = ComGuard(initialized.is_ok());
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| DomainError::LaunchProfileInvalid)?;
        let persist: IPersistFile = link.cast().map_err(|_| DomainError::LaunchProfileInvalid)?;
        persist
            .Load(PCWSTR(wide.as_ptr()), STGM_READ)
            .map_err(|_| DomainError::LaunchProfileInvalid)?;

        let mut target = vec![0u16; 32_768];
        let mut data = WIN32_FIND_DATAW::default();
        link.GetPath(&mut target, &mut data, SLGP_RAWPATH.0 as u32)
            .map_err(|_| DomainError::LaunchProfileInvalid)?;
        let target_path = utf16_buffer(&target)?;
        let canonical_target =
            crate::infrastructure::processes::validate_launch_target(&target_path)?;

        let mut working = vec![0u16; 32_768];
        link.GetWorkingDirectory(&mut working)
            .map_err(|_| DomainError::LaunchProfileInvalid)?;
        let working_directory = utf16_buffer_optional(&working)?;
        let mut arguments = vec![0u16; 32_768];
        link.GetArguments(&mut arguments)
            .map_err(|_| DomainError::LaunchProfileInvalid)?;

        Ok(ShortcutReview {
            target_path: canonical_target.to_string_lossy().into_owned(),
            working_directory,
            arguments: utf16_buffer_optional(&arguments)?.unwrap_or_default(),
        })
    }
}

#[cfg(windows)]
struct ComGuard(bool);

#[cfg(windows)]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

#[cfg(windows)]
fn utf16_buffer(buffer: &[u16]) -> Result<String, DomainError> {
    utf16_buffer_optional(buffer)?.ok_or(DomainError::LaunchProfileInvalid)
}

#[cfg(windows)]
fn utf16_buffer_optional(buffer: &[u16]) -> Result<Option<String>, DomainError> {
    let end = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    if end == 0 {
        return Ok(None);
    }
    String::from_utf16(&buffer[..end])
        .map(Some)
        .map_err(|_| DomainError::LaunchProfileInvalid)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn shortcut_review_rejects_missing_or_non_link_files() {
        assert_eq!(
            inspect_shortcut(r"C:\definitely-missing\game.lnk"),
            Err(DomainError::LaunchProfileInvalid)
        );
        assert_eq!(
            inspect_shortcut(std::env::current_exe().unwrap().to_string_lossy().as_ref()),
            Err(DomainError::LaunchProfileInvalid)
        );
    }
}
