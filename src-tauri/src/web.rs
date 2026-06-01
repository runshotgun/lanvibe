use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Result};
use axum::{
    routing::{get, patch, post},
    Router,
};
use tauri::AppHandle;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, services::ServeDir};

use crate::{
    api,
    app_state::{ApiState, AppState},
};

pub async fn run(app: AppHandle, state: Arc<AppState>) -> Result<()> {
    let settings = state.current_settings().await;
    let (listener, port) =
        bind_with_fallback(&settings.dashboard_bind, settings.dashboard_port).await?;
    *state.dashboard_port.write().await = port;

    let router = Router::new()
        .route("/api/devices", get(api::http_list_devices))
        .route("/api/devices/refresh", post(api::http_refresh_devices))
        .route("/api/devices/{id}", patch(api::http_update_device))
        .route("/api/services", get(api::http_list_services))
        .route(
            "/api/favorites",
            get(api::http_list_favorites).patch(api::http_set_favorite),
        )
        .route("/api/favicon", get(api::http_get_favicon))
        .route("/api/scan", post(api::http_scan))
        .route("/api/scan/status", get(api::http_scan_status))
        .route(
            "/api/settings",
            get(api::http_get_settings).patch(api::http_update_settings),
        )
        .fallback_service(ServeDir::new(dist_dir()).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(ApiState { app, state });

    axum::serve(listener, router).await?;
    Ok(())
}

async fn bind_with_fallback(bind: &str, preferred_port: u16) -> Result<(TcpListener, u16)> {
    for offset in 0..50u16 {
        let Some(port) = preferred_port.checked_add(offset) else {
            break;
        };
        let addr: SocketAddr = format!("{bind}:{port}")
            .parse()
            .map_err(|error| anyhow!("invalid dashboard bind address {bind}:{port}: {error}"))?;
        if let Ok(listener) = TcpListener::bind(addr).await {
            return Ok((listener, port));
        }
    }

    Err(anyhow!(
        "could not bind dashboard server on {bind}:{preferred_port} or the next 49 ports"
    ))
}

fn dist_dir() -> PathBuf {
    let manifest_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
    if manifest_dist.exists() {
        return manifest_dist;
    }

    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("dist")))
        .unwrap_or_else(|| PathBuf::from("dist"))
}
