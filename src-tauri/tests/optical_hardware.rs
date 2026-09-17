#![cfg(windows)]

use media_deck_lib::application::devices::{
    DeviceEventSink, DeviceMonitor, DeviceMonitorConfig, MediaInsertedEvent, MediaRemovedEvent,
};
use media_deck_lib::domain::entities::{DriveType, MediaKind, MonitorPolicy};
use media_deck_lib::domain::errors::DomainError;
use media_deck_lib::domain::ids::SystemIdGenerator;
use media_deck_lib::domain::media_profile::parse_profile;
use media_deck_lib::domain::ports::DevicePort;
use media_deck_lib::infrastructure::database::open_and_migrate;
use media_deck_lib::infrastructure::database::sessions::SqliteDeviceRepository;
use media_deck_lib::infrastructure::database::settings::SqliteSettingsRepository;
use media_deck_lib::infrastructure::devices::{
    NativeDeviceEvent, NativeEventKind, Win32DeviceAdapter, Win32DeviceEventSource,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::mpsc;

#[derive(Debug)]
enum ObservedEvent {
    Inserted(MediaInsertedEvent),
    Removed(MediaRemovedEvent),
}

struct ChannelSink {
    sender: mpsc::UnboundedSender<ObservedEvent>,
}

impl DeviceEventSink for ChannelSink {
    fn media_inserted(&self, event: &MediaInsertedEvent) -> Result<(), DomainError> {
        self.sender
            .send(ObservedEvent::Inserted(event.clone()))
            .map_err(|_| DomainError::MediaIoFailed)
    }

    fn media_removed(&self, event: &MediaRemovedEvent) -> Result<(), DomainError> {
        self.sender
            .send(ObservedEvent::Removed(event.clone()))
            .map_err(|_| DomainError::MediaIoFailed)
    }
}

fn configured_drive() -> String {
    let value = std::env::var("MEDIADECK_HIL_OPTICAL_DRIVE")
        .expect("set MEDIADECK_HIL_OPTICAL_DRIVE, for example I:");
    let bytes = value.as_bytes();
    assert!(
        bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':',
        "hardware drive must use the X: format"
    );
    format!("{}:", char::from(bytes[0].to_ascii_uppercase()))
}

async fn next_event(receiver: &mut mpsc::UnboundedReceiver<ObservedEvent>) -> ObservedEvent {
    tokio::time::timeout(Duration::from_secs(120), receiver.recv())
        .await
        .expect("timed out waiting for the physical optical event")
        .expect("device monitor stopped before the physical event")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an operator and a physical optical disc"]
async fn optical_disc_is_valid_and_emits_remove_reinsert() {
    let mount_point = configured_drive();
    let adapter: Arc<dyn DevicePort> = Arc::new(Win32DeviceAdapter);
    let mut optical = adapter
        .inspect_drive(&mount_point)
        .await
        .expect("inspect optical drive");
    assert_eq!(optical.drive_type, DriveType::CdRom);
    assert!(optical.interface_path.is_some(), "missing drive identity");
    assert!(
        adapter.is_ready(&mount_point).await.expect("readiness"),
        "insert the prepared data disc before starting this test"
    );

    let profile_path = PathBuf::from(format!("{mount_point}\\GAME.INI"));
    let content = std::fs::read(profile_path).expect("read GAME.INI from optical media");
    let profile = parse_profile(&content, &SystemIdGenerator).expect("validate optical GAME.INI");
    assert_eq!(profile.profile.media_kind, Some(MediaKind::Optical));
    assert_eq!(profile.profile.app_id, Some(851_850));
    assert_eq!(profile.profile.process_hints, ["AT.exe"]);
    assert_eq!(profile.profile.artwork_hint, None);

    let directory = tempdir().expect("temporary database directory");
    let pool = open_and_migrate(&directory.path().join("hardware.db"))
        .await
        .expect("temporary database");
    let repository = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let settings = Arc::new(SqliteSettingsRepository::new(pool));
    optical.enabled = true;
    optical.monitor_policy = MonitorPolicy::ExactDevice;
    repository.upsert(&optical).await.expect("configure drive");

    let (sink_sender, mut sink_receiver) = mpsc::unbounded_channel();
    let sink = Arc::new(ChannelSink {
        sender: sink_sender,
    });
    let (native_sender, native_receiver) = mpsc::unbounded_channel();
    let native_source = Win32DeviceEventSource::start(native_sender.clone())
        .expect("start native device event source");
    let monitor = DeviceMonitor::new(
        adapter,
        repository,
        settings,
        sink,
        DeviceMonitorConfig {
            poll_interval: Duration::from_secs(30),
            ..DeviceMonitorConfig::default()
        },
    );
    let monitor_task = tokio::spawn(monitor.run(native_receiver));

    native_sender
        .send(NativeDeviceEvent {
            kind: NativeEventKind::Arrival,
            mount_point: mount_point.clone(),
        })
        .expect("prime inserted state");
    match next_event(&mut sink_receiver).await {
        ObservedEvent::Inserted(event) => {
            assert_eq!(event.mount_point, mount_point);
            assert_eq!(event.drive_type, DriveType::CdRom);
        }
        ObservedEvent::Removed(_) => panic!("unexpected removal while priming inserted state"),
    }

    eprintln!("HIL_READY_TO_EJECT {mount_point}");
    match next_event(&mut sink_receiver).await {
        ObservedEvent::Removed(event) => {
            assert_eq!(event.mount_point, mount_point);
            eprintln!("HIL_REMOVAL_SOURCE {:?}", event.source);
        }
        ObservedEvent::Inserted(_) => panic!("duplicate insertion before optical removal"),
    }

    eprintln!("HIL_READY_TO_REINSERT {mount_point}");
    match next_event(&mut sink_receiver).await {
        ObservedEvent::Inserted(event) => {
            assert_eq!(event.mount_point, mount_point);
            eprintln!("HIL_REINSERTION_SOURCE {:?}", event.source);
        }
        ObservedEvent::Removed(_) => panic!("duplicate removal before optical reinsertion"),
    }

    monitor_task.abort();
    drop(native_source);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an operator and an empty physical optical drive"]
async fn optical_reinsertion_is_detected_and_profile_remains_valid() {
    let mount_point = configured_drive();
    let adapter: Arc<dyn DevicePort> = Arc::new(Win32DeviceAdapter);
    let mut optical = adapter
        .inspect_drive(&mount_point)
        .await
        .expect("inspect empty optical drive");
    assert_eq!(optical.drive_type, DriveType::CdRom);
    assert!(optical.interface_path.is_some(), "missing drive identity");
    assert!(
        !adapter.is_ready(&mount_point).await.expect("readiness"),
        "eject the disc before starting this reinsertion test"
    );

    let directory = tempdir().expect("temporary database directory");
    let pool = open_and_migrate(&directory.path().join("reinsertion.db"))
        .await
        .expect("temporary database");
    let repository = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let settings = Arc::new(SqliteSettingsRepository::new(pool));
    optical.enabled = true;
    optical.monitor_policy = MonitorPolicy::ExactDevice;
    repository.upsert(&optical).await.expect("configure drive");

    let (sink_sender, mut sink_receiver) = mpsc::unbounded_channel();
    let sink = Arc::new(ChannelSink {
        sender: sink_sender,
    });
    let (native_sender, native_receiver) = mpsc::unbounded_channel();
    let native_source =
        Win32DeviceEventSource::start(native_sender).expect("start native device event source");
    let monitor = DeviceMonitor::new(
        adapter,
        repository,
        settings,
        sink,
        DeviceMonitorConfig::default(),
    );
    let monitor_task = tokio::spawn(monitor.run(native_receiver));

    eprintln!("HIL_READY_TO_REINSERT {mount_point}");
    let insertion = next_event(&mut sink_receiver).await;
    match insertion {
        ObservedEvent::Inserted(event) => {
            assert_eq!(event.mount_point, mount_point);
            assert_eq!(event.drive_type, DriveType::CdRom);
            eprintln!("HIL_REINSERTION_SOURCE {:?}", event.source);
        }
        ObservedEvent::Removed(_) => panic!("unexpected removal while waiting for reinsertion"),
    }

    let profile_path = PathBuf::from(format!("{mount_point}\\GAME.INI"));
    let content = std::fs::read(profile_path).expect("read GAME.INI after reinsertion");
    let profile =
        parse_profile(&content, &SystemIdGenerator).expect("validate reinserted GAME.INI");
    assert_eq!(profile.profile.media_kind, Some(MediaKind::Optical));
    assert_eq!(profile.profile.app_id, Some(851_850));
    assert_eq!(profile.profile.artwork_hint, None);

    monitor_task.abort();
    drop(native_source);
}
