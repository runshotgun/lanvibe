use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, Position, Rect, WebviewWindow,
};

use crate::{app_state::AppState, db};

const TRAY_ID: &str = "main-tray";
const WINDOW_MARGIN: i32 = 12;
/// When the popover loses focus from the very click that hit the tray icon, the
/// blur-hide and the tray toggle race. Within this window we treat a toggle as a
/// "close" so the popover doesn't immediately reopen.
const POPOVER_REOPEN_GUARD: Duration = Duration::from_millis(250);
/// Windows briefly drops focus on a window right after a tray-triggered show.
/// Ignore blur-hide requests within this window so the popover doesn't self-close.
const POPOVER_SHOW_GRACE: Duration = Duration::from_millis(500);
/// Where the popover parks while "closed". We keep the window visible and
/// simply move it far off every monitor instead of hiding it. Re-showing a
/// hidden transparent window can otherwise flash before the WebView settles.
const OFFSCREEN_X: i32 = -32000;
const OFFSCREEN_Y: i32 = -32000;

#[derive(Debug, Clone, Copy)]
struct TrayAnchor {
    x: f64,
    y: f64,
}

static LAST_TRAY_ANCHOR: OnceLock<Mutex<Option<TrayAnchor>>> = OnceLock::new();
static LAST_POPOVER_HIDE: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static LAST_POPOVER_SHOW: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
/// Whether the popover is currently parked on-screen (open) vs off-screen.
static POPOVER_OPEN: AtomicBool = AtomicBool::new(false);

pub fn create(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    let launch_at_startup =
        tauri::async_runtime::block_on(state.current_settings()).launch_at_startup;
    let menu = build_menu(app, launch_at_startup)?;
    let favorite_count = tauri::async_runtime::block_on(favorite_count(&state));

    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip_text(favorite_count))
        .icon(tray_icon())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            remember_tray_anchor(&event);

            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_popover(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "scan" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("manual-scan-requested", ());
                }
            }
            "launch_at_startup" => toggle_launch_at_startup(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Keeps the tray tooltip's favorite count in sync. The menu itself is static
/// (favorites now live in the popover window), so only the tooltip is refreshed.
pub async fn refresh(app: &AppHandle) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let state = app.state::<Arc<AppState>>();
    let count = favorite_count(state.inner()).await;
    let launch_at_startup = state.current_settings().await.launch_at_startup;
    tray.set_tooltip(Some(tooltip_text(count)))?;
    let menu = build_menu(app, launch_at_startup)?;
    tray.set_menu(Some(menu))?;
    Ok(())
}

fn build_menu(app: &AppHandle, launch_at_startup: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let open_item = MenuItem::with_id(app, "open", "Open LANVibe", true, None::<&str>)?;
    let scan_item = MenuItem::with_id(app, "scan", "Scan now", true, None::<&str>)?;
    let startup_item = CheckMenuItem::with_id(
        app,
        "launch_at_startup",
        "Launch at startup",
        true,
        launch_at_startup,
        None::<&str>,
    )?;
    menu.append(&open_item)?;
    menu.append(&scan_item)?;
    menu.append(&startup_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    menu.append(&quit_item)?;
    Ok(menu)
}

fn toggle_launch_at_startup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>();
        let mut settings = state.current_settings().await;
        settings.launch_at_startup = !settings.launch_at_startup;
        if crate::startup::apply_launch_at_startup(&app, settings.launch_at_startup).is_ok() {
            let _ = state.save_settings(settings).await;
        }
        let _ = refresh(&app).await;
        let _ = app.emit("settings-updated", ());
    });
}

async fn favorite_count(state: &AppState) -> usize {
    db::list_favorite_keys(&state.pool)
        .await
        .unwrap_or_default()
        .len()
}

fn tooltip_text(count: usize) -> String {
    format!(
        "LANVibe - {count} favorite{}",
        if count == 1 { "" } else { "s" }
    )
}

/// Make the popover visible but parked off-screen at startup, so WebView
/// startup happens before the user opens it from the tray.
pub fn prime_popover(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("popover") {
        park_popover_offscreen(&window);
        let _ = window.show();
    }
}

/// Left-click handler: opens the favorites popover near the tray, or closes it
/// if already open.
pub fn toggle_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popover") else {
        return;
    };

    if POPOVER_OPEN.load(Ordering::SeqCst) {
        hide_popover_window(&window);
        return;
    }

    if recently_hidden() {
        return;
    }

    // The window is already visible (primed off-screen), so opening is a
    // reposition + focus. Position before show as a fallback for cases where
    // the platform has hidden or recreated the WebView.
    let _ = window.set_shadow(false);
    crate::native_effects::apply_popover_frost(&window);
    let _ = position_window_near_tray(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    POPOVER_OPEN.store(true, Ordering::SeqCst);
    if let Ok(mut shown) = LAST_POPOVER_SHOW.get_or_init(|| Mutex::new(None)).lock() {
        *shown = Some(Instant::now());
    }
    let _ = window.emit("popover-shown", ());
}

/// Close the popover from the frontend (X button, opening a favorite, etc.).
/// Parks it off-screen instead of hiding so the next open doesn't flash.
pub fn close_popover(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("popover") {
        hide_popover_window(&window);
    }
}

/// Blur handler. Closes the popover so it behaves like a menu, but ignores the
/// transient focus loss that Windows fires immediately after a tray-show.
pub fn hide_popover_on_blur(app: &AppHandle) {
    if recently_shown() {
        return;
    }
    if POPOVER_OPEN.load(Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("popover") {
            hide_popover_window(&window);
        }
    }
}

