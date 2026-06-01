use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub name_override: Option<String>,
    pub selected: bool,
    pub ignored: bool,
    pub source: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: i64,
    pub device_id: String,
    pub ip: String,
    pub port: u16,
    pub scheme: String,
    pub url: String,
    pub title: Option<String>,
    pub status_code: Option<i64>,
    pub server: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
    pub last_checked: String,
    pub active: bool,
    pub last_failure: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_scan: bool,
    pub manual_only: bool,
    pub minimize_to_tray: bool,
    pub launch_at_startup: bool,
    pub scan_interval_seconds: u64,
    pub discovery_interval_seconds: u64,
    pub retention_days: i64,
    pub scan_concurrency: usize,
    pub connect_timeout_ms: u64,
    pub http_timeout_ms: u64,
    pub dashboard_bind: String,
    pub dashboard_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_scan: true,
            manual_only: false,
            minimize_to_tray: true,
            launch_at_startup: true,
            scan_interval_seconds: 120,
            discovery_interval_seconds: 60,
            retention_days: 30,
            scan_concurrency: 512,
            connect_timeout_ms: 450,
            http_timeout_ms: 1200,
            dashboard_bind: "0.0.0.0".to_string(),
            dashboard_port: 8765,
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        self.scan_interval_seconds = self.scan_interval_seconds.max(30);
        self.discovery_interval_seconds = self.discovery_interval_seconds.max(30);
        self.retention_days = self.retention_days.max(1);
        self.scan_concurrency = self.scan_concurrency.clamp(32, 4096);
        self.connect_timeout_ms = self.connect_timeout_ms.clamp(100, 10_000);
        self.http_timeout_ms = self.http_timeout_ms.clamp(250, 20_000);
        self.dashboard_bind = normalize_dashboard_bind(&self.dashboard_bind);
        self
    }
}

fn normalize_dashboard_bind(bind: &str) -> String {
    let trimmed = bind.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("localhost") {
        return Ipv4Addr::UNSPECIFIED.to_string();
    }

    match trimmed.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) if ip.is_loopback() => Ipv4Addr::UNSPECIFIED.to_string(),
        Ok(IpAddr::V6(ip)) if ip.is_loopback() => Ipv4Addr::UNSPECIFIED.to_string(),
        Ok(IpAddr::V6(ip)) if ip == Ipv6Addr::UNSPECIFIED => Ipv4Addr::UNSPECIFIED.to_string(),
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod settings_tests {
    use super::{normalize_dashboard_bind, Settings};

    #[test]
    fn dashboard_bind_defaults_to_lan_exposed_wildcard() {
        assert_eq!(normalize_dashboard_bind(""), "0.0.0.0");
        assert_eq!(normalize_dashboard_bind("localhost"), "0.0.0.0");
        assert_eq!(normalize_dashboard_bind("127.0.0.1"), "0.0.0.0");
        assert_eq!(normalize_dashboard_bind("::1"), "0.0.0.0");
    }

    #[test]
    fn dashboard_bind_keeps_explicit_lan_bind() {
        assert_eq!(normalize_dashboard_bind("192.168.1.100"), "192.168.1.100");
    }

    #[test]
    fn settings_normalization_prevents_loopback_only_dashboard() {
        let settings = Settings {
            dashboard_bind: "127.0.0.1".to_string(),
            ..Settings::default()
        }
        .normalized();

        assert_eq!(settings.dashboard_bind, "0.0.0.0");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Settings,
    pub actual_dashboard_port: u16,
    pub dashboard_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePatch {
    pub selected: bool,
    pub ignored: bool,
    pub name_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritePatch {
    pub service_key: String,
    pub favorite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub scanned_devices: usize,
    pub discovered_services: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusView {
    pub phase: String,
    pub selected_devices: usize,
    pub scanned_devices: usize,
    pub discovered_services: usize,
    pub current_device_ip: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl Default for ScanStatusView {
    fn default() -> Self {
        Self {
            phase: "idle".to_string(),
            selected_devices: 0,
            scanned_devices: 0,
            discovered_services: 0,
            current_device_ip: None,
            started_at: None,
            finished_at: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub source: String,
}

impl DiscoveredDevice {
    pub fn stable_id(&self) -> String {
        if let Some(mac) = &self.mac {
            return format!("mac:{}", mac.to_ascii_lowercase().replace('-', ":"));
        }
        format!("ip:{}", self.ip)
    }
}

#[derive(Debug, Clone)]
pub struct ProbeHit {
    pub port: u16,
    pub scheme: String,
    pub url: String,
    pub title: Option<String>,
    pub status_code: Option<i64>,
    pub server: Option<String>,
}
