#[cfg(windows)]
use tauri::WebviewWindow;

#[cfg(windows)]
#[repr(C)]
struct AccentPolicy {
    accent_state: i32,
    accent_flags: i32,
    gradient_color: u32,
    animation_id: i32,
}

#[cfg(windows)]
#[repr(C)]
struct WindowCompositionAttribData {
    attribute: i32,
    data: *mut std::ffi::c_void,
    size_of_data: usize,
}

#[cfg(windows)]
const WCA_ACCENT_POLICY: i32 = 19;
#[cfg(windows)]
const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;
#[cfg(windows)]
const ACCENT_ENABLE_BLURBEHIND: i32 = 3;
#[cfg(windows)]
const GWL_STYLE: i32 = -16;
#[cfg(windows)]
const WS_CAPTION: isize = 0x00c00000;
#[cfg(windows)]
const WS_SYSMENU: isize = 0x00080000;
#[cfg(windows)]
const WS_THICKFRAME: isize = 0x00040000;
#[cfg(windows)]
const WS_MINIMIZEBOX: isize = 0x00020000;
#[cfg(windows)]
const WS_MAXIMIZEBOX: isize = 0x00010000;
#[cfg(windows)]
const SWP_NOSIZE: u32 = 0x0001;
#[cfg(windows)]
const SWP_NOMOVE: u32 = 0x0002;
#[cfg(windows)]
const SWP_NOZORDER: u32 = 0x0004;
#[cfg(windows)]
const SWP_NOACTIVATE: u32 = 0x0010;
#[cfg(windows)]
const SWP_FRAMECHANGED: u32 = 0x0020;
#[cfg(windows)]
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
#[cfg(windows)]
const DWMWCP_ROUND: i32 = 2;
#[cfg(windows)]
const POPOVER_CORNER_RADIUS_LOGICAL_PX: f64 = 8.0;

#[cfg(windows)]
type SetWindowCompositionAttributeFn =
    unsafe extern "system" fn(isize, *mut WindowCompositionAttribData) -> i32;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const u8) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *const std::ffi::c_void;
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new_long: isize) -> isize;
    fn SetWindowPos(
        hwnd: isize,
        hwnd_insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn SetWindowRgn(hwnd: isize, region: isize, redraw: i32) -> i32;
}

#[cfg(windows)]
#[link(name = "gdi32")]
extern "system" {
    fn CreateRoundRectRgn(
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        ellipse_width: i32,
        ellipse_height: i32,
    ) -> isize;
    fn DeleteObject(object: isize) -> i32;
}

#[cfg(windows)]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        attribute: u32,
        value: *const std::ffi::c_void,
        size: u32,
    ) -> i32;
}

#[cfg(windows)]
pub fn apply_popover_frost(window: &WebviewWindow) {
    let Some(set_window_composition_attribute) = composition_api() else {
        return;
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = hwnd.0 as isize;

    strip_native_frame(hwnd);
    apply_dwm_corner_preference(hwnd);
    apply_rounded_region(window, hwnd);

    let mut policy = AccentPolicy {
        accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
        accent_flags: 2,
        // ABGR: alpha first, then blue/green/red. Keep this fairly transparent;
        // CSS supplies the readable tint while Windows supplies the real blur.
        gradient_color: 0x26160f0e,
        animation_id: 0,
    };

    let mut data = WindowCompositionAttribData {
        attribute: WCA_ACCENT_POLICY,
        data: std::ptr::addr_of_mut!(policy).cast(),
        size_of_data: std::mem::size_of::<AccentPolicy>(),
    };

    // SAFETY: hwnd is valid, and data points to a correctly-sized accent policy
    // for the duration of the call.
    let applied = unsafe { set_window_composition_attribute(hwnd, &mut data) } != 0;
    if !applied {
        let mut fallback_policy = AccentPolicy {
            accent_state: ACCENT_ENABLE_BLURBEHIND,
            accent_flags: 2,
            gradient_color: 0x26160f0e,
            animation_id: 0,
        };
        let mut fallback_data = WindowCompositionAttribData {
            attribute: WCA_ACCENT_POLICY,
            data: std::ptr::addr_of_mut!(fallback_policy).cast(),
            size_of_data: std::mem::size_of::<AccentPolicy>(),
        };
        let _ = unsafe { set_window_composition_attribute(hwnd, &mut fallback_data) };
    }

    strip_native_frame(hwnd);
    apply_dwm_corner_preference(hwnd);
    apply_rounded_region(window, hwnd);
}

#[cfg(windows)]
fn strip_native_frame(hwnd: isize) {
    let frame_bits = WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;

    // SAFETY: hwnd is a live top-level window handle owned by this process. The
    // style update only clears standard non-client frame bits, then asks Windows
    // to recalculate the frame without moving, resizing, activating, or z-ordering.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if style != 0 {
            let _ = SetWindowLongPtrW(hwnd, GWL_STYLE, style & !frame_bits);
        }
        let _ = SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(windows)]
pub fn apply_popover_shape(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = hwnd.0 as isize;
    strip_native_frame(hwnd);
    apply_dwm_corner_preference(hwnd);
    apply_rounded_region(window, hwnd);
}

#[cfg(windows)]
fn apply_dwm_corner_preference(hwnd: isize) {
    let corner_preference = DWMWCP_ROUND;

    // SAFETY: hwnd is valid, and the payload is a fixed-size DWM enum value.
    // Older Windows versions ignore the unsupported attribute.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::addr_of!(corner_preference).cast(),
            std::mem::size_of::<i32>() as u32,
        );
    }
}

#[cfg(windows)]
fn apply_rounded_region(window: &WebviewWindow, hwnd: isize) {
    let Ok(size) = window.outer_size() else {
        return;
    };

    let width = size.width as i32;
    let height = size.height as i32;
    if width <= 0 || height <= 0 {
        return;
    }
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let corner_diameter = ((POPOVER_CORNER_RADIUS_LOGICAL_PX * scale_factor).round() as i32)
        .max(1)
        * 2;

    // SAFETY: hwnd is valid, and CreateRoundRectRgn returns a GDI region that
    // becomes owned by Windows after SetWindowRgn succeeds.
    unsafe {
        let region = CreateRoundRectRgn(
            0,
            0,
            width + 1,
            height + 1,
            // CreateRoundRectRgn wants a physical-pixel ellipse diameter.
            corner_diameter,
            corner_diameter,
        );
        if region == 0 {
            return;
        }

        if SetWindowRgn(hwnd, region, 1) == 0 {
            let _ = DeleteObject(region);
        }
    }
}

#[cfg(windows)]
fn composition_api() -> Option<SetWindowCompositionAttributeFn> {
    // SAFETY: static NUL-terminated strings are valid C strings. The loaded
    // module is owned by the process, and the symbol type matches the Windows
    // private API signature used for accent policies.
    let module = unsafe { LoadLibraryA(c"user32.dll".as_ptr().cast()) };
    if module == 0 {
        return None;
    }

    let proc = unsafe { GetProcAddress(module, c"SetWindowCompositionAttribute".as_ptr().cast()) };
    if proc.is_null() {
        return None;
    }

    Some(unsafe {
        std::mem::transmute::<*const std::ffi::c_void, SetWindowCompositionAttributeFn>(proc)
    })
}

#[cfg(not(windows))]
pub fn apply_popover_frost(_window: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn apply_popover_shape(_window: &tauri::WebviewWindow) {}
