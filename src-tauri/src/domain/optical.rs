use crate::domain::errors::DomainError;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OpticalRecorder {
    pub unique_id: String,
    pub vendor: String,
    pub product: String,
    pub volume_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpticalPhase {
    Validating,
    BuildingImage,
    InitializingHardware,
    FormattingMedia,
    CalibratingPower,
    WritingData,
    Finalizing,
    Verifying,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpticalProgress {
    pub phase: OpticalPhase,
    pub percent: u8,
    pub elapsed_seconds: u32,
    pub remaining_seconds: Option<u32>,
    pub cancellation_safe: bool,
}

#[derive(Default)]
pub struct OpticalCancel {
    requested: AtomicBool,
}

impl OpticalCancel {
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    pub fn requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

pub type OpticalProgressSink = Arc<dyn Fn(OpticalProgress) + Send + Sync>;

pub trait OpticalPort: Send + Sync {
    fn list_recorders(&self) -> Result<Vec<OpticalRecorder>, DomainError>;
    fn export_iso(&self, staging: &Path, destination: &Path) -> Result<(), DomainError>;
    fn burn(
        &self,
        staging: &Path,
        expected_unique_id: &str,
        cancel: Arc<OpticalCancel>,
        progress: OpticalProgressSink,
    ) -> Result<i32, DomainError>;
    fn erase(&self, expected_unique_id: &str) -> Result<i32, DomainError>;
}
