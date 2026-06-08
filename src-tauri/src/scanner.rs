use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{bail, Result};
use chrono::Utc;
use futures::{stream, StreamExt};
use regex::Regex;
use reqwest::redirect::Policy;
use tauri::{AppHandle, Emitter};
use tokio::{net::TcpStream, time};

#[cfg(not(windows))]
use std::collections::BTreeSet;

#[cfg(not(windows))]
use tokio::process::Command;

#[cfg(windows)]
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::CloseHandle,
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    },
    Networking::WinSock::AF_INET,
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    },
};

use crate::{
    app_state::SharedState,
    db, discovery,
    models::{Device, KillProcessResult, ProbeHit, ScanResult, Service, Settings},
    tray,
};

const SCAN_PROGRESS_UPDATE_EVERY_PORTS: usize = 512;

struct ScanRunGuard {
    state: SharedState,
}

impl Drop for ScanRunGuard {
    fn drop(&mut self) {
        self.state.scan_running.store(false, Ordering::Release);
    }
}

fn try_acquire_scan(state: &SharedState) -> Option<ScanRunGuard> {
    state
        .scan_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()
        .map(|_| ScanRunGuard {
            state: state.clone(),
        })
}

async fn current_scan_result(state: &SharedState) -> ScanResult {
    let status = state.scan_status.read().await;
    ScanResult {
        scanned_devices: status.scanned_devices,
        discovered_services: status.discovered_services,
    }
}

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
        let mut interval = time::interval(favorite_refresh_interval());
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if let Err(error) = refresh_favorite_services(state.clone(), Some(app.clone())).await {
                let _ = app.emit("scan-error", error.to_string());
            }
        }
    });
}

pub fn favorite_refresh_interval() -> Duration {
    Duration::from_secs(30)
}

