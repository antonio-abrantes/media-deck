use crate::domain::entities::{DeviceId, DriveType, MediaDevice, MonitorPolicy};
use crate::domain::errors::DomainError;
use crate::domain::ports::{DevicePort, DeviceRepository, SettingsRepository};
use crate::infrastructure::devices::{NativeDeviceEvent, NativeEventKind};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(750);
const DEFAULT_READY_RETRY: Duration = Duration::from_millis(100);
const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_DUPLICATE_WINDOW: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy)]
pub struct DeviceMonitorConfig {
    pub poll_interval: Duration,
    pub ready_retry: Duration,
    pub ready_timeout: Duration,
    pub duplicate_window: Duration,
}

impl Default for DeviceMonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL_INTERVAL,
            ready_retry: DEFAULT_READY_RETRY,
            ready_timeout: DEFAULT_READY_TIMEOUT,
            duplicate_window: DEFAULT_DUPLICATE_WINDOW,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceEventSource {
    Native,
    Poll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaInsertedEvent {
    pub device_id: String,
    pub mount_point: String,
    pub drive_type: DriveType,
    pub source: DeviceEventSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaRemovedEvent {
    pub device_id: String,
    pub mount_point: String,
    pub source: DeviceEventSource,
}

pub trait DeviceEventSink: Send + Sync {
    fn media_inserted(&self, event: &MediaInsertedEvent) -> Result<(), DomainError>;
    fn media_removed(&self, event: &MediaRemovedEvent) -> Result<(), DomainError>;
}

#[derive(Clone)]
pub struct DeviceService {
    device_port: Arc<dyn DevicePort>,
    repository: Arc<dyn DeviceRepository>,
}

impl DeviceService {
    pub fn new(device_port: Arc<dyn DevicePort>, repository: Arc<dyn DeviceRepository>) -> Self {
        Self {
            device_port,
            repository,
        }
    }

    /// Discover eligible drives and persist them disabled until the user opts in.
    pub async fn list_eligible(&self) -> Result<Vec<MediaDevice>, DomainError> {
        let observed = self.device_port.list_drives().await?;
        let existing = self.repository.list_all().await?;
        let mut result = Vec::new();

        for mut drive in observed.into_iter().filter(is_supported_drive) {
            if let Some(saved) = existing
                .iter()
                .find(|saved| same_physical_device(saved, &drive))
            {
                drive.id = saved.id;
                drive.enabled = saved.enabled;
                drive.monitor_policy = saved.monitor_policy.clone();
                drive.created_at = saved.created_at;
            }
            drive.updated_at = Utc::now();
            self.repository.upsert(&drive).await?;
            result.push(drive);
        }

        result.sort_by(|left, right| left.friendly_name.cmp(&right.friendly_name));
        Ok(result)
    }

    pub async fn configure(
        &self,
        id: &DeviceId,
        enabled: bool,
        policy: MonitorPolicy,
    ) -> Result<MediaDevice, DomainError> {
        let mut device = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or(DomainError::MediaDeviceNotAllowed)?;
        if !is_supported_drive(&device) {
            return Err(DomainError::MediaDeviceNotAllowed);
        }

        if enabled && policy == MonitorPolicy::AnyOptical {
            let conflicting_policy = self.repository.list_all().await?.into_iter().any(|saved| {
                saved.id != device.id
                    && saved.enabled
                    && saved.monitor_policy == MonitorPolicy::AnyOptical
            });
            if conflicting_policy {
                return Err(DomainError::MediaDeviceNotAllowed);
            }
        }

        device.monitor_policy = if enabled {
            validate_policy(&device, policy)?
        } else {
            MonitorPolicy::Disabled
        };
        device.enabled = enabled;
        device.updated_at = Utc::now();
        self.repository.upsert(&device).await?;
        Ok(device)
    }
}

fn validate_policy(
    device: &MediaDevice,
    policy: MonitorPolicy,
) -> Result<MonitorPolicy, DomainError> {
    match policy {
        MonitorPolicy::ExactDevice
            if device.device_instance_id.is_some() || device.interface_path.is_some() =>
        {
            Ok(policy)
        }
        MonitorPolicy::AnyOptical if device.drive_type == DriveType::CdRom => Ok(policy),
        _ => Err(DomainError::MediaDeviceNotAllowed),
    }
}

fn is_supported_drive(device: &MediaDevice) -> bool {
    matches!(device.drive_type, DriveType::Removable | DriveType::CdRom)
}

fn same_physical_device(left: &MediaDevice, right: &MediaDevice) -> bool {
    match (&left.device_instance_id, &right.device_instance_id) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => match (&left.interface_path, &right.interface_path) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            _ => false,
        },
    }
}

