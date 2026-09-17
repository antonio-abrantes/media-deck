use crate::application::devices::{
    DeviceEventSink, DeviceService, MediaInsertedEvent, MediaRemovedEvent,
};
use crate::application::runtime::RuntimeCoordinator;
use crate::application::session::{CloseDecision, SessionService};
use crate::domain::entities::{
    DriveType, LaunchKind, LaunchProfile, MediaDevice, MonitorPolicy, SessionId,
};
use crate::domain::errors::{AppError, DomainError};
use crate::domain::ids::SystemIdGenerator;
use crate::domain::media_profile::{parse_profile, MediaProfile};
use crate::domain::ports::{CloseResult, GameProvider, ProfileRepository, SettingsRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

const PHYSICAL_MEDIA_CLOSE_TIMEOUT_SECS: u32 = 3;

pub struct DeviceServiceState(pub Arc<DeviceService>);

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub friendly_name: String,
    pub drive_type: DriveType,
    pub current_mount_point: Option<String>,
    pub monitor_policy: MonitorPolicy,
    pub enabled: bool,
    pub has_stable_identity: bool,
    pub capabilities: serde_json::Value,
}

impl From<MediaDevice> for DeviceDto {
    fn from(device: MediaDevice) -> Self {
        Self {
            id: device.id.to_string(),
            friendly_name: device.friendly_name,
            drive_type: device.drive_type,
            current_mount_point: device.current_mount_point,
            monitor_policy: device.monitor_policy,
            enabled: device.enabled,
            has_stable_identity: device.device_instance_id.is_some()
                || device.interface_path.is_some(),
            capabilities: device.capabilities,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfigureRequest {
    pub device_id: String,
    pub enabled: bool,
    pub monitor_policy: MonitorPolicy,
}

#[tauri::command]
pub async fn device_list(state: State<'_, DeviceServiceState>) -> Result<Vec<DeviceDto>, AppError> {
    state
        .0
        .list_eligible()
        .await
        .map(|devices| devices.into_iter().map(DeviceDto::from).collect())
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn device_configure(
    request: DeviceConfigureRequest,
    state: State<'_, DeviceServiceState>,
) -> Result<DeviceDto, AppError> {
    let id = request
        .device_id
        .parse()
        .map_err(|_| AppError::from(DomainError::MediaDeviceNotAllowed))?;
    state
        .0
        .configure(&id, request.enabled, request.monitor_policy)
        .await
        .map(DeviceDto::from)
        .map_err(AppError::from)
}

pub struct TauriDeviceEventSink<R: Runtime> {
    app: AppHandle<R>,
    runtime: Arc<RuntimeCoordinator>,
    profiles: Arc<dyn ProfileRepository>,
    settings: Arc<dyn SettingsRepository>,
    sessions: Arc<SessionService>,
}

impl<R: Runtime> TauriDeviceEventSink<R> {
    pub fn new(
        app: AppHandle<R>,
        runtime: Arc<RuntimeCoordinator>,
        profiles: Arc<dyn ProfileRepository>,
        settings: Arc<dyn SettingsRepository>,
        sessions: Arc<SessionService>,
    ) -> Self {
        Self {
            app,
            runtime,
            profiles,
            settings,
            sessions,
        }
    }

    fn emit_to_window<T: Serialize>(
        &self,
        label: &str,
        event_name: &str,
        payload: &T,
    ) -> Result<(), DomainError> {
        self.app
            .get_webview_window(label)
            .ok_or(DomainError::MediaIoFailed)?
            .emit(event_name, payload)
            .map_err(|_| DomainError::MediaIoFailed)
    }
}

impl<R: Runtime> DeviceEventSink for TauriDeviceEventSink<R> {
    fn media_inserted(&self, event: &MediaInsertedEvent) -> Result<(), DomainError> {
        self.emit_to_window("main", "device://media_inserted", event)?;
        let app = self.app.clone();
        let runtime = self.runtime.clone();
        let profiles = self.profiles.clone();
        let settings = self.settings.clone();
        let sessions = self.sessions.clone();
        let event = event.clone();
        tauri::async_runtime::spawn(async move {
            let result = read_known_profile(&event.mount_point, profiles.as_ref()).await;
            match result {
                Ok((media_profile, launch_profile)) => {
                    let snapshot =
                        match runtime.present_valid_media(&media_profile, &event.mount_point) {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                tracing::warn!(
                                    code = error.code(),
                                    "valid media could not enter runtime"
                                );
                                return;
                            }
                        };
                    let runtime_window = app.get_webview_window("runtime");
                    if let Some(window) = &runtime_window {
                        // Leave minimized state before offscreen show so the
                        // WebView can paint the cover (show alone does not restore).
                        let _ = window.unminimize();
                        let _ = window.set_position(tauri::PhysicalPosition::new(-32_000, -32_000));
                        let _ = window.show();
                        let _ = window.emit("device://media_inserted", &event);
                    }
                    if let Err(error) = runtime.wait_for_cover_render().await {
                        tracing::warn!(code = error.code(), "runtime cover did not render");
                        if let Some(window) = &runtime_window {
                            let _ = window.hide();
                        }
                        if runtime.snapshot().mount_point.as_deref()
                            == Some(event.mount_point.as_str())
                        {
                            let _ = runtime.fail_physical_launch();
                        }
                        return;
                    }
                    if let Some(window) = &runtime_window {
                        let monitor_id = settings
                            .get("launcher_monitor")
                            .await
                            .ok()
                            .flatten()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "auto".into());
                        position_runtime_window(window, &monitor_id);
                        reveal_runtime_window(window);
                    }
                    if let Err(error) = runtime.complete_physical_presentation().await {
                        tracing::warn!(code = error.code(), "valid media could not enter runtime");
                        if runtime.snapshot().mount_point.as_deref()
                            == Some(event.mount_point.as_str())
                        {
                            let _ = runtime.fail_physical_launch();
                        }
                        return;
                    }
                    let Some(session_id) = snapshot
                        .session_id
                        .as_deref()
                        .and_then(|value| value.parse::<SessionId>().ok())
                    else {
                        let _ = runtime.fail_physical_launch();
                        return;
                    };
                    let provider: Box<dyn GameProvider> = match launch_profile.launch_kind {
                        LaunchKind::Steam { .. } => {
                            Box::new(crate::infrastructure::providers::SteamProvider::default())
                        }
                        LaunchKind::Executable => {
                            Box::new(crate::infrastructure::providers::ExecutableProvider)
                        }
                    };
                    match sessions
                        .launch_and_track(
                            session_id,
                            media_profile.media_id.clone(),
                            provider.as_ref(),
                            &launch_profile,
                            &media_profile.process_hints,
                            Duration::from_secs(30),
                        )
                        .await
                    {
                        Ok(_) => {
                            if runtime.snapshot().mount_point.as_deref()
                                == Some(event.mount_point.as_str())
                            {
                                let _ = runtime.complete_physical_launch();
                            } else {
                                let _ = sessions.request_close(&session_id, 5).await;
                            }
                        }
                        Err(error) => {
                            tracing::error!(code = error.code(), "physical media launch failed");
                            let _ = runtime.fail_physical_launch();
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        code = error.code(),
                        mount_point = %event.mount_point,
                        "inserted media was ignored"
                    );
                }
            }
        });
        Ok(())
    }

    fn media_removed(&self, event: &MediaRemovedEvent) -> Result<(), DomainError> {
        self.emit_to_window("main", "device://media_removed", event)?;
        let snapshot = self.runtime.snapshot();
        if snapshot.mount_point.as_deref() != Some(event.mount_point.as_str()) {
            return Ok(());
        }
        let app = self.app.clone();
        let runtime = self.runtime.clone();
        let sessions = self.sessions.clone();
        let event = event.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(window) = app.get_webview_window("runtime") {
                let _ = window.emit("device://media_removed", &event);
            }
            let session_identity = snapshot
                .session_id
                .as_deref()
                .and_then(|value| value.parse::<SessionId>().ok())
                .zip(snapshot.media_key.clone());
            let close_result = match &session_identity {
                Some((session_id, _)) => {
                    sessions
                        .request_close(session_id, PHYSICAL_MEDIA_CLOSE_TIMEOUT_SECS)
                        .await
                }
                None => Ok(CloseResult::AlreadyGone),
            };
            match close_result {
                Ok(CloseResult::TimedOut) => {
                    if let Some((session_id, media_key)) = session_identity {
                        if let Err(error) = sessions
                            .resolve_close(&session_id, &media_key, CloseDecision::Force)
                            .await
                        {
                            tracing::error!(
                                code = error.code(),
                                "strongly bound game process could not be terminated"
                            );
                            let _ = runtime.mark_close_decision();
                            return;
                        }
                    }
                    let _ = runtime.reset_after_media_removed();
                    if let Some(window) = app.get_webview_window("runtime") {
                        let _ = window.hide();
                    }
                }
                Ok(CloseResult::Exited | CloseResult::AlreadyGone) => {
                    let _ = runtime.reset_after_media_removed();
                    if let Some(window) = app.get_webview_window("runtime") {
                        let _ = window.hide();
                    }
                }
                Err(error) => {
                    tracing::error!(code = error.code(), "game close request failed");
                    let _ = runtime.mark_close_decision();
                }
            }
        });
        Ok(())
    }
}

fn reveal_runtime_window<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn position_runtime_window<R: Runtime>(window: &tauri::WebviewWindow<R>, monitor_id: &str) {
    if monitor_id == "auto" {
        let _ = window.center();
        return;
    }
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    let Some(monitor) = monitors.into_iter().find(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        let name = monitor.name().map_or("Monitor", String::as_str);
        crate::ipc::settings::monitor_id(name, position.x, position.y, size.width, size.height)
            == monitor_id
    }) else {
        return;
    };
    let Ok(window_size) = window.outer_size() else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let x = monitor_position.x
        + (i64::from(monitor_size.width) - i64::from(window_size.width)).max(0) as i32 / 2;
    let y = monitor_position.y
        + (i64::from(monitor_size.height) - i64::from(window_size.height)).max(0) as i32 / 2;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

async fn read_known_profile(
    mount_point: &str,
    profiles: &dyn ProfileRepository,
) -> Result<(MediaProfile, LaunchProfile), DomainError> {
    let root = format!("{}\\", mount_point.trim_end_matches(['\\', '/']));
    let path = std::path::Path::new(&root).join("GAME.INI");
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| DomainError::MediaProfileMissing)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 {
        return Err(DomainError::MediaProfileInvalid);
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| DomainError::MediaIoFailed)?;
    let parsed =
        parse_profile(&bytes, &SystemIdGenerator).map_err(|_| DomainError::MediaProfileInvalid)?;
    let launch_profile = profiles
        .find_by_id(&parsed.profile.profile_id)
        .await?
        .ok_or(DomainError::LaunchProfileInvalid)?;
    let matches_profile = match (&parsed.profile.provider, &launch_profile.launch_kind) {
        (crate::domain::media_profile::MediaProvider::Steam, LaunchKind::Steam { app_id }) => {
            parsed.profile.app_id == Some(*app_id)
        }
        (crate::domain::media_profile::MediaProvider::Executable, LaunchKind::Executable) => true,
        _ => false,
    };
    if !launch_profile.enabled || !matches_profile {
        return Err(DomainError::LaunchProfileInvalid);
    }
    Ok((parsed.profile, launch_profile))
}
