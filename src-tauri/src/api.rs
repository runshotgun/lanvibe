use axum::{
    extract::{Path, Query, State as AxumState},
    Json,
};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State as TauriState};

use crate::{
    app_state::{ApiState, SharedState},
    db, discovery,
    models::{
        Device, DevicePatch, FavoritePatch, ScanResult, ScanStatusView, Service, Settings,
        SettingsView,
    },
    scanner, startup, tray,
};

#[tauri::command]
pub async fn list_devices(state: TauriState<'_, SharedState>) -> Result<Vec<Device>, String> {
    db::list_devices(&state.pool).await.map_err(to_string)
}

#[tauri::command]
pub async fn refresh_devices(state: TauriState<'_, SharedState>) -> Result<Vec<Device>, String> {
    discovery::discover_once(state.inner().clone(), None)
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
pub async fn get_scan_status(state: TauriState<'_, SharedState>) -> Result<ScanStatusView, String> {
    Ok(state.scan_status.read().await.clone())
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

pub async fn http_get_settings(
    AxumState(api): AxumState<ApiState>,
) -> Result<Json<SettingsView>, String> {
    Ok(Json(api.state.settings_view().await))
}

pub async fn http_update_settings(
    AxumState(api): AxumState<ApiState>,
    Json(settings): Json<Settings>,
) -> Result<Json<SettingsView>, String> {
    save_settings(api.app.clone(), api.state.clone(), settings).await?;
    let _ = tray::refresh(&api.app).await;
    Ok(Json(api.state.settings_view().await))
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
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
