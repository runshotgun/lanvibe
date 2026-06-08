use axum::{
    extract::{connect_info::ConnectInfo, Path, Query, State as AxumState},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
#[cfg(target_os = "windows")]
use std::path::Path as StdPath;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tauri::{AppHandle, Emitter, State as TauriState};
use tauri_plugin_updater::UpdaterExt;

use crate::{
    app_state::{can_http_client_open_loopback_services, ApiState, SharedState},
    db, discovery,
    models::{
        Device, DevicePatch, DiscoveryStatusView, FavoriteOrderPatch, FavoritePatch,
        KillProcessResult, ScanResult, ScanStatusView, Service, Settings, SettingsView,
        UpdateStatusView,
    },
    scanner, startup, tray,
};

#[tauri::command]
pub async fn list_devices(state: TauriState<'_, SharedState>) -> Result<Vec<Device>, String> {
    db::list_devices(&state.pool).await.map_err(to_string)
}

#[tauri::command]
pub async fn refresh_devices(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
) -> Result<Vec<Device>, String> {
    discovery::discover_once(state.inner().clone(), Some(app))
        .await
        .map_err(to_string)?;
    db::list_devices(&state.pool).await.map_err(to_string)
}

#[tauri::command]
pub async fn update_device(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
    id: String,
    selected: bool,
    ignored: bool,
    name_override: Option<String>,
) -> Result<Device, String> {
    let previous = db::get_device(&state.pool, &id).await.map_err(to_string)?;
    let device = db::update_device_flags(&state.pool, &id, selected, ignored, name_override)
        .await
        .map_err(to_string)?;
    refresh_services_after_device_flag_change(
        app.clone(),
        state.inner().clone(),
        &previous,
        &device,
    )
    .await?;
    let _ = tray::refresh(&app).await;
    Ok(device)
}

#[tauri::command]
pub async fn list_services(state: TauriState<'_, SharedState>) -> Result<Vec<Service>, String> {
    let settings = state.current_settings().await;
    db::list_retained_services(&state.pool, settings.retention_days)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub async fn list_favorites(state: TauriState<'_, SharedState>) -> Result<Vec<String>, String> {
    db::list_favorite_keys(&state.pool).await.map_err(to_string)
}

#[tauri::command]
pub async fn set_favorite(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
    service_key: String,
    favorite: bool,
) -> Result<Vec<String>, String> {
    let favorites = db::set_favorite(&state.pool, &service_key, favorite)
        .await
        .map_err(to_string)?;
    let _ = tray::refresh(&app).await;
    let _ = app.emit("favorites-updated", favorites.clone());
    Ok(favorites)
}

#[tauri::command]
pub async fn reorder_favorites(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
    service_keys: Vec<String>,
) -> Result<Vec<String>, String> {
    let favorites = db::reorder_favorites(&state.pool, &service_keys)
        .await
        .map_err(to_string)?;
    let _ = tray::refresh(&app).await;
    let _ = app.emit("favorites-updated", favorites.clone());
    Ok(favorites)
}

#[tauri::command]
pub async fn start_manual_scan(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
) -> Result<ScanResult, String> {
    scanner::scan_selected_devices(state.inner().clone(), Some(app))
        .await
        .map_err(to_string)
}

#[tauri::command]
pub async fn kill_service_process(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
    service_id: i64,
) -> Result<KillProcessResult, String> {
    kill_service_process_by_id(app, state.inner().clone(), service_id)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub async fn get_scan_status(state: TauriState<'_, SharedState>) -> Result<ScanStatusView, String> {
    Ok(state.scan_status.read().await.clone())
}

#[tauri::command]
pub async fn get_discovery_status(
    state: TauriState<'_, SharedState>,
) -> Result<DiscoveryStatusView, String> {
    Ok(state.discovery_status.read().await.clone())
}

#[tauri::command]
pub async fn get_update_status(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
) -> Result<UpdateStatusView, String> {
    Ok(current_update_status(&app, state.inner().clone()).await)
}

#[tauri::command]
pub async fn trigger_host_update(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
) -> Result<UpdateStatusView, String> {
    start_update_task(app, state.inner().clone()).await
}

#[tauri::command]
pub async fn get_settings_view(state: TauriState<'_, SharedState>) -> Result<SettingsView, String> {
    Ok(state.settings_view().await)
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: TauriState<'_, SharedState>,
    settings: Settings,
) -> Result<SettingsView, String> {
    save_settings(app.clone(), state.inner().clone(), settings).await?;
    let _ = tray::refresh(&app).await;
    Ok(state.settings_view().await)
}

#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(url).map_err(to_string)
}

#[tauri::command]
pub fn close_popover(app: AppHandle) {
    crate::tray::close_popover(&app);
}

#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    crate::tray::show_main_window(&app);
    crate::tray::close_popover(&app);
}

#[tauri::command]
pub fn resize_popover(app: AppHandle, favorite_count: usize, loading: bool) {
    crate::tray::resize_popover(&app, favorite_count, loading);
}

#[tauri::command]
pub fn resize_popover_to_content_height(app: AppHandle, height: u32) {
    crate::tray::resize_popover_to_content_height(&app, height);
}

#[tauri::command]
pub async fn get_favicon(
    state: TauriState<'_, SharedState>,
    origin: String,
) -> Result<Option<String>, String> {
    let timeout = state.current_settings().await.http_timeout_ms;
    Ok(state.favicons.get(&origin, timeout).await)
}

#[derive(Debug, Deserialize)]
pub struct FaviconQuery {
    pub origin: String,
}

pub async fn http_list_devices(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<Vec<Device>>, String> {
    db::list_devices(&api.state.pool)
        .await
        .map(Json)
        .map_err(to_string)
}

pub async fn http_refresh_devices(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<Vec<Device>>, String> {
    discovery::discover_once(api.state.clone(), Some(api.app.clone()))
        .await
        .map_err(to_string)?;
    db::list_devices(&api.state.pool)
        .await
        .map(Json)
        .map_err(to_string)
}

pub async fn http_update_device(
    AxumState(api): AxumState<ApiState>,
    Path(id): Path<String>,
    Json(patch): Json<DevicePatch>,
) -> Result<Json<Device>, String> {
    let previous = db::get_device(&api.state.pool, &id)
        .await
        .map_err(to_string)?;
    let device = db::update_device_flags(
        &api.state.pool,
        &id,
        patch.selected,
        patch.ignored,
        patch.name_override,
    )
    .await
    .map_err(to_string)?;
    refresh_services_after_device_flag_change(
        api.app.clone(),
        api.state.clone(),
        &previous,
        &device,
    )
    .await?;
    let _ = tray::refresh(&api.app).await;
    Ok(Json(device))
}

pub async fn http_list_services(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<Vec<Service>>, String> {
    let settings = api.state.current_settings().await;
    db::list_retained_services(&api.state.pool, settings.retention_days)
        .await
        .map(Json)
        .map_err(to_string)
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub message: String,
}

type ApiJsonError = (StatusCode, Json<ApiError>);

pub async fn http_kill_service_process(
    AxumState(api): AxumState<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<Json<KillProcessResult>, ApiJsonError> {
    if !can_http_client_open_loopback_services(peer.ip()) {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Only the host machine can kill local service processes",
        ));
    }

    kill_service_process_by_id(api.app, api.state, id)
        .await
        .map(Json)
        .map_err(|error| json_error(StatusCode::BAD_REQUEST, error.to_string()))
}

pub async fn http_list_favorites(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<Vec<String>>, String> {
    db::list_favorite_keys(&api.state.pool)
        .await
        .map(Json)
        .map_err(to_string)
}

pub async fn http_set_favorite(
    AxumState(api): AxumState<ApiState>,
    Json(patch): Json<FavoritePatch>,
) -> Result<Json<Vec<String>>, String> {
    let favorites = db::set_favorite(&api.state.pool, &patch.service_key, patch.favorite)
        .await
        .map_err(to_string)?;
    let _ = tray::refresh(&api.app).await;
    let _ = api.app.emit("favorites-updated", favorites.clone());
    Ok(Json(favorites))
}

pub async fn http_reorder_favorites(
    AxumState(api): AxumState<ApiState>,
    Json(patch): Json<FavoriteOrderPatch>,
) -> Result<Json<Vec<String>>, String> {
    let favorites = db::reorder_favorites(&api.state.pool, &patch.service_keys)
        .await
        .map_err(to_string)?;
    let _ = tray::refresh(&api.app).await;
    let _ = api.app.emit("favorites-updated", favorites.clone());
    Ok(Json(favorites))
}

pub async fn http_get_favicon(
    AxumState(api): AxumState<ApiState>,
    Query(query): Query<FaviconQuery>,
) -> Json<Option<String>> {
    let timeout = api.state.current_settings().await.http_timeout_ms;
    Json(api.state.favicons.get(&query.origin, timeout).await)
}

pub async fn http_scan(AxumState(api): AxumState<ApiState>) -> Result<Json<ScanResult>, String> {
    scanner::scan_selected_devices(api.state, Some(api.app))
        .await
        .map(Json)
        .map_err(to_string)
}

pub async fn http_scan_status(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<ScanStatusView>, String> {
    Ok(Json(api.state.scan_status.read().await.clone()))
}

pub async fn http_discovery_status(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<DiscoveryStatusView>, String> {
    Ok(Json(api.state.discovery_status.read().await.clone()))
}

pub async fn http_get_update_status(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<UpdateStatusView>, String> {
    Ok(Json(current_update_status(&api.app, api.state).await))
}

pub async fn http_trigger_host_update(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<UpdateStatusView>, String> {
    start_update_task(api.app, api.state).await.map(Json)
}

pub async fn http_get_settings(
    AxumState(api): AxumState<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Result<Json<SettingsView>, String> {
    Ok(Json(
        api.state
            .settings_view_with_loopback_access(can_http_client_open_loopback_services(peer.ip()))
            .await,
    ))
}

pub async fn http_update_settings(
    AxumState(api): AxumState<ApiState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(settings): Json<Settings>,
) -> Result<Json<SettingsView>, String> {
    save_settings(api.app.clone(), api.state.clone(), settings).await?;
    let _ = tray::refresh(&api.app).await;
    Ok(Json(
        api.state
            .settings_view_with_loopback_access(can_http_client_open_loopback_services(peer.ip()))
            .await,
    ))
}

async fn kill_service_process_by_id(
    app: AppHandle,
    state: SharedState,
    service_id: i64,
) -> Result<KillProcessResult, String> {
    let service = db::get_service_by_id(&state.pool, service_id)
        .await
        .map_err(to_string)?;
    let own_dashboard_port = *state.dashboard_port.read().await;
    let result = scanner::kill_local_service_process(&service, own_dashboard_port)
        .await
        .map_err(to_string)?;
    db::mark_service_inactive(
        &state.pool,
        &service.device_id,
        service.port,
        "Process terminated by LANVibe",
    )
    .await
    .map_err(to_string)?;
    let _ = app.emit(
        "services-updated",
        ScanResult {
            scanned_devices: 0,
            discovered_services: 0,
        },
    );
    Ok(result)
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn json_error(status: StatusCode, message: impl Into<String>) -> ApiJsonError {
    (
        status,
        Json(ApiError {
            message: message.into(),
        }),
    )
}

async fn current_update_status(app: &AppHandle, state: SharedState) -> UpdateStatusView {
    let mut status = state.update_status.read().await.clone();
    status.current_version = app.package_info().version.to_string();
    status
}

async fn start_update_task(app: AppHandle, state: SharedState) -> Result<UpdateStatusView, String> {
    if state
        .update_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok(current_update_status(&app, state).await);
    }

    let started_at = Utc::now().to_rfc3339();
    publish_update_status(
        &app,
        state.clone(),
        UpdateStatusView {
            phase: "checking".to_string(),
            current_version: app.package_info().version.to_string(),
            latest_version: None,
            downloaded_bytes: 0,
            total_bytes: None,
            message: "Checking GitHub Releases for a signed update...".to_string(),
            started_at: Some(started_at),
            finished_at: None,
        },
    )
    .await;

    let app_for_task = app.clone();
    let state_for_task = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_update_task(app_for_task.clone(), state_for_task.clone()).await {
            let previous = state_for_task.update_status.read().await.clone();
            publish_update_status(
                &app_for_task,
                state_for_task.clone(),
                UpdateStatusView {
                    phase: "error".to_string(),
                    current_version: app_for_task.package_info().version.to_string(),
                    latest_version: previous.latest_version,
                    downloaded_bytes: 0,
                    total_bytes: None,
                    message: error,
                    started_at: previous.started_at,
                    finished_at: Some(Utc::now().to_rfc3339()),
                },
            )
            .await;
            state_for_task
                .update_running
                .store(false, Ordering::Release);
        }
    });

    Ok(current_update_status(&app, state).await)
}

async fn run_update_task(app: AppHandle, state: SharedState) -> Result<(), String> {
    let updater = app.updater().map_err(to_string)?;
    let update = updater.check().await.map_err(to_string)?;

    let Some(update) = update else {
        let previous = state.update_status.read().await.clone();
        publish_update_status(
            &app,
            state.clone(),
            UpdateStatusView {
                phase: "current".to_string(),
                current_version: app.package_info().version.to_string(),
                latest_version: None,
                downloaded_bytes: 0,
                total_bytes: None,
                message: "LANVibe is up to date.".to_string(),
                started_at: previous.started_at,
                finished_at: Some(Utc::now().to_rfc3339()),
            },
        )
        .await;
        state.update_running.store(false, Ordering::Release);
        return Ok(());
    };

    let current_version = update.current_version.clone();
    let latest_version = update.version.clone();
    let previous = state.update_status.read().await.clone();

    publish_update_status(
        &app,
        state.clone(),
        UpdateStatusView {
            phase: "downloading".to_string(),
            current_version: current_version.clone(),
            latest_version: Some(latest_version.clone()),
            downloaded_bytes: 0,
            total_bytes: None,
            message: format!("Downloading LANVibe {latest_version}..."),
            started_at: previous.started_at,
            finished_at: None,
        },
    )
    .await;

    let downloaded = Arc::new(AtomicU64::new(0));
    let progress_state = state.clone();
    let progress_app = app.clone();
    let progress_current = current_version.clone();
    let progress_latest = latest_version.clone();
    let progress_started = state.update_status.read().await.started_at.clone();
    let progress_downloaded = downloaded.clone();

    let update_bytes = update
        .download(
            move |chunk_len, total_bytes| {
                let downloaded_bytes = progress_downloaded
                    .fetch_add(chunk_len as u64, Ordering::AcqRel)
                    + chunk_len as u64;
                let state = progress_state.clone();
                let app = progress_app.clone();
                let current_version = progress_current.clone();
                let latest_version = progress_latest.clone();
                let started_at = progress_started.clone();
                tauri::async_runtime::spawn(async move {
                    publish_update_status(
                        &app,
                        state,
                        UpdateStatusView {
                            phase: "downloading".to_string(),
                            current_version,
                            latest_version: Some(latest_version),
                            downloaded_bytes,
                            total_bytes,
                            message: "Downloading update...".to_string(),
                            started_at,
                            finished_at: None,
                        },
                    )
                    .await;
                });
            },
            {
                let state = state.clone();
                let app = app.clone();
                let current_version = current_version.clone();
                let latest_version = latest_version.clone();
                move || {
                    tauri::async_runtime::spawn(async move {
                        let started_at = state.update_status.read().await.started_at.clone();
                        publish_update_status(
                            &app,
                            state,
                            UpdateStatusView {
                                phase: "installing".to_string(),
                                current_version,
                                latest_version: Some(latest_version),
                                downloaded_bytes: 0,
                                total_bytes: None,
                                message: "Installing update...".to_string(),
                                started_at,
                                finished_at: None,
                            },
                        )
                        .await;
                    });
                }
            },
        )
        .await
        .map_err(to_string)?;

    #[cfg(target_os = "windows")]
    if windows_program_files_install_requires_elevation() {
        // Program Files MSI installs need UAC; Tauri's default "open" launch
        // exits into a quiet, non-elevated msiexec failure.
        install_windows_msi_update_elevated(&update, &update_bytes)?;
    }

    update.install(update_bytes).map_err(to_string)?;

    let previous = state.update_status.read().await.clone();
    publish_update_status(
        &app,
        state.clone(),
        UpdateStatusView {
            phase: "restarting".to_string(),
            current_version,
            latest_version: Some(latest_version),
            downloaded_bytes: previous.downloaded_bytes,
            total_bytes: previous.total_bytes,
            message: "Update installed. Restarting LANVibe...".to_string(),
            started_at: previous.started_at,
            finished_at: Some(Utc::now().to_rfc3339()),
        },
    )
    .await;

    app.request_restart();
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_program_files_install_requires_elevation() -> bool {
    !windows_process_is_elevated()
        && std::env::current_exe()
            .map(|path| windows_path_is_under_program_files(&path))
            .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn install_windows_msi_update_elevated(
    update: &tauri_plugin_updater::Update,
    bytes: &[u8],
) -> Result<(), String> {
    if !update
        .download_url
        .path()
        .to_ascii_lowercase()
        .ends_with(".msi")
    {
        return Err(
            "LANVibe is installed in Program Files, so Windows needs Administrator approval for this update. Download the latest installer from GitHub and run it as Administrator."
                .to_string(),
        );
    }

    let installer_path = persist_windows_update_installer(&update.version, bytes)?;
    shell_execute_windows_msi_update_elevated(&installer_path)?;
    std::process::exit(0);
}

#[cfg(target_os = "windows")]
fn persist_windows_update_installer(
    version: &str,
    bytes: &[u8],
) -> Result<std::path::PathBuf, String> {
    let safe_version = version
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "LANVibe_{safe_version}_{}_x64_en-US.msi",
        std::process::id()
    ));
    std::fs::write(&path, bytes).map_err(to_string)?;
    Ok(path)
}

#[cfg(target_os = "windows")]
fn shell_execute_windows_msi_update_elevated(installer_path: &StdPath) -> Result<(), String> {
    use std::ffi::{OsStr, OsString};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOW};

    let msiexec = std::env::var_os("SYSTEMROOT").map_or_else(
        || OsString::from("msiexec.exe"),
        |root| {
            let mut path = std::path::PathBuf::from(root);
            path.push("System32");
            path.push("msiexec.exe");
            path.into_os_string()
        },
    );
    let parameters = OsString::from(format!(
        "/i \"{}\" /quiet /promptrestart AUTOLAUNCHAPP=True",
        installer_path.display()
    ));
    let verb = encode_windows_wide(OsStr::new("runas"));
    let file = encode_windows_wide(&msiexec);
    let parameters = encode_windows_wide(&parameters);

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            std::ptr::null(),
            SW_SHOW,
        )
    };

    if result as isize <= 32 {
        return Err(format!(
            "Windows did not allow the elevated installer to start. ShellExecuteW returned {result:?}."
        ));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn encode_windows_wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn windows_process_is_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned_size = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned_size,
        ) != 0;

        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(target_os = "windows")]
fn windows_path_is_under_program_files(path: &StdPath) -> bool {
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .any(|root| windows_path_starts_with(path, &root))
}

#[cfg(target_os = "windows")]
fn windows_path_starts_with(path: &StdPath, root: &StdPath) -> bool {
    let path = windows_normalized_path(path);
    let root = windows_normalized_path(root);
    path == root || path.starts_with(&(root + "/"))
}

#[cfg(target_os = "windows")]
fn windows_normalized_path(path: &StdPath) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

async fn publish_update_status(app: &AppHandle, state: SharedState, status: UpdateStatusView) {
    *state.update_status.write().await = status.clone();
    let _ = app.emit("update-status", status);
}

async fn save_settings(
    app: AppHandle,
    state: SharedState,
    settings: Settings,
) -> Result<Settings, String> {
    let normalized = settings.normalized();
    startup::apply_launch_at_startup(&app, normalized.launch_at_startup).map_err(to_string)?;
    state.save_settings(normalized).await.map_err(to_string)
}

async fn refresh_services_after_device_flag_change(
    app: AppHandle,
    state: SharedState,
    previous: &Device,
    device: &Device,
) -> Result<(), String> {
    if previous.selected == device.selected && previous.ignored == device.ignored {
        return Ok(());
    }

    if device.selected && !device.ignored {
        let app_for_scan = app.clone();
        let state_for_scan = state.clone();
        let device_id = device.id.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = scanner::refresh_device_services(
                state_for_scan,
                Some(app_for_scan.clone()),
                device_id,
            )
            .await
            {
                let _ = app_for_scan.emit("scan-error", error.to_string());
            }
        });
        return Ok(());
    }

    scanner::refresh_device_services(state, Some(app), device.id.clone())
        .await
        .map(|_| ())
        .map_err(to_string)
}
