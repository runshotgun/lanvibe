use std::{
    collections::{BTreeMap, BTreeSet},
    net::Ipv4Addr,
    sync::atomic::Ordering,
    time::Duration,
};

use anyhow::Result;
use chrono::Utc;
use futures::{stream, StreamExt};
use get_if_addrs::{get_if_addrs, IfAddr};
use regex::Regex;
use tauri::{AppHandle, Emitter};
use tokio::{process::Command, time};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::{
    app_state::SharedState,
    db,
    models::{Device, DiscoveredDevice, DiscoveryStatusView},
    tray,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn spawn_at_launch(app: AppHandle, state: SharedState) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = discover_once(state.clone(), Some(app.clone())).await {
            let _ = app.emit("scan-error", error.to_string());
        }
    });
}

#[cfg(test)]
pub fn device_discovery_repeat_interval() -> Option<Duration> {
    None
}

pub async fn discover_once(state: SharedState, app: Option<AppHandle>) -> Result<Vec<Device>> {
    if state
        .discovery_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return db::list_devices(&state.pool).await;
    }

    let started_at = Utc::now().to_rfc3339();
    publish_discovery_status(
        app.as_ref(),
        state.clone(),
        DiscoveryStatusView {
            phase: "discovering".to_string(),
            discovered_devices: 0,
            started_at: Some(started_at.clone()),
            finished_at: None,
        },
    )
    .await;

    let result = discover_once_inner(state.clone(), app.clone()).await;
    let discovered_devices = result.as_ref().map(|devices| devices.len()).unwrap_or(0);
    publish_discovery_status(
        app.as_ref(),
        state.clone(),
        DiscoveryStatusView {
            phase: "idle".to_string(),
            discovered_devices,
            started_at: Some(started_at),
            finished_at: Some(Utc::now().to_rfc3339()),
        },
    )
    .await;
    state.discovery_running.store(false, Ordering::Release);
    result
}

async fn discover_once_inner(state: SharedState, app: Option<AppHandle>) -> Result<Vec<Device>> {
    let mut discovered = BTreeMap::<String, DiscoveredDevice>::new();

    for device in arp_devices().await? {
        discovered.insert(device.ip.clone(), device);
    }

    let pinged = ping_sweep().await;
    for ip in pinged {
        discovered.entry(ip.clone()).or_insert(DiscoveredDevice {
            ip,
            hostname: None,
            mac: None,
            vendor: None,
            source: "ping".to_string(),
        });
    }

    let mut saved = Vec::new();
    for mut device in discovered.into_values() {
        if device.hostname.is_none() {
            device.hostname = if let Some(hostname) = local_hostname(&device.ip).await {
                Some(hostname)
            } else {
                reverse_hostname(&device.ip).await
            };
        }
        saved.push(db::upsert_device(&state.pool, &device).await?);
    }
    prune_unusable_device_records(&state).await?;

    let devices = db::list_devices(&state.pool).await?;
    if let Some(app) = app {
        let _ = app.emit("devices-updated", &devices);
        let _ = tray::refresh(&app).await;
    }
    Ok(devices)
}

async fn publish_discovery_status(
    app: Option<&AppHandle>,
    state: SharedState,
    status: DiscoveryStatusView,
) {
    *state.discovery_status.write().await = status.clone();
    if let Some(app) = app {
        let _ = app.emit("discovery-status", status);
    }
}

pub fn dashboard_urls(port: u16, bind: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let bind = bind.trim();

    if bind == "127.0.0.1" || bind.eq_ignore_ascii_case("localhost") {
        urls.push(format!("http://localhost:{port}"));
        return urls;
    }

    for ip in local_ipv4_addresses() {
        urls.push(format!("http://{ip}:{port}"));
    }

    if urls.is_empty() {
        urls.push(format!("http://localhost:{port}"));
    }

    urls
}

pub fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
    usable_interface_ipv4_addresses()
}

