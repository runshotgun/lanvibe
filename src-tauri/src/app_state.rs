use std::{
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
    models::{ScanStatusView, Settings, UpdateStatusView},
};

pub struct AppState {
    pub pool: SqlitePool,
    pub settings: RwLock<Settings>,
    pub dashboard_port: RwLock<u16>,
    pub scan_status: RwLock<ScanStatusView>,
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
            update_status: RwLock::new(UpdateStatusView::default()),
            update_running: AtomicBool::new(false),
            settings: RwLock::new(settings),
            favicons: FaviconStore::new(),
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
        let settings = self.current_settings().await;
        let actual_dashboard_port = *self.dashboard_port.read().await;
        let dashboard_urls =
            discovery::dashboard_urls(actual_dashboard_port, &settings.dashboard_bind);
        crate::models::SettingsView {
            settings,
            actual_dashboard_port,
            dashboard_urls,
        }
    }
}

pub type SharedState = Arc<AppState>;

#[derive(Clone)]
pub struct ApiState {
    pub app: AppHandle,
    pub state: SharedState,
}
