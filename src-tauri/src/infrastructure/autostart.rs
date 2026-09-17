use crate::application::settings::AutostartPort;
use crate::domain::errors::DomainError;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "MediaDeck";

#[derive(Debug, Default)]
pub struct WindowsAutostart;

impl WindowsAutostart {
    fn command() -> Result<String, DomainError> {
        let executable = std::env::current_exe().map_err(|_| {
            DomainError::DatabaseOperationFailed("autostart path unavailable".into())
        })?;
        Ok(format!("\"{}\" --background", executable.to_string_lossy()))
    }

    fn open_read() -> Result<RegKey, DomainError> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_READ)
            .map_err(|_| {
                DomainError::DatabaseOperationFailed("autostart registry unavailable".into())
            })
    }

    fn open_write() -> Result<RegKey, DomainError> {
        RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
            .map(|(key, _)| key)
            .map_err(|_| {
                DomainError::DatabaseOperationFailed("autostart registry unavailable".into())
            })
    }
}

impl AutostartPort for WindowsAutostart {
    fn is_enabled(&self) -> Result<bool, DomainError> {
        let Ok(key) = Self::open_read() else {
            return Ok(false);
        };
        Ok(key.get_value::<String, _>(VALUE_NAME).is_ok())
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), DomainError> {
        let key = Self::open_write()?;
        if enabled {
            key.set_value(VALUE_NAME, &Self::command()?)
                .map_err(|_| DomainError::DatabaseOperationFailed("autostart update failed".into()))
        } else {
            match key.delete_value(VALUE_NAME) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(DomainError::DatabaseOperationFailed(
                    "autostart update failed".into(),
                )),
            }
        }
    }
}
