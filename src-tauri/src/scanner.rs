use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Result;
use chrono::Utc;
use futures::{stream, StreamExt};
use regex::Regex;
use reqwest::redirect::Policy;
use tauri::{AppHandle, Emitter};
use tokio::{net::TcpStream, time};

use crate::{
    app_state::SharedState,
    db, discovery,
    models::{Device, ProbeHit, ScanResult, Service, Settings},
    tray,
};

pub fn spawn_loop(app: AppHandle, state: SharedState) {
    tauri::async_runtime::spawn(async move {
        loop {
            let settings = state.current_settings().await;
            time::sleep(Duration::from_secs(settings.scan_interval_seconds)).await;

            let settings = state.current_settings().await;
            if !settings.auto_scan || settings.manual_only {
                continue;
            }

            if let Err(error) = scan_selected_devices(state.clone(), Some(app.clone())).await {
                let _ = app.emit("scan-error", error.to_string());
            }
        }
    });
}

pub fn spawn_favorite_loop(app: AppHandle, state: SharedState) {
    tauri::async_runtime::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if let Err(error) = refresh_favorite_services(state.clone(), Some(app.clone())).await {
                let _ = app.emit("scan-error", error.to_string());
            }
        }
    });
}

pub async fn scan_selected_devices(
    state: SharedState,
    app: Option<AppHandle>,
) -> Result<ScanResult> {
    let settings = state.current_settings().await;
    let devices = db::list_selected_devices(&state.pool).await?;
    let scan_started_at = Utc::now().to_rfc3339();
    let discovered_services = Arc::new(AtomicUsize::new(0));

    {
        let mut status = state.scan_status.write().await;
        status.phase = "scanning".to_string();
        status.selected_devices = devices.len();
        status.scanned_devices = 0;
        status.discovered_services = 0;
        status.current_device_ip = None;
        status.started_at = Some(scan_started_at.clone());
        status.finished_at = None;
    }

    if let Some(app) = &app {
        let _ = app.emit("scan-started", devices.len());
    }

    for device in &devices {
        {
            let mut status = state.scan_status.write().await;
            status.current_device_ip = Some(device.ip.clone());
        }
        scan_device(
            state.clone(),
            device.clone(),
            settings.clone(),
            discovered_services.clone(),
        )
        .await?;
        db::mark_missing_services_inactive(&state.pool, &device.id, &scan_started_at).await?;
        {
            let mut status = state.scan_status.write().await;
            status.scanned_devices += 1;
            status.discovered_services = discovered_services.load(Ordering::Relaxed);
        }
    }

    let total = discovered_services.load(Ordering::Relaxed);
    db::insert_scan_history(&state.pool, &scan_started_at, devices.len(), total).await?;

    let result = ScanResult {
        scanned_devices: devices.len(),
        discovered_services: total,
    };

    if let Some(app) = &app {
        let _ = app.emit("services-updated", &result);
        let _ = app.emit("scan-finished", &result);
        let _ = tray::refresh(app).await;
    }

    {
        let mut status = state.scan_status.write().await;
        status.phase = "idle".to_string();
        status.current_device_ip = None;
        status.scanned_devices = devices.len();
        status.discovered_services = total;
        status.finished_at = Some(Utc::now().to_rfc3339());
    }

    Ok(result)
}

