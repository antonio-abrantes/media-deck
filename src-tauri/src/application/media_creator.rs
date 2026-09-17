//! Media Creator orchestration. IPC requests contain persisted IDs and portable
//! metadata only; mount points and filesystem names remain inside the core.

use crate::application::artwork::ArtworkService;
use crate::domain::artwork::ArtworkKind;
use crate::domain::clock::Clock;
use crate::domain::entities::{
    ContentHash, DeviceId, DriveType, ExportKind, GameActivation, GameId, MediaDescriptor, MediaId,
    MediaKey, MediaKind, MediaStatus, ProfileId,
};
use crate::domain::errors::DomainError;
use crate::domain::ids::SystemIdGenerator;
use crate::domain::media_profile::{
    build_v2_profile, parse_profile, write_canonical_v2, MediaProfile, ProfileSourceVersion,
    ProfileWarning,
};
use crate::domain::ports::{
    ActivationRepository, DevicePort, DeviceRepository, GameRepository, MediaRepository,
    ProfileRepository,
};
use crate::infrastructure::media::floppy::FloppyMediaWriter;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct CreatorSelection {
    pub game_id: GameId,
    pub profile_id: ProfileId,
    pub device_id: Option<DeviceId>,
    pub media_key: MediaKey,
    pub media_kind: Option<MediaKind>,
    pub include_artwork: bool,
}

#[derive(Debug, Clone)]
pub struct CreatorPreview {
    pub profile: MediaProfile,
    pub canonical_ini: String,
    pub device_name: Option<String>,
    pub mount_point: Option<String>,
    pub include_artwork: bool,
}

#[derive(Debug, Clone)]
pub struct IniExportReceipt {
    pub path: PathBuf,
    pub content_hash: ContentHash,
    pub activation: GameActivation,
}

#[derive(Debug, Clone)]
pub struct LegacyImport {
    pub source_version: ProfileSourceVersion,
    pub profile: MediaProfile,
    pub warnings: Vec<ProfileWarning>,
    pub source_hash: String,
}

pub struct MediaCreatorService {
    games: Arc<dyn GameRepository>,
    profiles: Arc<dyn ProfileRepository>,
    devices: Arc<dyn DeviceRepository>,
    device_port: Arc<dyn DevicePort>,
    media: Arc<dyn MediaRepository>,
    artwork: Option<Arc<ArtworkService>>,
    activations: Arc<dyn ActivationRepository>,
    clock: Arc<dyn Clock>,
}

pub struct MediaCreatorRepositories {
    pub games: Arc<dyn GameRepository>,
    pub profiles: Arc<dyn ProfileRepository>,
    pub devices: Arc<dyn DeviceRepository>,
    pub media: Arc<dyn MediaRepository>,
    pub activations: Arc<dyn ActivationRepository>,
}

