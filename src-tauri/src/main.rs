#![cfg_attr(windows, windows_subsystem = "windows")]

mod api;
mod app_state;
mod db;
mod discovery;
mod favicon;
mod models;
mod native_effects;
mod scanner;
mod startup;
mod tray;
mod web;

use std::sync::Arc;
use tauri::{Manager, WindowEvent};

use app_state::AppState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = tauri::async_runtime::block_on(AppState::initialize(data_dir))?;
            let state = Arc::new(state);

            app.manage(state.clone());
            let launch_at_startup =
                tauri::async_runtime::block_on(state.current_settings()).launch_at_startup;
            let _ = startup::apply_launch_at_startup(app.handle(), launch_at_startup);
            tray::create(app.handle(), state.clone())?;

            // Keep the native popover window transparent from birth. The visible
            // tint is drawn immediately by CSS, while Windows supplies real
            // compositor blur before the window is ever moved onscreen.
            if let Some(window) = app.get_webview_window("popover") {
                let _ = window.set_background_color(Some(tauri::utils::config::Color(0, 0, 0, 0)));
                let _ = window.set_shadow(false);
                let _ = window.set_decorations(false);
                if let Some(icon) = tray::app_icon() {
                    let _ = window.set_icon(icon);
                }
                native_effects::apply_popover_frost(&window);
            }

            if let Some(window) = app.get_webview_window("main") {
                if let Some(icon) = tray::app_icon() {
                    let _ = window.set_icon(icon);
                }
            }

            // Park the popover visible-but-off-screen so WebView startup happens
            // before the user opens it from the tray.
            tray::prime_popover(app.handle());

            tauri::async_runtime::spawn(web::run(app.handle().clone(), state.clone()));
            discovery::spawn_loop(app.handle().clone(), state.clone());
            scanner::spawn_loop(app.handle().clone(), state.clone());
            scanner::spawn_favorite_loop(app.handle().clone(), state.clone());

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "popover" {
                    api.prevent_close();
                    tray::close_popover(window.app_handle());
                    return;
                }
                let state = window.app_handle().state::<Arc<AppState>>();
                let settings = tauri::async_runtime::block_on(state.current_settings());
                if settings.minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            WindowEvent::Focused(false) if window.label() == "popover" => {
                tray::hide_popover_on_blur(window.app_handle());
            }
            WindowEvent::ScaleFactorChanged { .. } if window.label() == "popover" => {
                if let Some(popover) = window.app_handle().get_webview_window("popover") {
                    native_effects::apply_popover_shape(&popover);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            api::list_devices,
            api::refresh_devices,
            api::update_device,
            api::list_services,
            api::list_favorites,
            api::set_favorite,
            api::start_manual_scan,
            api::get_scan_status,
            api::get_update_status,
            api::trigger_host_update,
            api::get_settings_view,
            api::update_settings,
            api::open_url,
            api::get_favicon,
            api::close_popover,
            api::open_main_window,
            api::resize_popover
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LANVibe");
}
