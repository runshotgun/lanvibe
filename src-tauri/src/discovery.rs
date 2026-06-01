use std::{
    collections::{BTreeMap, BTreeSet},
    net::Ipv4Addr,
    time::Duration,
};

use anyhow::Result;
use futures::{stream, StreamExt};
use get_if_addrs::{get_if_addrs, IfAddr};
use regex::Regex;
use tauri::{AppHandle, Emitter};
use tokio::{process::Command, time};

use crate::{
    app_state::SharedState,
    db,
    models::{Device, DiscoveredDevice},
    tray,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn spawn_loop(app: AppHandle, state: SharedState) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(error) = discover_once(state.clone(), Some(app.clone())).await {
                let _ = app.emit("scan-error", error.to_string());
            }

            let settings = state.current_settings().await;
            time::sleep(Duration::from_secs(settings.discovery_interval_seconds)).await;
        }
    });
}

pub async fn discover_once(state: SharedState, app: Option<AppHandle>) -> Result<Vec<Device>> {
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

    let devices = db::list_devices(&state.pool).await?;
    if let Some(app) = app {
        let _ = app.emit("devices-updated", &devices);
        let _ = tray::refresh(&app).await;
    }
    Ok(devices)
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
    let output = if cfg!(target_os = "windows") {
        let mut command = Command::new("arp");
        hide_tokio_command_window(&mut command);
        command.arg("-a").output().await?
    } else if command_exists("ip").await {
        Command::new("ip").arg("neigh").output().await?
    } else {
        Command::new("arp").arg("-a").output().await?
    };

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_arp_output(&text))
}

fn parse_arp_output(text: &str) -> Vec<DiscoveredDevice> {
    let ip_re = Regex::new(r"(?P<ip>(?:\d{1,3}\.){3}\d{1,3})").expect("valid ip regex");
    let mac_re = Regex::new(r"(?i)(?P<mac>[0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2})")
        .expect("valid mac regex");

    text.lines()
        .filter_map(|line| {
            let ip = ip_re.captures(line)?.name("ip")?.as_str().to_string();
            let mac = mac_re
                .captures(line)
                .and_then(|captures| captures.name("mac"))
                .map(|value| value.as_str().replace('-', ":").to_ascii_lowercase());
            let parsed_ip = ip.parse::<Ipv4Addr>().ok()?;
            if !is_private_lan(parsed_ip) {
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

    use super::{is_usable_lan_interface, parse_arp_output};

    #[test]
    fn parses_windows_arp_lines() {
        let devices = parse_arp_output(
            "  Internet Address      Physical Address      Type\n  192.168.1.10          aa-bb-cc-dd-ee-ff     dynamic",
        );
        assert_eq!(devices[0].ip, "192.168.1.10");
        assert_eq!(devices[0].mac.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
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