fn usable_interface_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut addresses = get_if_addrs()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .filter_map(|interface| {
                    let IfAddr::V4(v4) = interface.addr else {
                        return None;
                    };
                    if is_usable_lan_interface(&interface.name, v4.ip) {
                        Some(v4.ip)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if let Some(with_neighbors) = interface_ips_with_dynamic_neighbors() {
        if !with_neighbors.is_empty() {
            addresses.retain(|ip| with_neighbors.contains(ip));
        }
    }

    addresses.sort_by_key(|ip| {
        let octets = ip.octets();
        match octets {
            [192, 168, ..] => 0,
            [10, ..] => 1,
            [172, second, ..] if (16..=31).contains(&second) => 2,
            _ => 3,
        }
    });
    addresses.dedup();
    addresses
}

pub fn is_local_ip(ip: Ipv4Addr) -> bool {
    local_ipv4_addresses().into_iter().any(|local| local == ip)
}

async fn arp_devices() -> Result<Vec<DiscoveredDevice>> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("arp");
        hide_tokio_command_window(&mut command);
        command.arg("-a");
        command
    } else if command_exists("ip").await {
        let mut command = Command::new("ip");
        command.arg("neigh");
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("arp");
        command.arg("-an");
        command
    } else {
        let mut command = Command::new("arp");
        command.arg("-a");
        command
    };

    let output = match time::timeout(Duration::from_secs(3), command.output()).await {
        Ok(output) => output?,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_arp_output(&text))
}

fn parse_arp_output(text: &str) -> Vec<DiscoveredDevice> {
    let ip_re = Regex::new(r"(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").expect("valid ip regex");
    let mac_re =
        Regex::new(r"(?i)(?P<mac>[0-9a-f]{1,2}(?:[:-][0-9a-f]{1,2}){5})").expect("valid mac regex");
    let interface_re =
        Regex::new(r"(?i)^interface:\s+(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").expect("valid regex");
    let usable_interfaces = usable_interface_ipv4_addresses()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut current_interface_allowed: Option<bool> = None;

    text.lines()
        .filter_map(move |line| {
            if line.to_ascii_lowercase().contains("(incomplete)") {
                return None;
            }

            if let Some(interface) = interface_re.captures(line) {
                current_interface_allowed = interface
                    .name("ip")
                    .and_then(|value| value.as_str().parse::<Ipv4Addr>().ok())
                    .map(|ip| usable_interfaces.is_empty() || usable_interfaces.contains(&ip));
                return None;
            }

            if current_interface_allowed == Some(false) {
                return None;
            }

            let ip = ip_re.captures(line)?.name("ip")?.as_str().to_string();
            let mac = mac_re
                .captures(line)
                .and_then(|captures| captures.name("mac"))
                .and_then(|value| normalize_mac(value.as_str()));
            let parsed_ip = ip.parse::<Ipv4Addr>().ok()?;
            if is_local_ip(parsed_ip)
                || !is_discoverable_device_ip(parsed_ip)
                || is_unusable_mac(mac.as_deref())
            {
                return None;
            }
            Some(DiscoveredDevice {
                ip,
                hostname: None,
                vendor: infer_vendor(mac.as_deref()).map(ToOwned::to_owned),
                mac,
                source: "arp".to_string(),
            })
        })
        .collect()
}

