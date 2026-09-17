#![deny(warnings)]

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod ipc;
pub mod support;

use tauri::Manager;
use tracing::level_filters::LevelFilter;
use tracing::{error, info};

#[derive(Default)]
struct AppLifecycleState {
    quitting: std::sync::atomic::AtomicBool,
}

#[cfg(windows)]
struct DeviceRuntimeState {
    monitor: tauri::async_runtime::JoinHandle<()>,
    _source: infrastructure::devices::Win32DeviceEventSource,
}

#[cfg(windows)]
impl Drop for DeviceRuntimeState {
    fn drop(&mut self) {
        self.monitor.abort();
    }
}

#[cfg(windows)]
fn install_tray(app: &tauri::App) -> Result<(), tauri::Error> {
    use std::sync::atomic::Ordering;
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(app, "open", "Abrir MediaDeck", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "Monitor: ativo", false, None::<&str>)?;
    let monitor = MenuItem::with_id(
        app,
        "toggle-monitor",
        "Pausar/retomar monitoramento",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Sair do MediaDeck", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &status, &monitor, &quit])?;
    let mut builder = TrayIconBuilder::with_id("mediadeck")
        .tooltip("MediaDeck")
        .menu(&menu)
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "toggle-monitor" => {
                let handle = app.clone();
                let status = status.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<ipc::settings::AdminSettingsState>();
                    if let Ok(current) = state.0.get().await {
                        if let Ok(updated) =
                            state.0.set_monitor_active(!current.monitor_active).await
                        {
                            let text = if updated.monitor_active {
                                "Monitor: ativo"
                            } else {
                                "Monitor: pausado"
                            };
                            let _ = status.set_text(text);
                        }
                    }
                });
            }
            "quit" => {
                app.state::<AppLifecycleState>()
                    .quitting
                    .store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(windows)]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

async fn initialize_storage(
    app_dirs: &support::dirs::AppDirs,
) -> Result<sqlx::SqlitePool, domain::errors::DomainError> {
    infrastructure::database::open_and_migrate(&app_dirs.database).await
}

#[cfg(windows)]
fn clamp_main_window_to_work_area(window: &tauri::WebviewWindow) {
    use std::mem::size_of;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let available_width = f64::from(info.rcWork.right - info.rcWork.left) / scale;
    let available_height = f64::from(info.rcWork.bottom - info.rcWork.top) / scale;
    let width = 1100.0_f64.min(available_width.max(1.0));
    let height = 700.0_f64.min(available_height.max(1.0));
    let minimum = tauri::LogicalSize::new(900.0_f64.min(width), 560.0_f64.min(height));
    let size = tauri::LogicalSize::new(width, height);
    let _ = window.set_min_size(Some(minimum));
    let _ = window.set_size(size);
    let _ = window.center();
}