fn recently_shown() -> bool {
    LAST_POPOVER_SHOW
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|shown| *shown)
        .map(|instant| instant.elapsed() < POPOVER_SHOW_GRACE)
        .unwrap_or(false)
}

fn hide_popover_window(window: &WebviewWindow) {
    // Park off-screen rather than hide, to keep the frosted backdrop composited.
    park_popover_offscreen(window);
    POPOVER_OPEN.store(false, Ordering::SeqCst);
    if let Ok(mut last) = LAST_POPOVER_HIDE.get_or_init(|| Mutex::new(None)).lock() {
        *last = Some(Instant::now());
    }
}

fn park_popover_offscreen(window: &WebviewWindow) {
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(
        OFFSCREEN_X,
        OFFSCREEN_Y,
    )));
}

fn recently_hidden() -> bool {
    LAST_POPOVER_HIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|last| *last)
        .map(|instant| instant.elapsed() < POPOVER_REOPEN_GUARD)
        .unwrap_or(false)
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn remember_tray_anchor(event: &TrayIconEvent) {
    let Some(anchor) = (match event {
        TrayIconEvent::Click { position, rect, .. }
        | TrayIconEvent::DoubleClick { position, rect, .. }
        | TrayIconEvent::Enter { position, rect, .. }
        | TrayIconEvent::Move { position, rect, .. }
        | TrayIconEvent::Leave { position, rect, .. } => Some(anchor_from_event(*position, *rect)),
        _ => None,
    }) else {
        return;
    };

    if let Ok(mut latest) = LAST_TRAY_ANCHOR.get_or_init(|| Mutex::new(None)).lock() {
        *latest = Some(anchor);
    }
}

fn anchor_from_event(position: PhysicalPosition<f64>, rect: Rect) -> TrayAnchor {
    let rect_position = rect.position.to_physical::<f64>(1.0);
    let rect_size = rect.size.to_physical::<f64>(1.0);

    if rect_size.width > 0.0 && rect_size.height > 0.0 {
        TrayAnchor {
            x: rect_position.x + (rect_size.width / 2.0),
            y: rect_position.y + (rect_size.height / 2.0),
        }
    } else {
        TrayAnchor {
            x: position.x,
            y: position.y,
        }
    }
}

fn position_window_near_tray(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
    let Some(anchor) = LAST_TRAY_ANCHOR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|latest| *latest)
    else {
        return Ok(());
    };

    let window_size = window.outer_size()?;
    let monitor = app
        .monitor_from_point(anchor.x, anchor.y)?
        .or_else(|| app.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return Ok(());
    };

    let work_area = monitor.work_area();
    let work_left = work_area.position.x;
    let work_top = work_area.position.y;
    let work_right = work_left + work_area.size.width as i32;
    let work_bottom = work_top + work_area.size.height as i32;
    let window_width = window_size.width as i32;
    let window_height = window_size.height as i32;
    let anchor_x = anchor.x.round() as i32;
    let anchor_y = anchor.y.round() as i32;

    let (mut x, mut y) = if anchor_y >= work_bottom {
        (
            anchor_x - window_width + 32,
            work_bottom - window_height - WINDOW_MARGIN,
        )
    } else if anchor_y <= work_top {
        (anchor_x - window_width + 32, work_top + WINDOW_MARGIN)
    } else if anchor_x <= work_left {
        (work_left + WINDOW_MARGIN, anchor_y - window_height + 32)
    } else if anchor_x >= work_right {
        (
            work_right - window_width - WINDOW_MARGIN,
            anchor_y - window_height + 32,
        )
    } else {
        (
            anchor_x - (window_width / 2),
            anchor_y - window_height - WINDOW_MARGIN,
        )
    };

    if y < work_top {
        y = anchor_y + WINDOW_MARGIN;
    }

    x = clamp_to_work_area(x, window_width, work_left, work_right);
    y = clamp_to_work_area(y, window_height, work_top, work_bottom);

    window.set_position(Position::Physical(PhysicalPosition::new(x, y)))
}

fn clamp_to_work_area(position: i32, size: i32, min: i32, max: i32) -> i32 {
    if max - min <= size {
        min
    } else {
        position.clamp(min, max - size)
    }
}

fn tray_icon() -> Image<'static> {
    if let Some(icon) = decode_icon(include_bytes!("../icons/tray.png")) {
        return icon;
    }

    if let Some(icon) = app_icon() {
        return icon;
    }

    fallback_icon()
}

pub fn app_icon() -> Option<Image<'static>> {
    decode_icon(include_bytes!("../icons/64x64.png"))
}

fn decode_icon(bytes: &[u8]) -> Option<Image<'static>> {
    if let Ok(icon) = image::load_from_memory_with_format(bytes, image::ImageFormat::Png) {
        let rgba = icon.to_rgba8();
        let (width, height) = rgba.dimensions();
        Some(Image::new_owned(rgba.into_raw(), width, height))
    } else {
        None
    }
}

fn fallback_icon() -> Image<'static> {
    let size = 32usize;
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let distance = (dx * dx + dy * dy).sqrt();
            let inside = distance <= 14.0;
            if inside {
                rgba.extend_from_slice(&[237, 18, 43, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    Image::new_owned(rgba, size as u32, size as u32)
}