pub async fn refresh_favorite_services(
    state: SharedState,
    app: Option<AppHandle>,
) -> Result<ScanResult> {
    let settings = state.current_settings().await;
    let favorites = db::list_favorite_services(&state.pool).await?;
    let refreshed_services = Arc::new(AtomicUsize::new(0));
    let own_dashboard_port = *state.dashboard_port.read().await;
    let concurrency = settings.scan_concurrency.clamp(1, 32);

    stream::iter(favorites)
        .for_each_concurrent(concurrency, |service| {
            let state = state.clone();
            let settings = settings.clone();
            let refreshed_services = refreshed_services.clone();
            async move {
                let own_ip = service
                    .ip
                    .parse::<Ipv4Addr>()
                    .map(discovery::is_local_ip)
                    .unwrap_or(false);

                if own_ip && service.port == own_dashboard_port {
                    return;
                }

                match probe_favorite_service(&service, &settings).await {
                    FavoriteProbe::Http(hit) => {
                        if db::upsert_service_for_device(
                            &state.pool,
                            &service.device_id,
                            &service.ip,
                            &hit,
                        )
                        .await
                        .is_ok()
                        {
                            refreshed_services.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    FavoriteProbe::TcpOpen => {
                        if db::mark_service_active(&state.pool, &service.device_id, service.port)
                            .await
                            .is_ok()
                        {
                            refreshed_services.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    FavoriteProbe::Unavailable => {
                        if db::mark_service_inactive(
                            &state.pool,
                            &service.device_id,
                            service.port,
                            "Favorite status check failed",
                        )
                        .await
                        .is_ok()
                        {
                            refreshed_services.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        })
        .await;

    let result = ScanResult {
        scanned_devices: 0,
        discovered_services: refreshed_services.load(Ordering::Relaxed),
    };

    if let Some(app) = &app {
        let _ = app.emit("services-updated", &result);
        let _ = tray::refresh(app).await;
    }

    Ok(result)
}

pub async fn refresh_device_services(
    state: SharedState,
    app: Option<AppHandle>,
    device_id: String,
) -> Result<ScanResult> {
    let device = db::get_device(&state.pool, &device_id).await?;

    if !device.selected || device.ignored {
        db::mark_device_services_inactive(&state.pool, &device.id, "Device disabled for scanning")
            .await?;

        let result = ScanResult {
            scanned_devices: 0,
            discovered_services: 0,
        };
        if let Some(app) = &app {
            let _ = app.emit("services-updated", &result);
            let _ = tray::refresh(app).await;
        }
        return Ok(result);
    }

    let settings = state.current_settings().await;
    let scan_started_at = Utc::now().to_rfc3339();
    let discovered_services = Arc::new(AtomicUsize::new(0));

    {
        let mut status = state.scan_status.write().await;
        status.phase = "scanning".to_string();
        status.selected_devices = 1;
        status.scanned_devices = 0;
        status.discovered_services = 0;
        status.current_device_ip = Some(device.ip.clone());
        status.started_at = Some(scan_started_at.clone());
        status.finished_at = None;
    }

    if let Some(app) = &app {
        let _ = app.emit("scan-started", 1);
    }

    let total = scan_device(
        state.clone(),
        device.clone(),
        settings,
        discovered_services.clone(),
    )
    .await?;
    db::mark_missing_services_inactive(&state.pool, &device.id, &scan_started_at).await?;
    db::insert_scan_history(&state.pool, &scan_started_at, 1, total).await?;

    let result = ScanResult {
        scanned_devices: 1,
        discovered_services: total,
    };

    if let Some(app) = &app {
        let _ = app.emit("services-updated", &result);
        let _ = app.emit("scan-finished", &result);
        let _ = tray::refresh(app).await;
    }

    {
        let mut status = state.scan_status.write().await;
        status.phase = "idle".to_string();
        status.current_device_ip = None;
        status.scanned_devices = 1;
        status.discovered_services = total;
        status.finished_at = Some(Utc::now().to_rfc3339());
    }

    Ok(result)
}

enum FavoriteProbe {
    Http(ProbeHit),
    TcpOpen,
    Unavailable,
}

async fn probe_favorite_service(service: &Service, settings: &Settings) -> FavoriteProbe {
    let mut status_settings = settings.clone();
    status_settings.connect_timeout_ms = status_settings.connect_timeout_ms.max(750);
    status_settings.http_timeout_ms = status_settings.http_timeout_ms.max(3_000);

    if let Some(hit) =
        probe_http(&service.ip, service.port, &service.scheme, &status_settings).await
    {
        return FavoriteProbe::Http(hit);
    }

    let alternate_scheme = if service.scheme == "https" {
        "http"
    } else {
        "https"
    };
    if let Some(hit) = probe_http(
        &service.ip,
        service.port,
        alternate_scheme,
        &status_settings,
    )
    .await
    {
        return FavoriteProbe::Http(hit);
    }

    if tcp_open(
        &service.ip,
        service.port,
        status_settings.connect_timeout_ms,
    )
    .await
    {
        return FavoriteProbe::TcpOpen;
    }

    FavoriteProbe::Unavailable
}

async fn scan_device(
    state: SharedState,
    device: Device,
    settings: Settings,
    total: Arc<AtomicUsize>,
) -> Result<usize> {
    let own_dashboard_port = *state.dashboard_port.read().await;
    let own_ip = device
        .ip
        .parse::<Ipv4Addr>()
        .map(discovery::is_local_ip)
        .unwrap_or(false);
    let found = Arc::new(AtomicUsize::new(0));

    stream::iter(1u16..=u16::MAX)
        .for_each_concurrent(settings.scan_concurrency, |port| {
            let state = state.clone();
            let device = device.clone();
            let settings = settings.clone();
            let found = found.clone();
            let total = total.clone();
            async move {
                if own_ip && port == own_dashboard_port {
                    return;
                }

                if let Some(hit) = probe_port(&device.ip, port, &settings).await {
                    if db::upsert_service(&state.pool, &device, &hit).await.is_ok() {
                        found.fetch_add(1, Ordering::Relaxed);
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .await;

    Ok(found.load(Ordering::Relaxed))
}

pub async fn probe_port(ip: &str, port: u16, settings: &Settings) -> Option<ProbeHit> {
    if !tcp_open(ip, port, settings.connect_timeout_ms).await {
        return None;
    }

    let http_hit = probe_http(ip, port, "http", settings).await;
    if http_hit
        .as_ref()
        .and_then(|hit| hit.title.as_deref())
        .is_some_and(|title| !title.trim().is_empty())
    {
        return http_hit;
    }

    let https_hit = probe_http(ip, port, "https", settings).await;
    if https_hit
        .as_ref()
        .and_then(|hit| hit.title.as_deref())
        .is_some_and(|title| !title.trim().is_empty())
    {
        return https_hit;
    }

    http_hit.or(https_hit)
}

async fn tcp_open(ip: &str, port: u16, timeout_ms: u64) -> bool {
    let Ok(ip) = ip.parse::<IpAddr>() else {
        return false;
    };
    let socket = SocketAddr::new(ip, port);
    matches!(
        time::timeout(
            Duration::from_millis(timeout_ms),
            TcpStream::connect(socket)
        )
        .await,
        Ok(Ok(_))
    )
}

async fn probe_http(ip: &str, port: u16, scheme: &str, settings: &Settings) -> Option<ProbeHit> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(Policy::limited(3))
        .timeout(Duration::from_millis(settings.http_timeout_ms))
        .build()
        .ok()?;
    let url = format!("{scheme}://{ip}:{port}/");
    let response = client.get(&url).send().await.ok()?;
    let status_code = Some(i64::from(response.status().as_u16()));
    let server = response
        .headers()
        .get(reqwest::header::SERVER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let body = response.text().await.unwrap_or_default();

    Some(ProbeHit {
        port,
        scheme: scheme.to_string(),
        url,
        title: extract_page_title(&body),
        status_code,
        server,
    })
}

fn extract_page_title(body: &str) -> Option<String> {
    let title = extract_title(body);
    let metadata_title = extract_meta_title(body);

    if title
        .as_deref()
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("login - "))
    {
        return metadata_title.or(title);
    }

    title.or(metadata_title)
}

fn extract_title(body: &str) -> Option<String> {
    let re = Regex::new("(?is)<title[^>]*>(?P<title>.*?)</title>").expect("valid title regex");
    normalize_title(re.captures(body)?.name("title")?.as_str())
}

fn extract_meta_title(body: &str) -> Option<String> {
    let patterns = [
        r#"(?is)<meta[^>]+(?:name|property)=["'](?:application-name|apple-mobile-web-app-title|og:site_name|og:title|description)["'][^>]+content=["'](?P<title>[^"']+)["'][^>]*>"#,
        r#"(?is)<meta[^>]+content=["'](?P<title>[^"']+)["'][^>]+(?:name|property)=["'](?:application-name|apple-mobile-web-app-title|og:site_name|og:title|description)["'][^>]*>"#,
    ];

    patterns.iter().find_map(|pattern| {
        Regex::new(pattern)
            .ok()?
            .captures(body)?
            .name("title")
            .and_then(|value| normalize_title(value.as_str()))
    })
}

fn normalize_title(raw: &str) -> Option<String> {
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        None
    } else {
        Some(title.chars().take(140).collect())
    }
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::{extract_page_title, extract_title, probe_port};
    use crate::models::Settings;

    #[test]
    fn extracts_html_title() {
        assert_eq!(
            extract_title("<html><head><title> Home Assistant </title></head></html>").as_deref(),
            Some("Home Assistant")
        );
    }

    #[test]
    fn empty_title_is_none() {
        assert!(extract_title("<title>   </title>").is_none());
    }

    #[test]
    fn prefers_app_metadata_for_login_titles() {
        assert_eq!(
            extract_page_title(
                r#"<meta name="description" content="Prowlarr" /><title>Login - Prowlarr</title>"#
            )
            .as_deref(),
            Some("Prowlarr")
        );
    }

    #[test]
    fn falls_back_to_metadata_when_title_missing() {
        assert_eq!(
            extract_page_title(r#"<meta property="og:site_name" content="Portainer" />"#)
                .as_deref(),
            Some("Portainer")
        );
    }

    #[tokio::test]
    async fn detects_http_service() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            for _ in 0..4 {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let mut request_buf = [0u8; 1024];
                    let _ = stream.read(&mut request_buf).await;
                    let body = "<html><title>Local Tool</title><body></body></html>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        });

        let hit = probe_port("127.0.0.1", port, &Settings::default())
            .await
            .unwrap();
        assert_eq!(hit.scheme, "http");
        assert_eq!(hit.title.as_deref(), Some("Local Tool"));
    }

    #[tokio::test]
    async fn ignores_non_http_open_port() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            for _ in 0..3 {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let _ = stream.write_all(b"not http").await;
                }
            }
        });

        let settings = Settings {
            http_timeout_ms: 250,
            connect_timeout_ms: 100,
            ..Settings::default()
        };
        assert!(probe_port("127.0.0.1", port, &settings).await.is_none());
    }
}