pub async fn scan_selected_devices(
    state: SharedState,
    app: Option<AppHandle>,
) -> Result<ScanResult> {
    let Some(_scan_guard) = try_acquire_scan(&state) else {
        return Ok(current_scan_result(&state).await);
    };

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
        status.current_device_scanned_ports = 0;
        status.current_device_total_ports = 0;
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
            status.current_device_scanned_ports = 0;
            status.current_device_total_ports = 0;
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
            status.current_device_scanned_ports = 0;
            status.current_device_total_ports = 0;
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
        status.current_device_scanned_ports = 0;
        status.current_device_total_ports = 0;
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
    let local_listeners = local_listening_ports().await.ok();

    stream::iter(favorites)
        .for_each_concurrent(concurrency, |service| {
            let state = state.clone();
            let settings = settings.clone();
            let local_listeners = local_listeners.clone();
            let refreshed_services = refreshed_services.clone();
            async move {
                let local_service = is_local_service_address(&service.ip);

                if local_service && service.port == own_dashboard_port {
                    return;
                }

                if local_service {
                    let Some(local_listeners) = &local_listeners else {
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
                                if db::mark_service_active(
                                    &state.pool,
                                    &service.device_id,
                                    service.port,
                                )
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
                        return;
                    };

                    let Some(local_target) =
                        local_favorite_probe_target(&service.ip, service.port, local_listeners)
                    else {
                        if db::mark_service_inactive(
                            &state.pool,
                            &service.device_id,
                            service.port,
                            "Local listener is no longer active",
                        )
                        .await
                        .is_ok()
                        {
                            refreshed_services.fetch_add(1, Ordering::Relaxed);
                        }
                        return;
                    };

                    match probe_favorite_service_on_ip(&service, &local_target.probe_ip, &settings)
                        .await
                    {
                        FavoriteProbe::Http(mut hit) => {
                            if local_target.process_owner.is_some() {
                                hit.process_owner = local_target.process_owner.clone();
                            }
                            if db::upsert_service_for_device(
                                &state.pool,
                                &service.device_id,
                                &local_target.probe_ip,
                                &hit,
                            )
                            .await
                            .is_ok()
                            {
                                refreshed_services.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        FavoriteProbe::TcpOpen => {
                            if db::mark_service_active_with_process_owner(
                                &state.pool,
                                &service.device_id,
                                service.port,
                                local_target.process_owner.as_deref(),
                            )
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

    let Some(_scan_guard) = try_acquire_scan(&state) else {
        return Ok(current_scan_result(&state).await);
    };

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
        status.current_device_scanned_ports = 0;
        status.current_device_total_ports = 0;
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
        status.current_device_scanned_ports = 0;
        status.current_device_total_ports = 0;
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
    probe_favorite_service_on_ip(service, &service.ip, settings).await
}

async fn probe_favorite_service_on_ip(
    service: &Service,
    ip: &str,
    settings: &Settings,
) -> FavoriteProbe {
    let mut status_settings = settings.clone();
    status_settings.connect_timeout_ms = status_settings.connect_timeout_ms.max(750);
    status_settings.http_timeout_ms = status_settings.http_timeout_ms.max(3_000);

    if let Some(hit) = probe_http(ip, service.port, &service.scheme, &status_settings).await {
        return FavoriteProbe::Http(hit);
    }

    let alternate_scheme = if service.scheme == "https" {
        "http"
    } else {
        "https"
    };
    if let Some(hit) = probe_http(ip, service.port, alternate_scheme, &status_settings).await {
        return FavoriteProbe::Http(hit);
    }

    if tcp_open(ip, service.port, status_settings.connect_timeout_ms).await {
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

    if own_ip {
        if let Ok(listeners) = local_listening_ports().await {
            let targets = local_scan_targets(&device.ip, &listeners, own_dashboard_port);
            let target_total = targets.len();
            scan_probe_targets(
                state,
                device.id,
                targets,
                target_total,
                settings,
                found.clone(),
                total,
                true,
            )
            .await;
            return Ok(found.load(Ordering::Relaxed));
        }
    }

    let target_total = if own_ip {
        usize::from(u16::MAX) - 1
    } else {
        usize::from(u16::MAX)
    };
    let targets = (1u16..=u16::MAX)
        .filter(move |port| !own_ip || *port != own_dashboard_port)
        .map(|port| ProbeTarget {
            port,
            probe_ip: device.ip.clone(),
            service_ip: device.ip.clone(),
            process_owner: None,
        });
    scan_probe_targets(
        state,
        device.id,
        targets,
        target_total,
        settings,
        found.clone(),
        total,
        false,
    )
    .await;

    Ok(found.load(Ordering::Relaxed))
}

async fn scan_probe_targets(
    state: SharedState,
    device_id: String,
    targets: impl IntoIterator<Item = ProbeTarget>,
    target_total: usize,
    settings: Settings,
    found: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    known_open: bool,
) {
    publish_port_progress(&state, 0, target_total).await;
    let completed = Arc::new(AtomicUsize::new(0));

    stream::iter(targets)
        .for_each_concurrent(settings.scan_concurrency, |target| {
            let state = state.clone();
            let device_id = device_id.clone();
            let settings = settings.clone();
            let found = found.clone();
            let total = total.clone();
            let completed = completed.clone();
            async move {
                let hit = if known_open {
                    probe_web_on_open_port(&target.probe_ip, target.port, &settings).await
                } else {
                    probe_port(&target.probe_ip, target.port, &settings).await
                };

                if let Some(mut hit) = hit {
                    if target.process_owner.is_some() {
                        hit.process_owner = target.process_owner.clone();
                    }
                    if db::upsert_service_for_device(
                        &state.pool,
                        &device_id,
                        &target.service_ip,
                        &hit,
                    )
                    .await
                    .is_ok()
                    {
                        found.fetch_add(1, Ordering::Relaxed);
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                }

                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if done == target_total || done % SCAN_PROGRESS_UPDATE_EVERY_PORTS == 0 {
                    publish_port_progress(&state, done, target_total).await;
                }
            }
        })
        .await;

    publish_port_progress(&state, target_total, target_total).await;
}

async fn publish_port_progress(state: &SharedState, scanned_ports: usize, total_ports: usize) {
    let mut status = state.scan_status.write().await;
    status.current_device_scanned_ports = scanned_ports;
    status.current_device_total_ports = total_ports;
}

pub async fn probe_port(ip: &str, port: u16, settings: &Settings) -> Option<ProbeHit> {
    if !tcp_open(ip, port, settings.connect_timeout_ms).await {
        return None;
    }

    probe_web_on_open_port(ip, port, settings).await
}

async fn probe_web_on_open_port(ip: &str, port: u16, settings: &Settings) -> Option<ProbeHit> {
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocalTcpListener {
    addr: Ipv4Addr,
    port: u16,
    pid: Option<u32>,
    process_owner: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProbeTarget {
    port: u16,
    probe_ip: String,
    service_ip: String,
    process_owner: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocalFavoriteProbeTarget {
    probe_ip: String,
    pid: Option<u32>,
    process_owner: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessKillTarget {
    pid: u32,
    process_owner: Option<String>,
}

fn local_scan_targets(
    device_ip: &str,
    listeners: &[LocalTcpListener],
    own_dashboard_port: u16,
) -> Vec<ProbeTarget> {
    let device_ip = device_ip.parse::<Ipv4Addr>().ok();
    let mut targets = BTreeMap::<u16, ProbeTarget>::new();

    for listener in listeners {
        if listener.port == own_dashboard_port
            || !local_listener_matches_device(listener.addr, device_ip)
        {
            continue;
        }

        let probe_ip = local_listener_probe_ip(listener.addr, device_ip);
        let target = ProbeTarget {
            port: listener.port,
            service_ip: probe_ip.clone(),
            probe_ip,
            process_owner: listener.process_owner.clone(),
        };

        targets
            .entry(listener.port)
            .and_modify(|existing| {
                if existing.probe_ip == Ipv4Addr::LOCALHOST.to_string()
                    && target.probe_ip != Ipv4Addr::LOCALHOST.to_string()
                {
                    *existing = target.clone();
                }
            })
            .or_insert(target);
    }

    targets.into_values().collect()
}

fn local_listener_matches_device(listener_addr: Ipv4Addr, device_ip: Option<Ipv4Addr>) -> bool {
    listener_addr.is_unspecified()
        || listener_addr.is_loopback()
        || device_ip.is_some_and(|ip| listener_addr == ip)
}

fn local_listener_probe_ip(listener_addr: Ipv4Addr, device_ip: Option<Ipv4Addr>) -> String {
    if listener_addr.is_loopback() {
        Ipv4Addr::LOCALHOST.to_string()
    } else if listener_addr.is_unspecified() {
        device_ip.unwrap_or(Ipv4Addr::LOCALHOST).to_string()
    } else {
        listener_addr.to_string()
    }
}

fn local_favorite_probe_ip(
    service_ip: &str,
    port: u16,
    listeners: &[LocalTcpListener],
) -> Option<String> {
    local_favorite_probe_target(service_ip, port, listeners).map(|target| target.probe_ip)
}

fn local_favorite_probe_target(
    service_ip: &str,
    port: u16,
    listeners: &[LocalTcpListener],
) -> Option<LocalFavoriteProbeTarget> {
    let parsed_service_ip = service_ip.parse::<Ipv4Addr>().ok();
    listeners
        .iter()
        .filter(|listener| {
            listener.port == port
                && local_favorite_listener_matches_service(
                    listener.addr,
                    service_ip,
                    parsed_service_ip,
                )
        })
        .map(|listener| {
            let probe_ip = if listener.addr.is_loopback()
                || service_ip.eq_ignore_ascii_case("localhost")
                || parsed_service_ip.is_some_and(|ip| ip.is_loopback())
            {
                Ipv4Addr::LOCALHOST.to_string()
            } else if listener.addr.is_unspecified() {
                parsed_service_ip.unwrap_or(Ipv4Addr::LOCALHOST).to_string()
            } else {
                listener.addr.to_string()
            };
            LocalFavoriteProbeTarget {
                probe_ip,
                pid: listener.pid,
                process_owner: listener.process_owner.clone(),
            }
        })
        .next()
}

fn local_process_kill_target(
    service: &Service,
    own_dashboard_port: u16,
    listeners: &[LocalTcpListener],
) -> Result<ProcessKillTarget> {
    if !service.active {
        bail!("Service is inactive");
    }

    if service.port == own_dashboard_port {
        bail!("Refusing to kill the LANVibe dashboard process");
    }

    if !is_local_service_address(&service.ip) {
        bail!("Only services on this host can be killed");
    }

    let Some(target) = local_favorite_probe_target(&service.ip, service.port, listeners) else {
        bail!("No live local listener owns port {}", service.port);
    };

    let Some(pid) = target.pid else {
        bail!("The local listener owner PID is unavailable");
    };

    if pid == std::process::id() {
        bail!("Refusing to kill the LANVibe process");
    }

    if is_protected_process_pid(pid) {
        bail!("Refusing to kill a protected system process");
    }

    Ok(ProcessKillTarget {
        pid,
        process_owner: target.process_owner,
    })
}

fn is_protected_process_pid(pid: u32) -> bool {
    pid <= 4
}

pub async fn kill_local_service_process(
    service: &Service,
    own_dashboard_port: u16,
) -> Result<KillProcessResult> {
    let listeners = local_listening_ports().await?;
    let target = local_process_kill_target(service, own_dashboard_port, &listeners)?;
    terminate_process(target.pid)?;
    Ok(KillProcessResult {
        service_id: service.id,
        port: service.port,
        pid: target.pid,
        process_owner: target
            .process_owner
            .unwrap_or_else(|| format!("PID {}", target.pid)),
    })
}

fn local_favorite_listener_matches_service(
    listener_addr: Ipv4Addr,
    service_ip: &str,
    parsed_service_ip: Option<Ipv4Addr>,
) -> bool {
    listener_addr.is_unspecified()
        || listener_addr.is_loopback()
        || service_ip.eq_ignore_ascii_case("localhost")
        || parsed_service_ip.is_some_and(|ip| ip.is_loopback() || ip == listener_addr)
}

fn is_local_service_address(ip: &str) -> bool {
    if ip.eq_ignore_ascii_case("localhost") {
        return true;
    }

    ip.parse::<Ipv4Addr>()
        .map(|ip| ip.is_loopback() || discovery::is_local_ip(ip))
        .unwrap_or(false)
}

#[cfg(windows)]
async fn local_listening_ports() -> Result<Vec<LocalTcpListener>> {
    tokio::task::spawn_blocking(windows_listening_ports).await?
}

#[cfg(windows)]
fn windows_listening_ports() -> Result<Vec<LocalTcpListener>> {
    let mut size = 0u32;
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            u32::from(AF_INET),
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }

    if size == 0 {
        return Ok(Vec::new());
    }

    let mut buffer = vec![0u8; size as usize];
    let result = unsafe {
        GetExtendedTcpTable(
            buffer.as_mut_ptr().cast(),
            &mut size,
            0,
            u32::from(AF_INET),
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };

    if result != 0 {
        anyhow::bail!("GetExtendedTcpTable failed with code {result}");
    }

    let table = buffer.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
    let count = unsafe { (*table).dwNumEntries as usize };
    let rows = unsafe {
        std::slice::from_raw_parts(
            (*table).table.as_ptr().cast::<MIB_TCPROW_OWNER_PID>(),
            count,
        )
    };

    let mut listeners = rows
        .iter()
        .filter_map(|row| {
            let port = u16::from_be(row.dwLocalPort as u16);
            if port == 0 {
                return None;
            }
            Some(LocalTcpListener {
                addr: Ipv4Addr::from(u32::from_be(row.dwLocalAddr)),
                port,
                pid: Some(row.dwOwningPid),
                process_owner: windows_process_owner(row.dwOwningPid),
            })
        })
        .collect::<Vec<_>>();
    listeners.sort_by_key(|listener| (listener.port, listener.addr));
    listeners.dedup_by(|a, b| a.port == b.port && a.addr == b.addr);
    Ok(listeners)
}

#[cfg(windows)]
fn windows_process_owner(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }

    Some(match windows_process_name(pid) {
        Some(name) => format!("{name} (PID {pid})"),
        None => format!("PID {pid}"),
    })
}

#[cfg(windows)]
fn windows_process_name(pid: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 || size == 0 {
        return None;
    }

    let full_path = OsString::from_wide(&buffer[..size as usize])
        .to_string_lossy()
        .into_owned();
    let name = Path::new(&full_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&full_path)
        .trim()
        .to_string();

    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(windows)]
fn terminate_process(pid: u32) -> Result<()> {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        bail!("Unable to open process PID {pid} for termination");
    }

    let ok = unsafe { TerminateProcess(handle, 1) };
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 {
        bail!("Unable to terminate process PID {pid}");
    }

    Ok(())
}

#[cfg(not(windows))]
fn terminate_process(_pid: u32) -> Result<()> {
    bail!("Killing service processes is only available on Windows")
}

#[cfg(not(windows))]
async fn local_listening_ports() -> Result<Vec<LocalTcpListener>> {
    if command_exists("ss").await {
        let output = Command::new("ss").args(["-H", "-ltn"]).output().await?;
        if output.status.success() {
            return Ok(parse_listening_port_lines(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }

    if command_exists("lsof").await {
        let output = Command::new("lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN"])
            .output()
            .await?;
        if output.status.success() {
            return Ok(parse_listening_port_lines(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }

    if command_exists("netstat").await {
        let output = Command::new("netstat").args(["-an"]).output().await?;
        if output.status.success() {
            return Ok(parse_listening_port_lines(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }

    anyhow::bail!("no local TCP listener table command is available")
}

#[cfg(not(windows))]
async fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn parse_listening_port_lines(text: &str) -> Vec<LocalTcpListener> {
    let colon_re = Regex::new(r"(?P<addr>(?:\d{1,3}\.){3}\d{1,3}|\*|0\.0\.0\.0):(?P<port>\d+)")
        .expect("valid listener regex");
    let dot_re = Regex::new(r"(?P<addr>(?:\d{1,3}\.){3}\d{1,3}|\*)\.(?P<port>\d+)")
        .expect("valid listener regex");
    let mut listeners = BTreeSet::new();

    for line in text.lines() {
        if !line.to_ascii_uppercase().contains("LISTEN") {
            continue;
        }

        for captures in colon_re
            .captures_iter(line)
            .chain(dot_re.captures_iter(line))
        {
            let Some(port) = captures
                .name("port")
                .and_then(|value| value.as_str().parse::<u16>().ok())
            else {
                continue;
            };
            let addr = captures
                .name("addr")
                .map(|value| value.as_str())
                .and_then(|value| {
                    if value == "*" {
                        Some(Ipv4Addr::UNSPECIFIED)
                    } else {
                        value.parse::<Ipv4Addr>().ok()
                    }
                })
                .unwrap_or(Ipv4Addr::UNSPECIFIED);
            listeners.insert((addr, port));
        }
    }

    listeners
        .into_iter()
        .map(|(addr, port)| LocalTcpListener {
            addr,
            port,
            pid: None,
            process_owner: None,
        })
        .collect()
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
        process_owner: None,
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
    use std::time::Duration;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::{
        extract_page_title, extract_title, favorite_refresh_interval, local_favorite_probe_ip,
        local_process_kill_target, local_scan_targets, probe_port, LocalTcpListener, ProbeTarget,
    };
    use crate::models::{Service, Settings};

    #[test]
    fn refreshes_favorites_every_thirty_seconds() {
        assert_eq!(favorite_refresh_interval(), Duration::from_secs(30));
    }

    #[test]
    fn local_scan_uses_listener_snapshot_targets() {
        let targets = local_scan_targets(
            "192.168.1.20",
            &[
                LocalTcpListener {
                    addr: "127.0.0.1".parse().unwrap(),
                    port: 3000,
                    pid: None,
                    process_owner: None,
                },
                LocalTcpListener {
                    addr: "0.0.0.0".parse().unwrap(),
                    port: 8080,
                    pid: None,
                    process_owner: None,
                },
                LocalTcpListener {
                    addr: "192.168.1.20".parse().unwrap(),
                    port: 9090,
                    pid: None,
                    process_owner: None,
                },
                LocalTcpListener {
                    addr: "192.168.1.21".parse().unwrap(),
                    port: 7000,
                    pid: None,
                    process_owner: None,
                },
                LocalTcpListener {
                    addr: "0.0.0.0".parse().unwrap(),
                    port: 41580,
                    pid: None,
                    process_owner: None,
                },
            ],
            41580,
        );

        assert_eq!(
            targets,
            vec![
                ProbeTarget {
                    port: 3000,
                    probe_ip: "127.0.0.1".to_string(),
                    service_ip: "127.0.0.1".to_string(),
                    process_owner: None,
                },
                ProbeTarget {
                    port: 8080,
                    probe_ip: "192.168.1.20".to_string(),
                    service_ip: "192.168.1.20".to_string(),
                    process_owner: None,
                },
                ProbeTarget {
                    port: 9090,
                    probe_ip: "192.168.1.20".to_string(),
                    service_ip: "192.168.1.20".to_string(),
                    process_owner: None,
                },
            ]
        );
    }

    #[test]
    fn local_favorites_use_listener_snapshot() {
        let listeners = [
            LocalTcpListener {
                addr: "127.0.0.1".parse().unwrap(),
                port: 3000,
                pid: None,
                process_owner: None,
            },
            LocalTcpListener {
                addr: "0.0.0.0".parse().unwrap(),
                port: 8080,
                pid: None,
                process_owner: None,
            },
            LocalTcpListener {
                addr: "192.168.1.21".parse().unwrap(),
                port: 9090,
                pid: None,
                process_owner: None,
            },
        ];

        assert_eq!(
            local_favorite_probe_ip("192.168.1.20", 3000, &listeners).as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            local_favorite_probe_ip("192.168.1.20", 8080, &listeners).as_deref(),
            Some("192.168.1.20")
        );
        assert!(local_favorite_probe_ip("192.168.1.20", 9090, &listeners).is_none());
    }

    #[test]
    fn local_scan_targets_keep_listener_process_owner() {
        let targets = local_scan_targets(
            "192.168.1.20",
            &[LocalTcpListener {
                addr: "0.0.0.0".parse().unwrap(),
                port: 5173,
                pid: Some(4242),
                process_owner: Some("node.exe (PID 4242)".to_string()),
            }],
            41580,
        );

        assert_eq!(
            targets,
            vec![ProbeTarget {
                port: 5173,
                probe_ip: "192.168.1.20".to_string(),
                service_ip: "192.168.1.20".to_string(),
                process_owner: Some("node.exe (PID 4242)".to_string()),
            }]
        );
    }

    #[test]
    fn local_process_kill_target_requires_local_service_and_listener_pid() {
        let listeners = [LocalTcpListener {
            addr: "127.0.0.1".parse().unwrap(),
            port: 8080,
            pid: Some(4242),
            process_owner: Some("node.exe (PID 4242)".to_string()),
        }];
        let service = Service {
            id: 10,
            device_id: "ip:192.168.1.20".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 8080,
            scheme: "http".to_string(),
            url: "http://127.0.0.1:8080/".to_string(),
            title: Some("Vue App".to_string()),
            status_code: Some(200),
            server: None,
            first_seen: "2026-06-08T00:00:00Z".to_string(),
            last_seen: "2026-06-08T00:00:00Z".to_string(),
            last_checked: "2026-06-08T00:00:00Z".to_string(),
            active: true,
            last_failure: None,
            process_owner: None,
        };

        let target = local_process_kill_target(&service, 41580, &listeners).unwrap();
        assert_eq!(target.pid, 4242);
        assert_eq!(target.process_owner.as_deref(), Some("node.exe (PID 4242)"));

        let protected_listeners = [LocalTcpListener {
            addr: "127.0.0.1".parse().unwrap(),
            port: 8080,
            pid: Some(4),
            process_owner: Some("PID 4".to_string()),
        }];
        assert!(local_process_kill_target(&service, 41580, &protected_listeners).is_err());

        let remote_service = Service {
            ip: "192.168.1.101".to_string(),
            url: "http://192.168.1.101:8080/".to_string(),
            ..service
        };
        assert!(local_process_kill_target(&remote_service, 41580, &listeners).is_err());
    }

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
