use crate::application::maintenance::{MaintenanceReceipt, MaintenanceService};
use crate::domain::errors::AppError;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

pub struct MaintenanceState(pub Arc<MaintenanceService>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenancePathRequest {
    pub path: String,
}

#[tauri::command]
pub async fn maintenance_export_diagnostics(
    request: MaintenancePathRequest,
    state: State<'_, MaintenanceState>,
) -> Result<MaintenanceReceipt, AppError> {
    state
        .0
        .export_diagnostics(Path::new(&request.path))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn maintenance_export_backup(
    request: MaintenancePathRequest,
    state: State<'_, MaintenanceState>,
) -> Result<MaintenanceReceipt, AppError> {
    state
        .0
        .export_backup(Path::new(&request.path))
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub fn maintenance_stage_restore(
    request: MaintenancePathRequest,
    state: State<'_, MaintenanceState>,
) -> Result<MaintenanceReceipt, AppError> {
    state
        .0
        .stage_restore(Path::new(&request.path))
        .map_err(AppError::from)
}
