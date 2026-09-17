use crate::application::settings::{AdminSettings, AdminSettingsService};
use crate::domain::errors::{AppError, DomainError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

pub struct AdminSettingsState(pub Arc<AdminSettingsService>);

#[derive(Debug, Serialize)]
pub struct AdminSettingsDto {
    pub monitor_active: bool,
    pub autostart: bool,
    pub launcher_monitor: String,
}

impl From<AdminSettings> for AdminSettingsDto {
    fn from(value: AdminSettings) -> Self {
        Self {
            monitor_active: value.monitor_active,
            autostart: value.autostart,
            launcher_monitor: value.launcher_monitor,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BooleanSettingRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorSettingRequest {
    pub monitor_id: String,
}

#[derive(Debug, Serialize)]
pub struct DisplayMonitorDto {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

#[tauri::command]
pub async fn settings_get_admin(
    state: State<'_, AdminSettingsState>,
) -> Result<AdminSettingsDto, AppError> {
    state
        .0
        .get()
        .await
        .map(AdminSettingsDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn settings_set_monitor_active(
    request: BooleanSettingRequest,
    state: State<'_, AdminSettingsState>,
) -> Result<AdminSettingsDto, AppError> {
    state
        .0
        .set_monitor_active(request.enabled)
        .await
        .map(AdminSettingsDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn settings_set_autostart(
    request: BooleanSettingRequest,
    state: State<'_, AdminSettingsState>,
) -> Result<AdminSettingsDto, AppError> {
    state
        .0
        .set_autostart(request.enabled)
        .await
        .map(AdminSettingsDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn settings_list_monitors(app: AppHandle) -> Result<Vec<DisplayMonitorDto>, AppError> {
    available_monitors(&app)
}

#[tauri::command]
pub async fn settings_set_launcher_monitor(
    request: MonitorSettingRequest,
    app: AppHandle,
    state: State<'_, AdminSettingsState>,
) -> Result<AdminSettingsDto, AppError> {
    if request.monitor_id != "auto"
        && !available_monitors(&app)?
            .iter()
            .any(|monitor| monitor.id == request.monitor_id)
    {
        return Err(AppError::from(DomainError::SettingInvalid(
            "launcher_monitor".into(),
        )));
    }
    state
        .0
        .set_launcher_monitor(request.monitor_id)
        .await
        .map(AdminSettingsDto::from)
        .map_err(AppError::from)
}

fn available_monitors(app: &AppHandle) -> Result<Vec<DisplayMonitorDto>, AppError> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::from(DomainError::SettingInvalid("launcher_monitor".into())))?;
    let monitors = window
        .available_monitors()
        .map_err(|_| AppError::from(DomainError::SettingInvalid("launcher_monitor".into())))?;
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let position = monitor.position();
            let size = monitor.size();
            let name = monitor
                .name()
                .cloned()
                .unwrap_or_else(|| format!("Monitor {}", index + 1));
            DisplayMonitorDto {
                id: monitor_id(&name, position.x, position.y, size.width, size.height),
                label: format!(
                    "{} · {}×{} · posição {},{}",
                    name, size.width, size.height, position.x, position.y
                ),
                width: size.width,
                height: size.height,
                scale_factor: monitor.scale_factor(),
            }
        })
        .collect())
}

pub fn monitor_id(name: &str, x: i32, y: i32, width: u32, height: u32) -> String {
    format!("{name}|{x}|{y}|{width}|{height}")
}