impl MediaCreatorService {
    pub fn new(
        repositories: MediaCreatorRepositories,
        device_port: Arc<dyn DevicePort>,
        artwork: Option<Arc<ArtworkService>>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            games: repositories.games,
            profiles: repositories.profiles,
            devices: repositories.devices,
            device_port,
            media: repositories.media,
            artwork,
            activations: repositories.activations,
            clock,
        }
    }

    pub async fn preview(
        &self,
        selection: &CreatorSelection,
    ) -> Result<CreatorPreview, DomainError> {
        let game = self
            .games
            .find_by_id(&selection.game_id)
            .await?
            .ok_or(DomainError::GameNotInstalled)?;
        let profile = self
            .profiles
            .find_by_id(&selection.profile_id)
            .await?
            .ok_or(DomainError::LaunchProfileInvalid)?;
        let device = match selection.device_id {
            Some(_) => Some(self.valid_device(selection).await?),
            None => None,
        };
        let mut portable = build_v2_profile(
            &game,
            &profile,
            selection.media_key.clone(),
            selection.media_kind.clone(),
            self.clock.now_utc(),
        )?;
        if let Some(artwork) = &self.artwork {
            if let Some(cover) = artwork
                .resolve_active(&selection.game_id, ArtworkKind::LauncherCover)
                .await?
            {
                portable.cover_artwork_id = Some(cover.artwork.id);
                portable.cover_cache_path = Some(cover.artwork.relative_path);
            }
        }
        let canonical_ini =
            write_canonical_v2(&portable).map_err(|_| DomainError::MediaProfileInvalid)?;
        Ok(CreatorPreview {
            profile: portable,
            canonical_ini,
            device_name: device.as_ref().map(|value| value.friendly_name.clone()),
            mount_point: device.and_then(|value| value.current_mount_point),
            include_artwork: selection.include_artwork,
        })
    }

    pub async fn export_ini(
        &self,
        selection: &CreatorSelection,
        destination: &Path,
    ) -> Result<IniExportReceipt, DomainError> {
        validate_ini_destination(destination)?;
        let preview = self.preview(selection).await?;
        let expected_hash = digest(preview.canonical_ini.as_bytes());
        let mut file = tokio::fs::File::create(destination)
            .await
            .map_err(|_| DomainError::MediaIoFailed)?;
        file.write_all(preview.canonical_ini.as_bytes())
            .await
            .map_err(|_| DomainError::MediaIoFailed)?;
        file.sync_all()
            .await
            .map_err(|_| DomainError::MediaIoFailed)?;

        let bytes = tokio::fs::read(destination)
            .await
            .map_err(|_| DomainError::MediaVerificationFailed)?;
        let parsed = parse_profile(&bytes, &SystemIdGenerator)
            .map_err(|_| DomainError::MediaVerificationFailed)?;
        let canonical = write_canonical_v2(&parsed.profile)
            .map_err(|_| DomainError::MediaVerificationFailed)?;
        let actual_hash = digest(canonical.as_bytes());
        if parsed.source_version != ProfileSourceVersion::V2
            || parsed.profile.media_id != selection.media_key
            || parsed.profile.profile_id != selection.profile_id
            || actual_hash != expected_hash
        {
            return Err(DomainError::MediaVerificationFailed);
        }

        let now = self.clock.now_utc();
        let activation = GameActivation {
            game_id: selection.game_id,
            profile_id: selection.profile_id,
            last_export_kind: ExportKind::IniFile,
            last_media_key: selection.media_key.clone(),
            last_content_hash: ContentHash::new(actual_hash)?,
            schema_version: 2,
            export_count: 1,
            first_activated_at: now,
            last_exported_at: now,
        };
        self.activations
            .upsert(&activation)
            .await
            .map_err(|_| DomainError::ActivationPersistFailed)?;
        let activation = self
            .activations
            .find_by_game_id(&selection.game_id)
            .await
            .map_err(|_| DomainError::ActivationPersistFailed)?
            .ok_or(DomainError::ActivationPersistFailed)?;
        Ok(IniExportReceipt {
            path: destination.to_owned(),
            content_hash: activation.last_content_hash.clone(),
            activation,
        })
    }

    pub async fn write(
        &self,
        selection: &CreatorSelection,
        confirmed: bool,
    ) -> Result<MediaDescriptor, DomainError> {
        if !confirmed {
            return Err(DomainError::InvalidTransition {
                state: "preview".into(),
                event: "write_without_confirmation".into(),
            });
        }
        if selection.media_kind != Some(MediaKind::Floppy) {
            return Err(DomainError::OpticalMediaNotWritable);
        }
        let preview = self.preview(selection).await?;
        let configured = self.valid_device(selection).await?;
        let receipt = FloppyMediaWriter::new(self.device_port.clone())
            .write_verified(&configured, &preview.profile)
            .await?;
        let now = self.clock.now_utc();
        let previous = self.media.find_by_key(selection.media_key.as_str()).await?;
        let descriptor = MediaDescriptor {
            id: previous
                .as_ref()
                .map(|item| item.id)
                .unwrap_or_else(MediaId::new),
            media_key: selection.media_key.clone(),
            profile_id: selection.profile_id,
            media_kind: selection
                .media_kind
                .clone()
                .ok_or(DomainError::MediaProfileInvalid)?,
            last_device_id: selection.device_id,
            schema_version: 2,
            content_hash: receipt.content_hash,
            volume_serial: None,
            last_drive: preview.mount_point,
            created_at: previous.map(|item| item.created_at).unwrap_or(now),
            last_seen_at: Some(now),
            last_verified_at: Some(now),
            status: MediaStatus::Active,
        };
        self.media.upsert(&descriptor).await?;
        Ok(descriptor)
    }

    pub async fn inspect_import(&self, device_id: DeviceId) -> Result<LegacyImport, DomainError> {
        let device = self.configured_device(&device_id).await?;
        let bytes = FloppyMediaWriter::new(self.device_port.clone())
            .read_revalidated(&device)
            .await?;
        let parsed = parse_profile(&bytes, &SystemIdGenerator)
            .map_err(|_| DomainError::MediaProfileInvalid)?;
        Ok(LegacyImport {
            source_version: parsed.source_version,
            profile: parsed.profile,
            warnings: parsed.warnings,
            source_hash: digest(&bytes),
        })
    }

    pub async fn upgrade_import(
        &self,
        selection: &CreatorSelection,
        expected_source_hash: &str,
        confirmed: bool,
    ) -> Result<MediaDescriptor, DomainError> {
        if !confirmed || expected_source_hash.len() != 64 {
            return Err(DomainError::MediaProfileInvalid);
        }
        let imported = self
            .inspect_import(
                selection
                    .device_id
                    .ok_or(DomainError::MediaDeviceNotAllowed)?,
            )
            .await?;
        if imported.source_version != ProfileSourceVersion::V1
            || imported.source_hash != expected_source_hash
        {
            return Err(DomainError::MediaChanged);
        }
        self.write(selection, true).await
    }

    pub async fn list(&self) -> Result<Vec<MediaDescriptor>, DomainError> {
        self.media.list().await
    }

    /// Persist an optical result only after the optical coordinator has
    /// completed physical readback and canonical hash verification.
    pub async fn record_verified_optical(
        &self,
        selection: &CreatorSelection,
        canonical_hash: String,
        mount_point: String,
    ) -> Result<MediaDescriptor, DomainError> {
        if selection.media_kind != Some(MediaKind::Optical) || canonical_hash.len() != 64 {
            return Err(DomainError::OpticalVerifyFailed);
        }
        self.valid_device(selection).await?;
        let now = self.clock.now_utc();
        let previous = self.media.find_by_key(selection.media_key.as_str()).await?;
        let descriptor = MediaDescriptor {
            id: previous
                .as_ref()
                .map(|item| item.id)
                .unwrap_or_else(MediaId::new),
            media_key: selection.media_key.clone(),
            profile_id: selection.profile_id,
            media_kind: MediaKind::Optical,
            last_device_id: selection.device_id,
            schema_version: 2,
            content_hash: crate::domain::entities::ContentHash::new(canonical_hash)?,
            volume_serial: None,
            last_drive: Some(mount_point),
            created_at: previous.map(|item| item.created_at).unwrap_or(now),
            last_seen_at: Some(now),
            last_verified_at: Some(now),
            status: MediaStatus::Active,
        };
        self.media.upsert(&descriptor).await?;
        Ok(descriptor)
    }

    pub async fn verify(
        &self,
        media_key: &MediaKey,
        device_id: DeviceId,
    ) -> Result<MediaDescriptor, DomainError> {
        let mut descriptor = self
            .media
            .find_by_key(media_key.as_str())
            .await?
            .ok_or(DomainError::MediaProfileMissing)?;
        let device = self.configured_device(&device_id).await?;
        let bytes = FloppyMediaWriter::new(self.device_port.clone())
            .read_revalidated(&device)
            .await?;
        let parsed = parse_profile(&bytes, &SystemIdGenerator)
            .map_err(|_| DomainError::MediaVerificationFailed)?;
        let canonical = write_canonical_v2(&parsed.profile)
            .map_err(|_| DomainError::MediaVerificationFailed)?;
        if parsed.source_version != ProfileSourceVersion::V2
            || parsed.profile.media_id != descriptor.media_key
            || parsed.profile.profile_id != descriptor.profile_id
            || digest(canonical.as_bytes()) != descriptor.content_hash.as_str()
        {
            return Err(DomainError::MediaVerificationFailed);
        }
        let now = self.clock.now_utc();
        descriptor.last_device_id = Some(device_id);
        descriptor.last_drive = device.current_mount_point;
        descriptor.last_seen_at = Some(now);
        descriptor.last_verified_at = Some(now);
        descriptor.status = MediaStatus::Active;
        self.media.upsert(&descriptor).await?;
        Ok(descriptor)
    }

    async fn valid_device(
        &self,
        selection: &CreatorSelection,
    ) -> Result<crate::domain::entities::MediaDevice, DomainError> {
        let device_id = selection
            .device_id
            .as_ref()
            .ok_or(DomainError::MediaDeviceNotAllowed)?;
        let device = self.configured_device(device_id).await?;
        let compatible = matches!(
            (selection.media_kind.as_ref(), &device.drive_type),
            (Some(MediaKind::Floppy), DriveType::Removable)
                | (Some(MediaKind::Removable), DriveType::Removable)
                | (Some(MediaKind::Optical), DriveType::CdRom)
        );
        if !compatible {
            return Err(DomainError::MediaDeviceNotAllowed);
        }
        Ok(device)
    }

    async fn configured_device(
        &self,
        id: &DeviceId,
    ) -> Result<crate::domain::entities::MediaDevice, DomainError> {
        let device = self
            .devices
            .find_by_id(id)
            .await?
            .filter(|device| device.enabled && device.current_mount_point.is_some())
            .ok_or(DomainError::MediaDeviceNotAllowed)?;
        Ok(device)
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_ini_destination(path: &Path) -> Result<(), DomainError> {
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("ini"))
        || !path.parent().is_some_and(Path::exists)
    {
        return Err(DomainError::MediaProfileInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::clock::FakeClock;
    use crate::domain::entities::{
        Game, GameProvider, LaunchKind, LaunchProfile, MediaDevice, MonitorPolicy,
    };
    use crate::domain::ports::{
        ActivationRepository, DeviceRepository, GameRepository, ProfileRepository,
    };
    use crate::infrastructure::database::fake_repos::{
        FakeActivationRepository, FakeDeviceRepository, FakeGameRepository, FakeMediaRepository,
        FakeProfileRepository,
    };
    use crate::infrastructure::fakes::FakeDevicePort;

    #[tokio::test]
    async fn preview_uses_only_portable_catalog_fields() {
        let clock = Arc::new(FakeClock::new());
        let now = clock.now_utc();
        let mut game = Game::new(GameProvider::Executable, None, "Local Game", now);
        game.installed = true;
        game.install_dir = Some(r"C:\Private\Game".into());
        let profile = LaunchProfile::new(game.id, "Local", LaunchKind::Executable, now)
            .with_executable_target(
                r"C:\Private\Game\secret.exe",
                Some(r"C:\Private\Game".into()),
                vec!["--token=secret".into()],
                vec!["secret.exe".into()],
            )
            .unwrap();
        let mut device = MediaDevice::new("USB Floppy", DriveType::Removable, now);
        device.interface_path = Some("floppy-1".into());
        device.current_mount_point = Some("A:".into());
        device.enabled = true;
        device.monitor_policy = MonitorPolicy::ExactDevice;

        let games = Arc::new(FakeGameRepository::new());
        GameRepository::upsert(games.as_ref(), &game).await.unwrap();
        let profiles = Arc::new(FakeProfileRepository::default());
        ProfileRepository::upsert(profiles.as_ref(), &profile)
            .await
            .unwrap();
        let devices = Arc::new(FakeDeviceRepository::new());
        DeviceRepository::upsert(devices.as_ref(), &device)
            .await
            .unwrap();
        let activations = Arc::new(FakeActivationRepository::default());
        let service = MediaCreatorService::new(
            MediaCreatorRepositories {
                games,
                profiles,
                devices,
                media: Arc::new(FakeMediaRepository::default()),
                activations: activations.clone(),
            },
            Arc::new(FakeDevicePort::default()),
            None,
            clock,
        );
        let selection = CreatorSelection {
            game_id: game.id,
            profile_id: profile.id,
            device_id: None,
            media_key: MediaKey::new("LOCAL-001").unwrap(),
            media_kind: None,
            include_artwork: true,
        };
        let preview = service.preview(&selection).await.unwrap();

        assert!(preview.canonical_ini.contains("SCHEMA=2"));
        assert!(preview.canonical_ini.contains("MEDIA_ID=LOCAL-001"));
        assert!(!preview.canonical_ini.contains("C:\\"));
        assert!(!preview.canonical_ini.contains("--token"));
        assert!(preview.include_artwork);

        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("GAME.INI");
        let receipt = service.export_ini(&selection, &destination).await.unwrap();
        assert_eq!(receipt.path, destination);
        assert_eq!(receipt.activation.export_count, 1);
        assert_eq!(
            activations
                .find_by_game_id(&game.id)
                .await
                .unwrap()
                .unwrap()
                .last_media_key,
            selection.media_key
        );
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            preview.canonical_ini
        );
    }
}