fn normalize_mac(value: &str) -> Option<String> {
    let parts = value
        .replace('-', ":")
        .split(':')
        .map(|part| u8::from_str_radix(part, 16).ok())
        .collect::<Option<Vec<_>>>()?;

    if parts.len() != 6 {
        return None;
    }

    Some(
        parts
            .into_iter()
            .map(|part| format!("{part:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

async fn ping_sweep() -> Vec<String> {
    let candidates = candidate_ping_hosts();
    stream::iter(candidates)
        .map(|ip| async move {
            if ping(&ip).await {
                Some(ip.to_string())
            } else {
                None
            }
        })
        .buffer_unordered(64)
        .filter_map(|ip| async move { ip })
        .collect()
        .await
}

fn candidate_ping_hosts() -> Vec<Ipv4Addr> {
    let mut candidates = BTreeSet::new();

    for ip in local_ipv4_addresses() {
        let octets = ip.octets();
        for host in 1..=254u8 {
            let candidate = Ipv4Addr::new(octets[0], octets[1], octets[2], host);
            if candidate != ip {
                candidates.insert(candidate);
            }
        }
    }

    candidates.into_iter().collect()
}

async fn ping(ip: &Ipv4Addr) -> bool {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("ping");
        hide_tokio_command_window(&mut command);
        command.args(["-n", "1", "-w", "700", &ip.to_string()]);
        command
    } else {
        let mut command = Command::new("ping");
        command.args(["-c", "1", "-W", "1", &ip.to_string()]);
        command
    };

    match time::timeout(Duration::from_millis(1200), command.output()).await {
        Ok(Ok(output)) => output.status.success(),
        _ => false,
    }
}

async fn command_exists(name: &str) -> bool {
    let output = if cfg!(target_os = "windows") {
        let mut command = Command::new("where");
        hide_tokio_command_window(&mut command);
        command.arg(name).output().await
    } else {
        Command::new("sh")
            .args(["-c", &format!("command -v {name}")])
            .output()
            .await
    };

    output.map(|value| value.status.success()).unwrap_or(false)
}

async fn reverse_hostname(ip: &str) -> Option<String> {
    let ip = ip.parse().ok()?;
    tokio::task::spawn_blocking(move || dns_lookup::lookup_addr(&ip).ok())
        .await
        .ok()
        .flatten()
}

async fn local_hostname(ip: &str) -> Option<String> {
    let parsed = ip.parse::<Ipv4Addr>().ok()?;
    if !is_local_ip(parsed) {
        return None;
    }

    if let Some(hostname) = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .and_then(|value| normalize_hostname(&value))
    {
        return Some(hostname);
    }

    let mut command = Command::new("hostname");
    hide_tokio_command_window(&mut command);
    let output = command.output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    normalize_hostname(&String::from_utf8_lossy(&output.stdout))
}

fn normalize_hostname(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "?" || trimmed.parse::<Ipv4Addr>().is_ok() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_private_lan(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.octets()[0] == 169 && ip.octets()[1] == 254
}

fn is_discoverable_device_ip(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    is_private_lan(ip)
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && octets != [0, 0, 0, 0]
        && octets != [255, 255, 255, 255]
        && octets[3] != 0
        && octets[3] != 255
}

fn is_unusable_mac(mac: Option<&str>) -> bool {
    let Some(mac) = mac else {
        return false;
    };
    let normalized = mac.to_ascii_lowercase().replace('-', ":");
    normalized == "ff:ff:ff:ff:ff:ff"
        || normalized.starts_with("01:00:5e:")
        || normalized.starts_with("33:33:")
}

fn is_on_usable_local_subnet(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    usable_interface_ipv4_addresses()
        .into_iter()
        .map(|local| local.octets())
        .any(|local| local[0..3] == octets[0..3])
}

async fn prune_unusable_device_records(state: &SharedState) -> Result<()> {
    let devices = db::list_devices(&state.pool).await?;
    let mut local_ids = Vec::new();
    let mut stale_ids = Vec::new();

    for device in devices {
        if let Ok(ip) = device.ip.parse::<Ipv4Addr>() {
            if is_local_ip(ip) {
                local_ids.push(device.id);
                continue;
            }

            let invalid_ip = !is_discoverable_device_ip(ip);
            let stale_off_subnet = !is_on_usable_local_subnet(ip);
            let unusable_mac = is_unusable_mac(device.mac.as_deref());

            if invalid_ip || stale_off_subnet || unusable_mac {
                stale_ids.push(device.id);
            }
        }
    }

    db::delete_devices_with_dependents(&state.pool, &local_ids).await?;
    db::delete_unselected_devices_without_services(&state.pool, &stale_ids).await
}

fn is_usable_lan_interface(name: &str, ip: Ipv4Addr) -> bool {
    if !ip.is_private() || ip.is_loopback() || ip.is_link_local() {
        return false;
    }

    let name = name.to_ascii_lowercase();
    ![
        "bluetooth",
        "docker",
        "hyper-v",
        "loopback",
        "tailscale",
        "virtual",
        "vethernet",
        "vmware",
        "wsl",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}

#[cfg(windows)]
fn interface_ips_with_dynamic_neighbors() -> Option<BTreeSet<Ipv4Addr>> {
    let mut command = std::process::Command::new("arp");
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.arg("-a").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_dynamic_arp_interface_ips(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(not(windows))]
fn interface_ips_with_dynamic_neighbors() -> Option<BTreeSet<Ipv4Addr>> {
    None
}

#[cfg(any(windows, test))]
fn parse_dynamic_arp_interface_ips(text: &str) -> BTreeSet<Ipv4Addr> {
    let interface_re =
        Regex::new(r"(?i)^interface:\s+(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").expect("valid regex");
    let mut current = None;
    let mut interfaces = BTreeSet::new();

    for line in text.lines() {
        if let Some(interface) = interface_re.captures(line) {
            current = interface
                .name("ip")
                .and_then(|value| value.as_str().parse::<Ipv4Addr>().ok());
            continue;
        }

        if line.to_ascii_lowercase().contains(" dynamic") {
            if let Some(ip) = current {
                interfaces.insert(ip);
            }
        }
    }

    interfaces
}

fn infer_vendor(mac: Option<&str>) -> Option<&'static str> {
    let oui = mac?
        .to_ascii_lowercase()
        .replace('-', ":")
        .split(':')
        .take(3)
        .collect::<Vec<_>>()
        .join(":");
    match oui.as_str() {
        "d0:11:e5" | "a8:20:66" | "bc:d0:74" | "f0:18:98" | "3c:22:fb" => Some("Apple"),
        _ => None,
    }
}

#[cfg(windows)]
fn hide_tokio_command_window(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_tokio_command_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::{
        device_discovery_repeat_interval, is_discoverable_device_ip, is_unusable_mac,
        is_usable_lan_interface, parse_arp_output, parse_dynamic_arp_interface_ips,
    };

    #[test]
    fn does_not_repeat_device_discovery_after_launch() {
        assert_eq!(device_discovery_repeat_interval(), None);
    }

    #[test]
    fn parses_windows_arp_lines() {
        let devices = parse_arp_output(
            "  Internet Address      Physical Address      Type\n  192.168.1.10          aa-bb-cc-dd-ee-ff     dynamic",
        );
        assert_eq!(devices[0].ip, "192.168.1.10");
        assert_eq!(devices[0].mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn parses_macos_numeric_arp_lines() {
        let devices = parse_arp_output(
            "? (192.168.1.10) at dc:a6:32:e5:74:1d on en0 ifscope [ethernet]\n\
             ? (192.168.1.195) at (incomplete) on en0 ifscope [ethernet]\n\
             ? (192.168.1.205) at b0:a7:b9:ee:29:e on en0 ifscope [ethernet]",
        );
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].ip, "192.168.1.10");
        assert_eq!(devices[0].mac.as_deref(), Some("dc:a6:32:e5:74:1d"));
        assert_eq!(devices[1].ip, "192.168.1.205");
        assert_eq!(devices[1].mac.as_deref(), Some("b0:a7:b9:ee:29:0e"));
    }

    #[test]
    fn filters_broadcast_arp_entries() {
        let devices = parse_arp_output(
            "  Internet Address      Physical Address      Type\n  192.168.1.255         ff-ff-ff-ff-ff-ff     static",
        );
        assert!(devices.is_empty());
        assert!(!is_discoverable_device_ip(Ipv4Addr::new(192, 168, 1, 255)));
        assert!(is_unusable_mac(Some("ff-ff-ff-ff-ff-ff")));
    }

    #[test]
    fn detects_windows_interfaces_with_dynamic_neighbors() {
        let interfaces = parse_dynamic_arp_interface_ips(
            r#"
Interface: 172.22.16.1 --- 0x18
  224.0.0.251           01-00-5e-00-00-fb     static

Interface: 192.168.1.100 --- 0x19
  192.168.1.1           d0-21-f9-70-73-35     dynamic
"#,
        );
        assert!(interfaces.contains(&Ipv4Addr::new(192, 168, 1, 100)));
        assert!(!interfaces.contains(&Ipv4Addr::new(172, 22, 16, 1)));
    }

    #[test]
    fn filters_virtual_lan_interfaces_from_dashboard_urls() {
        assert!(is_usable_lan_interface(
            "Ethernet",
            Ipv4Addr::new(192, 168, 1, 100)
        ));
        assert!(!is_usable_lan_interface(
            "vEthernet (WSL)",
            Ipv4Addr::new(172, 26, 96, 1)
        ));
        assert!(!is_usable_lan_interface(
            "Bluetooth Network Connection",
            Ipv4Addr::new(192, 168, 44, 1)
        ));
        assert!(!is_usable_lan_interface(
            "Ethernet",
            Ipv4Addr::new(169, 254, 1, 20)
        ));
    }

    #[test]
    fn infers_known_mac_vendor_without_naming_the_device() {
        let devices = parse_arp_output("  192.168.1.10          d0-11-e5-00-1c-76     dynamic");
        assert_eq!(devices[0].vendor.as_deref(), Some("Apple"));
    }

    #[test]
    fn parses_unix_arp_lines() {
        let devices = parse_arp_output("? (10.0.0.9) at 00:11:22:33:44:55 on en0 ifscope");
        assert_eq!(devices[0].ip, "10.0.0.9");
        assert_eq!(devices[0].mac.as_deref(), Some("00:11:22:33:44:55"));
    }
}
