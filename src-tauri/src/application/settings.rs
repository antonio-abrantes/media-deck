use crate::domain::errors::DomainError;
use crate::domain::ports::SettingsRepository;
use std::sync::Arc;

pub trait AutostartPort: Send + Sync {
    fn is_enabled(&self) -> Result<bool, DomainError>;
    fn set_enabled(&self, enabled: bool) -> Result<(), DomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminSettings {
    pub monitor_active: bool,
    pub autostart: bool,
    pub launcher_monitor: String,
}

pub struct AdminSettingsService {
    settings: Arc<dyn SettingsRepository>,
    autostart: Arc<dyn AutostartPort>,
}

impl AdminSettingsService {
    pub fn new(settings: Arc<dyn SettingsRepository>, autostart: Arc<dyn AutostartPort>) -> Self {
        Self {
            settings,
            autostart,
        }
    }

    pub async fn get(&self) -> Result<AdminSettings, DomainError> {
        let monitor_active = self
            .settings
            .get("monitor_active")
            .await?
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        Ok(AdminSettings {
            monitor_active,
            autostart: self.autostart.is_enabled()?,
            launcher_monitor: self
                .settings
                .get("launcher_monitor")
                .await?
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "auto".into()),
        })
    }

    pub async fn set_monitor_active(&self, enabled: bool) -> Result<AdminSettings, DomainError> {
        self.settings
            .set("monitor_active", serde_json::Value::Bool(enabled))
            .await?;
        self.get().await
    }

    pub async fn set_autostart(&self, enabled: bool) -> Result<AdminSettings, DomainError> {
        let previous = self.autostart.is_enabled()?;
        self.autostart.set_enabled(enabled)?;
        if let Err(error) = self
            .settings
            .set("autostart", serde_json::Value::Bool(enabled))
            .await
        {
            let _ = self.autostart.set_enabled(previous);
            return Err(error);
        }
        self.get().await
    }

    pub async fn set_launcher_monitor(
        &self,
        monitor_id: String,
    ) -> Result<AdminSettings, DomainError> {
        self.settings
            .set("launcher_monitor", serde_json::Value::String(monitor_id))
            .await?;
        self.get().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::fake_repos::FakeSettingsRepository;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeAutostart(Mutex<bool>);

    impl AutostartPort for FakeAutostart {
        fn is_enabled(&self) -> Result<bool, DomainError> {
            Ok(*self.0.lock().unwrap())
        }

        fn set_enabled(&self, enabled: bool) -> Result<(), DomainError> {
            *self.0.lock().unwrap() = enabled;
            Ok(())
        }
    }

    #[tokio::test]
    async fn settings_are_opt_in_and_independently_reversible() {
        let repository = Arc::new(FakeSettingsRepository::new());
        repository
            .set("monitor_active", serde_json::json!(true))
            .unwrap();
        let service = AdminSettingsService::new(repository, Arc::new(FakeAutostart::default()));

        let initial = service.get().await.unwrap();
        assert!(initial.monitor_active);
        assert!(!initial.autostart);

        assert!(service.set_autostart(true).await.unwrap().autostart);
        assert!(
            !service
                .set_monitor_active(false)
                .await
                .unwrap()
                .monitor_active
        );
        assert!(!service.set_autostart(false).await.unwrap().autostart);
    }
}