fn matches_configuration(configured: &MediaDevice, observed: &MediaDevice) -> bool {
    if !configured.enabled || !is_supported_drive(observed) {
        return false;
    }
    match configured.monitor_policy {
        MonitorPolicy::Disabled => false,
        MonitorPolicy::AnyOptical => observed.drive_type == DriveType::CdRom,
        MonitorPolicy::ExactDevice => {
            configured.drive_type == observed.drive_type
                && same_physical_device(configured, observed)
        }
    }
}

fn select_configuration<'a>(
    configured: &'a [MediaDevice],
    observed: &MediaDevice,
) -> Option<&'a MediaDevice> {
    let exact: Vec<_> = configured
        .iter()
        .filter(|device| {
            device.monitor_policy == MonitorPolicy::ExactDevice
                && matches_configuration(device, observed)
        })
        .collect();
    if exact.len() == 1 {
        return exact.into_iter().next();
    }
    if !exact.is_empty() {
        return None;
    }

    let optical: Vec<_> = configured
        .iter()
        .filter(|device| {
            device.monitor_policy == MonitorPolicy::AnyOptical
                && matches_configuration(device, observed)
        })
        .collect();
    (optical.len() == 1).then(|| optical[0])
}

#[derive(Debug, Clone)]
struct PresentMedia {
    device_id: DeviceId,
}

pub struct DeviceMonitor {
    device_port: Arc<dyn DevicePort>,
    repository: Arc<dyn DeviceRepository>,
    settings: Arc<dyn SettingsRepository>,
    sink: Arc<dyn DeviceEventSink>,
    config: DeviceMonitorConfig,
    present: HashMap<String, PresentMedia>,
    recent_native: HashMap<(NativeEventKind, String), Instant>,
}

impl DeviceMonitor {
    pub fn new(
        device_port: Arc<dyn DevicePort>,
        repository: Arc<dyn DeviceRepository>,
        settings: Arc<dyn SettingsRepository>,
        sink: Arc<dyn DeviceEventSink>,
        config: DeviceMonitorConfig,
    ) -> Self {
        Self {
            device_port,
            repository,
            settings,
            sink,
            config,
            present: HashMap::new(),
            recent_native: HashMap::new(),
        }
    }

