use std::{
    net::IpAddr,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Result;
use sqlx::SqlitePool;
use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::{
    db, discovery,
    favicon::FaviconStore,
    models::{DiscoveryStatusView, ScanStatusView, Settings, UpdateStatusView},
};

pub struct AppState {
    pub pool: SqlitePool,
    pub settings: RwLock<Settings>,
    pub dashboard_port: RwLock<u16>,
    pub scan_status: RwLock<ScanStatusView>,
    pub scan_running: AtomicBool,
    pub discovery_status: RwLock<DiscoveryStatusView>,
    pub discovery_running: AtomicBool,
    pub update_status: RwLock<UpdateStatusView>,
    pub update_running: AtomicBool,
    pub favicons: FaviconStore,
}

impl AppState {
    pub async fn initialize(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("lanvibe.sqlite3");
        let pool = db::connect(&db_path).await?;
        let settings = db::load_settings(&pool).await?;
        Ok(Self {
            dashboard_port: RwLock::new(settings.dashboard_port),
            scan_status: RwLock::new(ScanStatusView::default()),
            scan_running: AtomicBool::new(false),
            discovery_status: RwLock::new(DiscoveryStatusView::default()),
            discovery_running: AtomicBool::new(false),
            update_status: RwLock::new(UpdateStatusView::default()),
            update_running: AtomicBool::new(false),
            settings: RwLock::new(settings),
            favicons: FaviconStore::new(pool.clone()),
            pool,
        })
    }

    pub async fn current_settings(&self) -> Settings {
        self.settings.read().await.clone()
    }

    pub async fn save_settings(&self, settings: Settings) -> Result<Settings> {
        let normalized = settings.normalized();
        db::save_settings(&self.pool, &normalized).await?;
        *self.settings.write().await = normalized.clone();
        Ok(normalized)
    }

    pub async fn settings_view(&self) -> crate::models::SettingsView {
        self.settings_view_with_loopback_access(true).await
    }

    pub async fn settings_view_with_loopback_access(
        &self,
        can_open_loopback_services: bool,
    ) -> crate::models::SettingsView {
        let settings = self.current_settings().await;
        let actual_dashboard_port = *self.dashboard_port.read().await;
        let dashboard_urls =
            discovery::dashboard_urls(actual_dashboard_port, &settings.dashboard_bind);
        crate::models::SettingsView {
            settings,
            actual_dashboard_port,
            dashboard_urls,
            can_open_loopback_services,
        }
    }
}

pub fn can_http_client_open_loopback_services(client_ip: IpAddr) -> bool {
    can_http_client_open_loopback_services_with_local_ips(
        client_ip,
        discovery::local_ipv4_addresses(),
    )
}

fn can_http_client_open_loopback_services_with_local_ips(
    client_ip: IpAddr,
    local_ips: impl IntoIterator<Item = std::net::Ipv4Addr>,
) -> bool {
    match client_ip {
        IpAddr::V4(ip) => ip.is_loopback() || local_ips.into_iter().any(|local| local == ip),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::can_http_client_open_loopback_services_with_local_ips;

    #[test]
    fn host_http_clients_can_open_loopback_services() {
        let local_ips = [Ipv4Addr::new(192, 168, 1, 100)];
        assert!(can_http_client_open_loopback_services_with_local_ips(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            local_ips
        ));
        assert!(can_http_client_open_loopback_services_with_local_ips(
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            local_ips
        ));
        assert!(can_http_client_open_loopback_services_with_local_ips(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
            local_ips
        ));
    }

    #[test]
    fn remote_http_clients_cannot_open_loopback_services() {
        assert!(!can_http_client_open_loopback_services_with_local_ips(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 55)),
            [Ipv4Addr::new(192, 168, 1, 100)]
        ));
    }
}

pub type SharedState = Arc<AppState>;

#[derive(Clone)]
pub struct ApiState {
    pub app: AppHandle,
    pub state: SharedState,
}
