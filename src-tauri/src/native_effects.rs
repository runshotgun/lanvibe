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
#[repr(C)]
struct Margins {
    cx_left_width: i32,
    cx_right_width: i32,
    cy_top_height: i32,
    cy_bottom_height: i32,
}

#[cfg(windows)]
const WCA_ACCENT_POLICY: i32 = 19;
#[cfg(windows)]
const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;
#[cfg(windows)]
const ACCENT_ENABLE_BLURBEHIND: i32 = 3;

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
#[link(name = "dwmapi")]
extern "system" {
    fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
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

    let margins = Margins {
        cx_left_width: -1,
        cx_right_width: -1,
        cy_top_height: -1,
        cy_bottom_height: -1,
    };
    // SAFETY: hwnd is a live top-level window handle owned by this process, and
    // margins points to a valid, immutable MARGINS-compatible struct.
    let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };

    let mut policy = AccentPolicy {
        accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
        accent_flags: 2,
        // ABGR: alpha first, then blue/green/red. Keep this fairly transparent;
        // CSS supplies the readable tint while Windows supplies the real blur.
        gradient_color: 0x66160f0e,
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
            gradient_color: 0x66160f0e,
            animation_id: 0,
        };
        let mut fallback_data = WindowCompositionAttribData {
            attribute: WCA_ACCENT_POLICY,
            data: std::ptr::addr_of_mut!(fallback_policy).cast(),
            size_of_data: std::mem::size_of::<AccentPolicy>(),
        };
        let _ = unsafe { set_window_composition_attribute(hwnd, &mut fallback_data) };
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
