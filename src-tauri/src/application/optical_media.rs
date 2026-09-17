//! Optical-media orchestration. All paths are derived from application-owned
//! roots; IPC supplies persisted IDs and an enumerated recorder ID only.

use crate::application::artwork::ArtworkService;
use crate::application::media_creator::{CreatorSelection, MediaCreatorService};
use crate::domain::artwork::ArtworkKind;
use crate::domain::entities::{DeviceId, DriveType, MediaDescriptor, MediaDevice, MediaKind};
use crate::domain::errors::DomainError;
use crate::domain::ids::SystemIdGenerator;
use crate::domain::media_profile::{parse_profile, write_canonical_v2, ProfileSourceVersion};
use crate::domain::optical::{OpticalCancel, OpticalPort, OpticalProgressSink, OpticalRecorder};
use crate::domain::ports::{DevicePort, DeviceRepository};
use crate::infrastructure::media::filesystem::read_profile_bytes;
use crate::infrastructure::media::staging::{StagedMedia, StagingWriter};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpticalArtifact {
    pub operation_id: String,
    pub path: String,
    pub canonical_hash: String,
    pub artwork_included: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EraseChallenge {
    pub recorder_id: String,
    pub target: String,
    pub confirmation_text: String,
}

pub struct OpticalMediaService {
    creator: Arc<MediaCreatorService>,
    artwork: Arc<ArtworkService>,
    devices: Arc<dyn DeviceRepository>,
    device_port: Arc<dyn DevicePort>,
    optical: Arc<dyn OpticalPort>,
    staging_root: PathBuf,
    export_root: PathBuf,
    active: Mutex<HashMap<String, Arc<OpticalCancel>>>,
}

impl OpticalMediaService {
    pub fn new(
        creator: Arc<MediaCreatorService>,
        artwork: Arc<ArtworkService>,
        devices: Arc<dyn DeviceRepository>,
        device_port: Arc<dyn DevicePort>,
        optical: Arc<dyn OpticalPort>,
        export_root: PathBuf,
    ) -> Result<Self, DomainError> {
        let staging_root = export_root.join("staging");
        std::fs::create_dir_all(&staging_root).map_err(|_| DomainError::MediaIoFailed)?;
        StagingWriter::new(&staging_root)?;
        Ok(Self {
            creator,
            artwork,
            devices,
            device_port,
            optical,
            staging_root,
            export_root,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub async fn recorders(
        &self,
        device_id: DeviceId,
    ) -> Result<Vec<OpticalRecorder>, DomainError> {
        let device = self.revalidate_device(device_id).await?;
        let mount = device
            .current_mount_point
            .as_deref()
            .ok_or(DomainError::MediaNotReady)?;
        let optical = self.optical.clone();
        let recorders = tokio::task::spawn_blocking(move || optical.list_recorders())
            .await
            .map_err(|_| DomainError::OpticalBurnFailed)??;
        let matching: Vec<_> = recorders
            .into_iter()
            .filter(|recorder| recorder_matches_mount(recorder, mount))
            .collect();
        if matching.len() != 1 {
            return Err(DomainError::MediaDeviceNotAllowed);
        }
        Ok(matching)
    }

    pub async fn export_iso(
        &self,
        selection: &CreatorSelection,
    ) -> Result<OpticalArtifact, DomainError> {
        let (operation_id, staged) = self.stage(selection).await?;
        let destination = self.export_root.join(format!(
            "{}-{operation_id}.iso",
            selection.media_key.as_str()
        ));
        let staging = staged.root.clone();
        let output = destination.clone();
        let optical = self.optical.clone();
        tokio::task::spawn_blocking(move || optical.export_iso(&staging, &output))
            .await
            .map_err(|_| DomainError::OpticalBurnFailed)??;
        Ok(OpticalArtifact {
            operation_id,
            path: destination.to_string_lossy().into_owned(),
            canonical_hash: staged.canonical_hash,
            artwork_included: staged.artwork_included,
        })
    }

    pub async fn burn(
        &self,
        selection: &CreatorSelection,
        recorder_id: String,
        operation_id: String,
        progress: OpticalProgressSink,
    ) -> Result<MediaDescriptor, DomainError> {
        let device_id = selection
            .device_id
            .ok_or(DomainError::MediaDeviceNotAllowed)?;
        let before = self.revalidate_device(device_id).await?;
        let matching = self.recorders(device_id).await?;
        if matching.len() != 1 || matching[0].unique_id != recorder_id {
            return Err(DomainError::MediaChanged);
        }
        let staged = self.stage_with_id(selection, &operation_id).await?;
        let cancel = Arc::new(OpticalCancel::default());
        self.active
            .lock()
            .map_err(|_| DomainError::OpticalBurnFailed)?
            .insert(operation_id.clone(), cancel.clone());

        let optical = self.optical.clone();
        let staging = staged.root.clone();
        let burn_recorder = recorder_id.clone();
        let burn = tokio::task::spawn_blocking(move || {
            optical.burn(&staging, &burn_recorder, cancel, progress)
        })
        .await
        .map_err(|_| DomainError::OpticalBurnFailed)?;
        self.active
            .lock()
            .map_err(|_| DomainError::OpticalBurnFailed)?
            .remove(&operation_id);
        burn?;

        let mount = before
            .current_mount_point
            .clone()
            .ok_or(DomainError::MediaNotReady)?;
        self.wait_for_ready(&mount).await?;
        let after = self.revalidate_device(device_id).await?;
        if !same_device(&before, &after) {
            return Err(DomainError::MediaChanged);
        }
        self.verify_readback(selection, &staged, &mount)?;
        self.creator
            .record_verified_optical(selection, staged.canonical_hash, mount)
            .await
    }

    pub fn cancel(&self, operation_id: &str) -> Result<(), DomainError> {
        let operations = self
            .active
            .lock()
            .map_err(|_| DomainError::OpticalBurnFailed)?;
        let operation =
            operations
                .get(operation_id)
                .ok_or_else(|| DomainError::InvalidTransition {
                    state: "not_burning".into(),
                    event: "cancel".into(),
                })?;
        operation.request();
        Ok(())
    }

    pub async fn erase_challenge(
        &self,
        device_id: DeviceId,
    ) -> Result<EraseChallenge, DomainError> {
        let device = self.revalidate_device(device_id).await?;
        let recorder = self
            .recorders(device_id)
            .await?
            .into_iter()
            .next()
            .ok_or(DomainError::MediaDeviceNotAllowed)?;
        let target = format!(
            "{} · {}",
            device.friendly_name,
            device.current_mount_point.unwrap_or_default()
        );
        Ok(EraseChallenge {
            recorder_id: recorder.unique_id,
            confirmation_text: format!("ERASE {} {}", recorder.product.trim(), target),
            target,
        })
    }

    pub async fn erase(
        &self,
        device_id: DeviceId,
        recorder_id: &str,
        typed_confirmation: &str,
    ) -> Result<(), DomainError> {
        let before = self.revalidate_device(device_id).await?;
        let challenge = self.erase_challenge(device_id).await?;
        if challenge.recorder_id != recorder_id || challenge.confirmation_text != typed_confirmation
        {
            return Err(DomainError::InvalidTransition {
                state: "erase_confirmation".into(),
                event: "confirmation_mismatch".into(),
            });
        }
        let optical = self.optical.clone();
        let recorder = recorder_id.to_owned();
        tokio::task::spawn_blocking(move || optical.erase(&recorder))
            .await
            .map_err(|_| DomainError::OpticalBurnFailed)??;
        let after = self.revalidate_device(device_id).await?;
        if !same_device(&before, &after) {
            return Err(DomainError::MediaChanged);
        }
        Ok(())
    }

    async fn stage(
        &self,
        selection: &CreatorSelection,
    ) -> Result<(String, StagedMedia), DomainError> {
        let operation_id = uuid::Uuid::now_v7().to_string();
        let staged = self.stage_with_id(selection, &operation_id).await?;
        Ok((operation_id, staged))
    }

    async fn stage_with_id(
        &self,
        selection: &CreatorSelection,
        operation_id: &str,
    ) -> Result<StagedMedia, DomainError> {
        if selection.media_kind != Some(MediaKind::Optical) {
            return Err(DomainError::OpticalMediaNotWritable);
        }
        self.revalidate_device(
            selection
                .device_id
                .ok_or(DomainError::MediaDeviceNotAllowed)?,
        )
        .await?;
        let preview = self.creator.preview(selection).await?;
        let artwork = if selection.include_artwork {
            Some(PathBuf::from(
                self.artwork
                    .resolve_active(&selection.game_id, ArtworkKind::LauncherCover)
                    .await?
                    .ok_or(DomainError::ArtworkInvalid)?
                    .absolute_path,
            ))
        } else {
            None
        };
        let staged = StagingWriter::new(&self.staging_root)?.prepare(
            operation_id,
            &preview.canonical_ini,
            artwork.as_deref(),
        )?;
        Ok(staged)
    }

    async fn revalidate_device(&self, device_id: DeviceId) -> Result<MediaDevice, DomainError> {
        let configured = self
            .devices
            .find_by_id(&device_id)
            .await?
            .filter(|device| device.enabled && device.drive_type == DriveType::CdRom)
            .ok_or(DomainError::MediaDeviceNotAllowed)?;
        let mount = configured
            .current_mount_point
            .as_deref()
            .ok_or(DomainError::MediaNotReady)?;
        let observed = self.device_port.inspect_drive(mount).await?;
        if observed.drive_type != DriveType::CdRom || !same_device(&configured, &observed) {
            return Err(DomainError::MediaChanged);
        }
        Ok(configured)
    }

    async fn wait_for_ready(&self, mount: &str) -> Result<(), DomainError> {
        for _ in 0..40 {
            if self.device_port.is_ready(mount).await.unwrap_or(false) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err(DomainError::OpticalVerifyFailed)
    }

    fn verify_readback(
        &self,
        selection: &CreatorSelection,
        staged: &StagedMedia,
        mount: &str,
    ) -> Result<(), DomainError> {
        let bytes = read_profile_bytes(&mount_root(mount)?)?;
        let parsed = parse_profile(&bytes, &SystemIdGenerator)
            .map_err(|_| DomainError::OpticalVerifyFailed)?;
        let canonical =
            write_canonical_v2(&parsed.profile).map_err(|_| DomainError::OpticalVerifyFailed)?;
        if parsed.source_version != ProfileSourceVersion::V2
            || parsed.profile.media_id != selection.media_key
            || parsed.profile.profile_id != selection.profile_id
            || format!("{:x}", Sha256::digest(canonical.as_bytes())) != staged.canonical_hash
        {
            return Err(DomainError::OpticalVerifyFailed);
        }
        Ok(())
    }
}

fn recorder_matches_mount(recorder: &OpticalRecorder, mount: &str) -> bool {
    let expected = format!("{}\\", mount.to_ascii_uppercase());
    recorder
        .volume_paths
        .iter()
        .any(|path| path.to_ascii_uppercase() == expected)
}

fn same_device(left: &MediaDevice, right: &MediaDevice) -> bool {
    match (&left.device_instance_id, &right.device_instance_id) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => match (&left.interface_path, &right.interface_path) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            _ => false,
        },
    }
}

fn mount_root(mount: &str) -> Result<PathBuf, DomainError> {
    let bytes = mount.as_bytes();
    if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return Err(DomainError::MediaDeviceNotAllowed);
    }
    Ok(PathBuf::from(format!("{}\\", mount.to_ascii_uppercase())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn recorder_selection_is_bound_to_exact_mount() {
        let recorder = OpticalRecorder {
            unique_id: "recorder-1".into(),
            vendor: "ACME".into(),
            product: "Writer".into(),
            volume_paths: vec!["G:\\".into()],
        };
        assert!(recorder_matches_mount(&recorder, "g:"));
        assert!(!recorder_matches_mount(&recorder, "H:"));
    }

    #[test]
    fn mount_root_rejects_paths_and_unc() {
        assert_eq!(mount_root("G:").unwrap(), Path::new("G:\\"));
        assert!(mount_root("G:\\folder").is_err());
        assert!(mount_root("\\\\server\\disc").is_err());
    }
}