pub fn run() {
    let start_background = std::env::args().any(|argument| argument == "--background");
    // ── 1. Resolve application data directories ──────────────────────────────
    let app_dirs = match support::dirs::AppDirs::init() {
        Ok(d) => d,
        Err(e) => {
            // Can't log yet; print to stderr and abort.
            eprintln!("[MediaDeck] FATAL: cannot initialise data directories: {e}");
            std::process::exit(1);
        }
    };
    if let Err(error) = application::maintenance::apply_pending_restore(&app_dirs) {
        eprintln!("[MediaDeck] FATAL: pending restore failed: {error}");
        std::process::exit(1);
    }

    // ── 2. Initialise structured logging ────────────────────────────────────
    let _log_guard = match support::tracing_setup::init(&app_dirs.logs, LevelFilter::INFO) {
        Ok(guard) => Some(guard),
        Err(e) => {
            // Non-fatal: tracing may already be initialised (e.g. in integration tests).
            eprintln!("[MediaDeck] WARNING: tracing init: {e}");
            None
        }
    };

    info!(
        version = env!("CARGO_PKG_VERSION"),
        db_file = app_dirs
            .database
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("media-deck.db"),
        "MediaDeck starting"
    );

    // ── 3. Open SQLite and run migrations (async block in sync context) ──────
    let startup_correlation_id = domain::entities::CorrelationId::new();
    let startup_span =
        support::tracing_setup::correlation_span(&startup_correlation_id, "database_startup");
    let _startup_guard = startup_span.enter();
    let pool = tauri::async_runtime::block_on(initialize_storage(&app_dirs));

    let pool = match pool {
        Ok(p) => {
            info!("database ready");
            p
        }
        Err(e) => {
            error!(error = %e, "database migration failed — cannot start");
            std::process::exit(1);
        }
    };

    // ── 4. Build and run Tauri application ───────────────────────────────────
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let quitting = window
                    .app_handle()
                    .state::<AppLifecycleState>()
                    .quitting
                    .load(std::sync::atomic::Ordering::SeqCst);
                if !quitting {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::collection::collection_list,
            ipc::collection::collection_get,
            ipc::collection::collection_cover_get,
            ipc::collection::collection_deactivate,
            ipc::media::media_validate,
            ipc::media::media_creator_preview,
            ipc::media::media_creator_export_ini,
            ipc::media::media_creator_write,
            ipc::media::media_import_inspect,
            ipc::media::media_import_upgrade,
            ipc::media::media_history_list,
            ipc::media::media_history_verify,
            ipc::media::media_optical_recorders,
            ipc::media::media_optical_export_iso,
            ipc::media::media_optical_burn,
            ipc::media::media_optical_cancel,
            ipc::media::media_optical_erase_challenge,
            ipc::media::media_optical_erase,
            ipc::maintenance::maintenance_export_diagnostics,
            ipc::maintenance::maintenance_export_backup,
            ipc::maintenance::maintenance_stage_restore,
            ipc::library::library_scan_steam,
            ipc::library::library_list,
            ipc::library::library_get,
            ipc::library::library_register_executable,
            ipc::library::library_update_profile,
            ipc::library::library_set_cover,
            ipc::library::library_review_shortcut,
            ipc::labels::label_project_list,
            ipc::labels::label_project_get,
            ipc::labels::label_project_upsert,
            ipc::labels::label_project_delete,
            ipc::labels::label_artwork_list,
            ipc::labels::label_artwork_get,
            ipc::labels::label_artwork_import_clipboard,
            ipc::labels::label_export,
            ipc::labels::label_calibration_export,
            ipc::labels::label_thumbnail_get,
            ipc::devices::device_list,
            ipc::devices::device_configure,
            ipc::settings::settings_get_admin,
            ipc::settings::settings_set_monitor_active,
            ipc::settings::settings_set_autostart,
            ipc::settings::settings_list_monitors,
            ipc::settings::settings_set_launcher_monitor,
            ipc::session::session_resolve_close,
            ipc::runtime::session_get_active,
            ipc::runtime::settings_get_runtime,
            ipc::runtime::runtime_simulate_insert,
            ipc::runtime::runtime_get_cover,
            ipc::runtime::runtime_cover_ready,
            ipc::runtime::runtime_animation_ready,
            ipc::runtime::runtime_get_monitor_info
        ])
        .manage(AppLifecycleState::default())
        .manage(pool)
        .manage(app_dirs)
        .setup(move |app| {
            #[cfg(windows)]
            if let Some(main) = app.get_webview_window("main") {
                clamp_main_window_to_work_area(&main);
            }
            #[cfg(windows)]
            {
                use std::sync::Arc;

                install_tray(app)?;
                if start_background {
                    if let Some(main) = app.get_webview_window("main") {
                        let _ = main.hide();
                    }
                }

                let pool = app.state::<sqlx::SqlitePool>().inner().clone();
                app.manage(ipc::maintenance::MaintenanceState(Arc::new(
                    application::maintenance::MaintenanceService::new(
                        pool.clone(),
                        Arc::new(app.state::<support::dirs::AppDirs>().inner().clone()),
                    ),
                )));
                let device_port: Arc<dyn domain::ports::DevicePort> =
                    Arc::new(infrastructure::devices::Win32DeviceAdapter);
                let repository: Arc<dyn domain::ports::DeviceRepository> = Arc::new(
                    infrastructure::database::sessions::SqliteDeviceRepository::new(pool.clone()),
                );
                let settings: Arc<dyn domain::ports::SettingsRepository> = Arc::new(
                    infrastructure::database::settings::SqliteSettingsRepository::new(
                        app.state::<sqlx::SqlitePool>().inner().clone(),
                    ),
                );
                let admin_settings = Arc::new(application::settings::AdminSettingsService::new(
                    settings.clone(),
                    Arc::new(infrastructure::autostart::WindowsAutostart),
                ));
                app.manage(ipc::settings::AdminSettingsState(admin_settings));
                let runtime_sink: Arc<dyn application::runtime::RuntimeEventSink> = Arc::new(
                    ipc::runtime::TauriRuntimeEventSink::new(app.handle().clone()),
                );
                let runtime = Arc::new(application::runtime::RuntimeCoordinator::new(
                    settings.clone(),
                    runtime_sink,
                ));
                app.manage(ipc::runtime::RuntimeCoordinatorState(runtime.clone()));
                info!("runtime coordinator ready");

                let service = Arc::new(application::devices::DeviceService::new(
                    device_port.clone(),
                    repository.clone(),
                ));
                app.manage(ipc::devices::DeviceServiceState(service));

                let process_port: Arc<dyn domain::ports::ProcessPort> =
                    Arc::new(infrastructure::processes::Win32ProcessAdapter);
                let session_repo: Arc<dyn domain::ports::SessionRepository> = Arc::new(
                    infrastructure::database::sessions::SqliteSessionRepository::new(
                        app.state::<sqlx::SqlitePool>().inner().clone(),
                    ),
                );
                let session_service = Arc::new(application::session::SessionService::new(
                    process_port,
                    session_repo,
                ));
                let recovered =
                    tauri::async_runtime::block_on(session_service.recover_on_startup())
                        .unwrap_or(0);
                if recovered > 0 {
                    info!(
                        recovered,
                        "marked interrupted sessions as recovery_required"
                    );
                }
                app.manage(ipc::session::SessionServiceState(session_service.clone()));
                info!("process supervisor ready");

                let pool = app.state::<sqlx::SqlitePool>().inner().clone();
                let catalog = Arc::new(
                    infrastructure::database::catalog::SqliteCatalogRepository::new(pool.clone()),
                );
                let games: Arc<dyn domain::ports::GameRepository> = Arc::new(
                    infrastructure::database::games::SqliteGameRepository::new(pool.clone()),
                );
                let profiles: Arc<dyn domain::ports::ProfileRepository> = catalog.clone();
                let scan_source: Arc<dyn domain::ports::LocalLibrarySource> =
                    Arc::new(infrastructure::providers::SteamProvider::default());
                let scan_catalog: Arc<dyn domain::ports::SteamCatalogRepository> = catalog.clone();
                let artwork_repository: Arc<dyn domain::ports::ArtworkRepository> = Arc::new(
                    infrastructure::database::artwork::SqliteArtworkRepository::new(pool.clone()),
                );
                let label_repository: Arc<dyn domain::ports::LabelProjectRepository> = Arc::new(
                    infrastructure::database::labels::SqliteLabelProjectRepository::new(
                        pool.clone(),
                    ),
                );
                let labels = Arc::new(application::labels::LabelProjectService::new(
                    label_repository,
                    Arc::new(domain::clock::SystemClock),
                ));
                let label_exports = Arc::new(application::label_export::LabelExportService::new(
                    app.state::<support::dirs::AppDirs>().exports.clone(),
                    app.state::<support::dirs::AppDirs>().labels.clone(),
                )?);
                let artwork_store: Arc<dyn domain::ports::ArtworkBlobStore> =
                    Arc::new(infrastructure::artwork::LocalArtworkCache::new(
                        app.state::<support::dirs::AppDirs>().artwork.clone(),
                    )?);
                let artwork = Arc::new(application::artwork::ArtworkService::new(
                    artwork_repository,
                    artwork_store,
                    Arc::new(domain::clock::SystemClock),
                ));
                let media_repository: Arc<dyn domain::ports::MediaRepository> = catalog.clone();
                let activations: Arc<dyn domain::ports::ActivationRepository> = Arc::new(
                    infrastructure::database::activations::SqliteActivationRepository::new(
                        pool.clone(),
                    ),
                );
                let creator = Arc::new(application::media_creator::MediaCreatorService::new(
                    application::media_creator::MediaCreatorRepositories {
                        games: games.clone(),
                        profiles: profiles.clone(),
                        devices: repository.clone(),
                        media: media_repository,
                        activations: activations.clone(),
                    },
                    device_port.clone(),
                    Some(artwork.clone()),
                    Arc::new(domain::clock::SystemClock),
                ));
                let collection = Arc::new(application::collection::GameCollectionService::new(
                    activations,
                    games.clone(),
                    profiles.clone(),
                    artwork.clone(),
                ));
                let optical = Arc::new(application::optical_media::OpticalMediaService::new(
                    creator.clone(),
                    artwork.clone(),
                    repository.clone(),
                    device_port.clone(),
                    Arc::new(infrastructure::media::optical::ImapiOpticalAdapter),
                    app.state::<support::dirs::AppDirs>().exports.clone(),
                )?);
                app.manage(ipc::media::MediaCreatorState(creator));
                app.manage(ipc::media::OpticalMediaState(optical));
                app.manage(ipc::collection::CollectionState(collection));
                app.manage(ipc::labels::LabelProjectState {
                    projects: labels,
                    artwork: artwork.clone(),
                    exports: label_exports,
                });
                app.manage(ipc::runtime::RuntimeArtworkState(artwork.clone()));
                app.manage(ipc::library::LibraryState {
                    scan: Arc::new(application::library::LibraryScanService::new(
                        scan_source,
                        scan_catalog,
                    )),
                    library: Arc::new(application::library::LibraryService::new(
                        games,
                        profiles.clone(),
                        catalog,
                    )),
                    artwork,
                });

                let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
                let source = infrastructure::devices::Win32DeviceEventSource::start(sender)?;
                let sink: Arc<dyn application::devices::DeviceEventSink> =
                    Arc::new(ipc::devices::TauriDeviceEventSink::new(
                        app.handle().clone(),
                        runtime,
                        profiles,
                        settings.clone(),
                        session_service,
                    ));
                let monitor = application::devices::DeviceMonitor::new(
                    device_port,
                    repository,
                    settings,
                    sink,
                    application::devices::DeviceMonitorConfig::default(),
                );
                let monitor = tauri::async_runtime::spawn(monitor.run(receiver));
                app.manage(DeviceRuntimeState {
                    monitor,
                    _source: source,
                });
                info!("device watcher ready");
                info!("administrative library ready");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run MediaDeck");
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    #[test]
    fn product_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "media-deck");
    }

    #[tokio::test]
    async fn offline_startup_initializes_local_storage() {
        let directory = tempdir().expect("tempdir");
        let app_dirs =
            crate::support::dirs::AppDirs::from_root(directory.path()).expect("app dirs");
        let pool = super::initialize_storage(&app_dirs)
            .await
            .expect("offline storage initialization");

        let settings_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .expect("settings query");
        assert!(settings_count >= 10);
    }
}