    pub async fn run(mut self, mut events: mpsc::UnboundedReceiver<NativeDeviceEvent>) {
        let mut poll = tokio::time::interval(self.config.poll_interval);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => self.handle_native(event).await,
                    None => break,
                },
                _ = poll.tick() => self.poll_configured().await,
            }
        }
    }

    async fn handle_native(&mut self, event: NativeDeviceEvent) {
        if !self.monitor_enabled().await {
            return;
        }
        let key = (event.kind, event.mount_point.clone());
        let now = Instant::now();
        if self
            .recent_native
            .get(&key)
            .is_some_and(|seen| now.duration_since(*seen) < self.config.duplicate_window)
        {
            debug!(mount_point = %event.mount_point, "duplicate device event ignored");
            return;
        }
        self.recent_native.insert(key, now);
        self.recent_native
            .retain(|_, seen| now.duration_since(*seen) < self.config.duplicate_window);

        match event.kind {
            NativeEventKind::Arrival => {
                self.process_arrival(&event.mount_point, DeviceEventSource::Native)
                    .await;
            }
            NativeEventKind::Removal => {
                self.process_removal(&event.mount_point, DeviceEventSource::Native)
                    .await;
            }
        }
    }

    async fn process_arrival(&mut self, mount_point: &str, source: DeviceEventSource) {
        let observed = match self.device_port.inspect_drive(mount_point).await {
            Ok(device) => device,
            Err(_) => return,
        };
        let configured = match self.repository.find_configured().await {
            Ok(configured) => configured,
            Err(error) => {
                warn!(code = error.code(), "cannot load configured devices");
                return;
            }
        };
        if select_configuration(&configured, &observed).is_none() {
            debug!(
                mount_point,
                "media event ignored for an unauthorized device"
            );
            return;
        }

        let deadline = Instant::now() + self.config.ready_timeout;
        while !matches!(self.device_port.is_ready(mount_point).await, Ok(true)) {
            if Instant::now() >= deadline {
                debug!(mount_point, "configured drive did not become ready");
                return;
            }
            tokio::time::sleep(self.config.ready_retry).await;
        }

        if !self.monitor_enabled().await {
            return;
        }
        let revalidated = match self.device_port.inspect_drive(mount_point).await {
            Ok(device) => device,
            _ => {
                warn!(
                    mount_point,
                    "drive identity changed during readiness debounce"
                );
                return;
            }
        };
        let current_configured = match self.repository.find_configured().await {
            Ok(configured) => configured,
            Err(error) => {
                warn!(code = error.code(), "cannot reload configured devices");
                return;
            }
        };
        let Some(saved) = select_configuration(&current_configured, &revalidated).cloned() else {
            debug!(mount_point, "drive was disabled during readiness debounce");
            return;
        };
        if !matches!(self.device_port.is_ready(mount_point).await, Ok(true)) {
            return;
        }

        if let Some((old_mount, _)) = self
            .present
            .iter()
            .find(|(_, present)| present.device_id == saved.id)
            .map(|(mount, present)| (mount.clone(), present.clone()))
        {
            if old_mount != mount_point {
                self.present.remove(&old_mount);
                self.present.insert(
                    mount_point.to_owned(),
                    PresentMedia {
                        device_id: saved.id,
                    },
                );
                let _ = self
                    .repository
                    .update_mount_point(&saved.id, Some(mount_point))
                    .await;
            }
            return;
        }

        if self.present.contains_key(mount_point) {
            return;
        }
        if let Err(error) = self
            .repository
            .update_mount_point(&saved.id, Some(mount_point))
            .await
        {
            warn!(code = error.code(), "cannot persist reconciled mount point");
            return;
        }

        let event = MediaInsertedEvent {
            device_id: saved.id.to_string(),
            mount_point: mount_point.to_owned(),
            drive_type: revalidated.drive_type.clone(),
            source,
        };
        if let Err(error) = self.sink.media_inserted(&event) {
            warn!(code = error.code(), "cannot deliver media insertion event");
        }
        self.present.insert(
            mount_point.to_owned(),
            PresentMedia {
                device_id: saved.id,
            },
        );
        info!(device_id = %saved.id, mount_point, "configured media inserted");
    }

    async fn process_removal(&mut self, mount_point: &str, source: DeviceEventSource) {
        let Some(present) = self.present.remove(mount_point) else {
            return;
        };
        if let Err(error) = self
            .repository
            .update_mount_point(&present.device_id, None)
            .await
        {
            warn!(code = error.code(), "cannot clear removed mount point");
        }
        let event = MediaRemovedEvent {
            device_id: present.device_id.to_string(),
            mount_point: mount_point.to_owned(),
            source,
        };
        if let Err(error) = self.sink.media_removed(&event) {
            warn!(code = error.code(), "cannot deliver media removal event");
        }
        info!(device_id = %present.device_id, mount_point, "configured media removed");
    }

    async fn poll_configured(&mut self) {
        if !self.monitor_enabled().await {
            return;
        }
        let configured = match self.repository.find_configured().await {
            Ok(configured) if !configured.is_empty() => configured,
            Ok(_) => return,
            Err(error) => {
                warn!(code = error.code(), "fallback polling cannot load devices");
                return;
            }
        };
        let observed = match self.device_port.list_drives().await {
            Ok(drives) => drives,
            Err(error) => {
                warn!(
                    code = error.code(),
                    "fallback polling cannot inspect drives"
                );
                return;
            }
        };

        let mut ready_mounts = Vec::new();
        for drive in observed.into_iter().filter(is_supported_drive) {
            let Some(mount_point) = drive.current_mount_point.as_deref() else {
                continue;
            };
            if select_configuration(&configured, &drive).is_none() {
                continue;
            }
            if matches!(self.device_port.is_ready(mount_point).await, Ok(true)) {
                ready_mounts.push(mount_point.to_owned());
            }
        }

        for mount_point in &ready_mounts {
            if !self.present.contains_key(mount_point) {
                self.process_arrival(mount_point, DeviceEventSource::Poll)
                    .await;
            }
        }

        let removed: Vec<_> = self
            .present
            .keys()
            .filter(|mount| !ready_mounts.contains(mount))
            .cloned()
            .collect();
        for mount_point in removed {
            self.process_removal(&mount_point, DeviceEventSource::Poll)
                .await;
        }
    }

    async fn monitor_enabled(&self) -> bool {
        match self.settings.get("monitor_active").await {
            Ok(Some(serde_json::Value::Bool(enabled))) => enabled,
            Ok(_) => false,
            Err(error) => {
                warn!(code = error.code(), "cannot read device monitor setting");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::database::fake_repos::{
        FakeDeviceRepository, FakeSettingsRepository,
    };
    use crate::infrastructure::fakes::FakeDevicePort;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        inserted: Mutex<Vec<MediaInsertedEvent>>,
        removed: Mutex<Vec<MediaRemovedEvent>>,
    }

    impl DeviceEventSink for RecordingSink {
        fn media_inserted(&self, event: &MediaInsertedEvent) -> Result<(), DomainError> {
            self.inserted.lock().unwrap().push(event.clone());
            Ok(())
        }

        fn media_removed(&self, event: &MediaRemovedEvent) -> Result<(), DomainError> {
            self.removed.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    fn observed(name: &str, drive_type: DriveType, mount: &str, identity: &str) -> MediaDevice {
        let mut device = MediaDevice::new(name, drive_type, Utc::now());
        device.interface_path = Some(identity.to_owned());
        device.current_mount_point = Some(mount.to_owned());
        device
    }

    fn configured(mut device: MediaDevice, policy: MonitorPolicy) -> MediaDevice {
        device.enabled = true;
        device.monitor_policy = policy;
        device
    }

    fn fast_config() -> DeviceMonitorConfig {
        DeviceMonitorConfig {
            poll_interval: Duration::from_secs(60),
            ready_retry: Duration::from_millis(1),
            ready_timeout: Duration::from_millis(10),
            duplicate_window: Duration::from_secs(1),
        }
    }

    fn enabled_settings() -> Arc<FakeSettingsRepository> {
        let settings = Arc::new(FakeSettingsRepository::new());
        settings
            .set("monitor_active", serde_json::json!(true))
            .unwrap();
        settings
    }

    #[test]
    fn default_fallback_poll_is_slow_and_readiness_retry_is_bounded() {
        let config = DeviceMonitorConfig::default();
        assert_eq!(config.poll_interval, Duration::from_millis(750));
        assert!(config.poll_interval > Duration::from_millis(100));
        assert_eq!(config.ready_timeout, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn discovery_persists_supported_drives_disabled() {
        let port = FakeDevicePort::default();
        port.set_drives(vec![
            observed("Floppy", DriveType::Removable, "B:", "floppy-1"),
            observed("System", DriveType::Fixed, "C:", "system"),
        ]);
        let repo = FakeDeviceRepository::new();
        let service = DeviceService::new(Arc::new(port), Arc::new(repo.clone()));

        let result = service.list_eligible().await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].current_mount_point.as_deref(), Some("B:"));
        assert!(!repo.list_all().unwrap()[0].enabled);
    }

    #[tokio::test]
    async fn configuration_rejects_any_optical_for_floppy() {
        let repo = FakeDeviceRepository::new();
        let device = observed("Floppy", DriveType::Removable, "A:", "floppy-1");
        repo.upsert(&device).unwrap();
        let service = DeviceService::new(Arc::new(FakeDevicePort::default()), Arc::new(repo));

        assert!(matches!(
            service
                .configure(&device.id, true, MonitorPolicy::AnyOptical)
                .await,
            Err(DomainError::MediaDeviceNotAllowed)
        ));
    }

    #[tokio::test]
    async fn authorized_g_drive_emits_one_insert_and_one_remove() {
        let port = FakeDevicePort::default();
        let device = configured(
            observed("DVD", DriveType::CdRom, "G:", "cdrom-0"),
            MonitorPolicy::ExactDevice,
        );
        port.set_drives(vec![device.clone()]);
        port.set_ready("G:", true);
        let repo = FakeDeviceRepository::new();
        repo.upsert(&device).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let mut monitor = DeviceMonitor::new(
            Arc::new(port),
            Arc::new(repo),
            enabled_settings(),
            sink.clone(),
            fast_config(),
        );

        monitor
            .handle_native(NativeDeviceEvent {
                kind: NativeEventKind::Arrival,
                mount_point: "G:".to_owned(),
            })
            .await;
        monitor
            .handle_native(NativeDeviceEvent {
                kind: NativeEventKind::Arrival,
                mount_point: "G:".to_owned(),
            })
            .await;
        monitor
            .handle_native(NativeDeviceEvent {
                kind: NativeEventKind::Removal,
                mount_point: "G:".to_owned(),
            })
            .await;

        assert_eq!(sink.inserted.lock().unwrap().len(), 1);
        assert_eq!(sink.removed.lock().unwrap().len(), 1);
        assert_eq!(sink.inserted.lock().unwrap()[0].mount_point, "G:");
    }

    #[tokio::test]
    async fn unauthorized_device_is_ignored() {
        let port = FakeDevicePort::default();
        let device = observed("DVD", DriveType::CdRom, "G:", "cdrom-0");
        port.set_drives(vec![device]);
        port.set_ready("G:", true);
        let sink = Arc::new(RecordingSink::default());
        let mut monitor = DeviceMonitor::new(
            Arc::new(port),
            Arc::new(FakeDeviceRepository::new()),
            enabled_settings(),
            sink.clone(),
            fast_config(),
        );

        monitor
            .process_arrival("G:", DeviceEventSource::Native)
            .await;

        assert!(sink.inserted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn disabled_global_monitor_ignores_native_events() {
        let port = FakeDevicePort::default();
        let device = configured(
            observed("DVD", DriveType::CdRom, "G:", "cdrom-0"),
            MonitorPolicy::ExactDevice,
        );
        port.set_drives(vec![device.clone()]);
        port.set_ready("G:", true);
        let repo = FakeDeviceRepository::new();
        repo.upsert(&device).unwrap();
        let settings = Arc::new(FakeSettingsRepository::new());
        settings
            .set("monitor_active", serde_json::json!(false))
            .unwrap();
        let sink = Arc::new(RecordingSink::default());
        let mut monitor = DeviceMonitor::new(
            Arc::new(port),
            Arc::new(repo),
            settings,
            sink.clone(),
            fast_config(),
        );

        monitor
            .handle_native(NativeDeviceEvent {
                kind: NativeEventKind::Arrival,
                mount_point: "G:".to_owned(),
            })
            .await;

        assert!(sink.inserted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn configured_drive_that_never_becomes_ready_is_not_emitted() {
        let port = FakeDevicePort::default();
        let device = configured(
            observed("Floppy", DriveType::Removable, "B:", "floppy-1"),
            MonitorPolicy::ExactDevice,
        );
        port.set_drives(vec![device.clone()]);
        port.set_ready("B:", false);
        let repo = FakeDeviceRepository::new();
        repo.upsert(&device).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let mut monitor = DeviceMonitor::new(
            Arc::new(port),
            Arc::new(repo),
            enabled_settings(),
            sink.clone(),
            fast_config(),
        );

        monitor
            .process_arrival("B:", DeviceEventSource::Native)
            .await;

        assert!(sink.inserted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn exact_device_reconciles_a_changed_drive_letter_without_new_insert() {
        let port = FakeDevicePort::default();
        let configured_device = configured(
            observed("Floppy", DriveType::Removable, "A:", "floppy-1"),
            MonitorPolicy::ExactDevice,
        );
        port.set_drives(vec![configured_device.clone()]);
        port.set_ready("A:", true);
        let repo = FakeDeviceRepository::new();
        repo.upsert(&configured_device).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let mut monitor = DeviceMonitor::new(
            Arc::new(port.clone()),
            Arc::new(repo.clone()),
            enabled_settings(),
            sink.clone(),
            fast_config(),
        );
        monitor
            .process_arrival("A:", DeviceEventSource::Native)
            .await;

        let moved = observed("Floppy", DriveType::Removable, "B:", "floppy-1");
        port.set_drives(vec![moved]);
        port.set_ready("A:", false);
        port.set_ready("B:", true);
        monitor.poll_configured().await;

        assert_eq!(sink.inserted.lock().unwrap().len(), 1);
        assert!(sink.removed.lock().unwrap().is_empty());
        assert_eq!(
            repo.find_by_id(&configured_device.id)
                .unwrap()
                .unwrap()
                .current_mount_point
                .as_deref(),
            Some("B:")
        );
    }

    #[tokio::test]
    async fn fallback_poll_detects_missing_native_removal() {
        let port = FakeDevicePort::default();
        let device = configured(
            observed("Floppy", DriveType::Removable, "B:", "floppy-1"),
            MonitorPolicy::ExactDevice,
        );
        port.set_drives(vec![device.clone()]);
        port.set_ready("B:", true);
        let repo = FakeDeviceRepository::new();
        repo.upsert(&device).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let mut monitor = DeviceMonitor::new(
            Arc::new(port.clone()),
            Arc::new(repo),
            enabled_settings(),
            sink.clone(),
            fast_config(),
        );
        monitor.poll_configured().await;
        port.set_drives(Vec::new());
        port.set_ready("B:", false);
        monitor.poll_configured().await;

        assert_eq!(sink.inserted.lock().unwrap().len(), 1);
        assert_eq!(sink.removed.lock().unwrap().len(), 1);
        assert_eq!(
            sink.removed.lock().unwrap()[0].source,
            DeviceEventSource::Poll
        );
    }
}
