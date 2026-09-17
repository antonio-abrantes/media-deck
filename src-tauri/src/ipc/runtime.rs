//! Runtime session IPC: active snapshot, settings and simulated insert.

use crate::application::artwork::ArtworkService;
use crate::application::runtime::{
    RuntimeCoordinator, RuntimeEventSink, RuntimeSettingsDto, RuntimeStepDto, SessionSnapshotDto,
};
use crate::domain::entities::ArtworkId;
use crate::domain::errors::{AppError, DomainError};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfoDto {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub scale_factor: Option<f64>,
}

impl MonitorInfoDto {
    fn unavailable() -> Self {
        Self {
            width: None,
            height: None,
            scale_factor: None,
        }
    }
}

pub struct RuntimeCoordinatorState(pub Arc<RuntimeCoordinator>);
pub struct RuntimeArtworkState(pub Arc<ArtworkService>);

#[derive(Debug, serde::Serialize)]
pub struct RuntimeCoverDto {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub struct TauriRuntimeEventSink {
    app: AppHandle,
}

impl TauriRuntimeEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit_targets(
        &self,
        event: &str,
        payload: &impl serde::Serialize,
    ) -> Result<(), DomainError> {
        for label in ["runtime", "main"] {
            if let Some(window) = self.app.get_webview_window(label) {
                window
                    .emit(event, payload)
                    .map_err(|error| DomainError::DatabaseOperationFailed(error.to_string()))?;
            }
        }
        Ok(())
    }
}

impl RuntimeEventSink for TauriRuntimeEventSink {
    fn state_changed(&self, snapshot: &SessionSnapshotDto) -> Result<(), DomainError> {
        self.emit_targets("runtime://state_changed", snapshot)
    }

    fn step(&self, step: &RuntimeStepDto) -> Result<(), DomainError> {
        if let Some(window) = self.app.get_webview_window("runtime") {
            window
                .emit("runtime://step", step)
                .map_err(|error| DomainError::DatabaseOperationFailed(error.to_string()))?;
        }
        Ok(())
    }

    fn close_decision_required(&self, session_id: &str) -> Result<(), DomainError> {
        self.emit_targets(
            "runtime://close_decision_required",
            &serde_json::json!({ "sessionId": session_id }),
        )
    }
}

#[tauri::command]
pub fn session_get_active(
    state: State<'_, RuntimeCoordinatorState>,
) -> Result<SessionSnapshotDto, AppError> {
    Ok(state.0.snapshot())
}

#[tauri::command]
pub async fn settings_get_runtime(
    state: State<'_, RuntimeCoordinatorState>,
) -> Result<RuntimeSettingsDto, AppError> {
    state
        .0
        .runtime_settings()
        .await
        .map_err(|error| AppError::from_domain(&error))
}

#[tauri::command]
pub async fn runtime_simulate_insert(
    state: State<'_, RuntimeCoordinatorState>,
) -> Result<SessionSnapshotDto, AppError> {
    state
        .0
        .simulate_insert()
        .await
        .map_err(|error| AppError::from_domain(&error))
}

#[tauri::command]
pub async fn runtime_get_cover(
    runtime: State<'_, RuntimeCoordinatorState>,
    artwork: State<'_, RuntimeArtworkState>,
) -> Result<Option<RuntimeCoverDto>, AppError> {
    let Some(id) = runtime.0.snapshot().cover_artwork_id else {
        return Ok(None);
    };
    let id = id
        .parse::<ArtworkId>()
        .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
    let Some(resolved) = artwork.0.resolve_by_id(&id).await.map_err(AppError::from)? else {
        return Ok(None);
    };
    let metadata = tokio::fs::metadata(&resolved.absolute_path)
        .await
        .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err(AppError::from(DomainError::ArtworkInvalid));
    }
    let bytes = tokio::fs::read(&resolved.absolute_path)
        .await
        .map_err(|_| AppError::from(DomainError::ArtworkInvalid))?;
    Ok(Some(RuntimeCoverDto {
        mime_type: resolved.artwork.mime_type,
        bytes,
    }))
}

#[tauri::command]
pub fn runtime_cover_ready(state: State<'_, RuntimeCoordinatorState>) {
    state.0.mark_cover_rendered();
}

#[tauri::command]
pub fn runtime_animation_ready(state: State<'_, RuntimeCoordinatorState>) {
    state.0.mark_animation_rendered();
}

/// Returns the physical pixel dimensions of the monitor that owns the runtime
/// window. The launcher viewport remains 865x458 and is intentionally not used
/// as a substitute for the display resolution.
#[tauri::command]
pub fn runtime_get_monitor_info(window: tauri::WebviewWindow) -> MonitorInfoDto {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return MonitorInfoDto::unavailable();
    };
    let size = monitor.size();

    MonitorInfoDto {
        width: Some(size.width),
        height: Some(size.height),
        scale_factor: Some(monitor.scale_factor()),
    }
}
