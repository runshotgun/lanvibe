use anyhow::{anyhow, Result};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub fn apply_launch_at_startup(app: &AppHandle, enabled: bool) -> Result<()> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| anyhow!(error.to_string()))
}
