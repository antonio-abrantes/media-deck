//! Final removable-filesystem writer. The WebView supplies only a persisted
//! device ID; the current mount point is resolved and revalidated in the core.

use super::filesystem::{
    read_profile_bytes, AtomicProfileWriter, LocalProfileStorage, WriteReceipt,
};
use crate::domain::entities::{DriveType, MediaDevice};
use crate::domain::errors::DomainError;
use crate::domain::ids::SystemIdGenerator;
use crate::domain::media_profile::{parse_profile, MediaProfile};
use crate::domain::ports::DevicePort;
use std::path::PathBuf;
use std::sync::Arc;

pub struct FloppyMediaWriter {
    devices: Arc<dyn DevicePort>,
}

impl FloppyMediaWriter {
    pub fn new(devices: Arc<dyn DevicePort>) -> Self {
        Self { devices }
    }

    pub async fn write_verified(
        &self,
        configured: &MediaDevice,
        profile: &MediaProfile,
    ) -> Result<WriteReceipt, DomainError> {
        let (mount, before) = self.revalidate(configured).await?;
        let root = mount_root(&mount)?;
        let storage = LocalProfileStorage::from_validated_root(root.clone())?;
        let mut writer = AtomicProfileWriter::new(storage);
        let receipt = writer.write_verified(profile, &SystemIdGenerator)?;

        let (_, after) = self.revalidate(configured).await?;
        if !same_device(&before, &after) {
            return Err(DomainError::MediaChanged);
        }
        let bytes = read_profile_bytes(&root)?;
        let reread = parse_profile(&bytes, &SystemIdGenerator)
            .map_err(|_| DomainError::MediaVerificationFailed)?
            .profile;
        if &reread != profile {
            return Err(DomainError::MediaVerificationFailed);
        }
        Ok(receipt)
    }

    pub async fn read_revalidated(&self, configured: &MediaDevice) -> Result<Vec<u8>, DomainError> {
        let (mount, before) = self.revalidate(configured).await?;
        let bytes = read_profile_bytes(&mount_root(&mount)?)?;
        let (_, after) = self.revalidate(configured).await?;
        if !same_device(&before, &after) {
            return Err(DomainError::MediaChanged);
        }
        Ok(bytes)
    }

    async fn revalidate(
        &self,
        configured: &MediaDevice,
    ) -> Result<(String, MediaDevice), DomainError> {
        if !configured.enabled || configured.drive_type != DriveType::Removable {
            return Err(DomainError::MediaDeviceNotAllowed);
        }
        let mount = configured
            .current_mount_point
            .clone()
            .ok_or(DomainError::MediaNotReady)?;
        if !self.devices.is_ready(&mount).await? {
            return Err(DomainError::MediaNotReady);
        }
        let observed = self.devices.inspect_drive(&mount).await?;
        if observed.drive_type != DriveType::Removable || !same_device(configured, &observed) {
            return Err(DomainError::MediaChanged);
        }
        Ok((mount, observed))
    }
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

    #[test]
    fn accepts_only_drive_letters_from_persisted_devices() {
        assert_eq!(mount_root("a:").unwrap(), PathBuf::from("A:\\"));
        for invalid in ["A:\\folder", "\\\\server\\share", "C:/", ".", ""] {
            assert_eq!(mount_root(invalid), Err(DomainError::MediaDeviceNotAllowed));
        }
    }
}
