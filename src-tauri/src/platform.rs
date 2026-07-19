#[cfg(not(windows))]
use crate::hotkey::last_pointer;
use crate::types::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition};

const OVERLAY_WIDTH: f64 = 104.0;
const OVERLAY_HEIGHT: f64 = 36.0;
const ERROR_OVERLAY_WIDTH: f64 = 280.0;
const ERROR_OVERLAY_HEIGHT: f64 = 48.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForegroundTarget {
    pub native_id: isize,
}

pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var("XDG_SESSION_TYPE")
            .is_ok_and(|value| value.eq_ignore_ascii_case("wayland"))
}

pub fn platform_name() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub fn capture_foreground() -> ForegroundTarget {
    #[cfg(windows)]
    unsafe {
        ForegroundTarget {
            native_id: windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() as isize,
        }
    }
    #[cfg(not(windows))]
    ForegroundTarget { native_id: 0 }
}

pub fn target_is_current(target: ForegroundTarget) -> bool {
    #[cfg(windows)]
    unsafe {
        target.native_id != 0
            && windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() as isize
                == target.native_id
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        // Until the platform-specific active-window identity can be captured
        // reliably, copying is safer than injecting text into a different app.
        false
    }
}

pub fn position_and_show_overlay(app: &AppHandle) -> AppResult<()> {
    let window = app
        .get_webview_window("overlay")
        .ok_or_else(|| AppError::new("overlay_missing", "Overlay window was not created."))?;
    size_and_position_overlay(&window, OVERLAY_WIDTH, OVERLAY_HEIGHT)?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|e| AppError::new("overlay_clickthrough", e.to_string()))?;
    window
        .show()
        .map_err(|e| AppError::new("overlay_show", e.to_string()))?;
    Ok(())
}

pub fn prepare_overlay_error(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = size_and_position_overlay(&window, ERROR_OVERLAY_WIDTH, ERROR_OVERLAY_HEIGHT);
    }
}

fn size_and_position_overlay(
    window: &tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> AppResult<()> {
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|e| AppError::new("overlay_size", e.to_string()))?;
    let size = window
        .outer_size()
        .map_err(|e| AppError::new("overlay_size", e.to_string()))?;
    let (x, y) = overlay_anchor(size.width as i32, size.height as i32);
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| AppError::new("overlay_position", e.to_string()))
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
}

#[cfg(windows)]
fn overlay_anchor(width: i32, height: i32) -> (i32, i32) {
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::{
            ClientToScreen, GetMonitorInfoW, MonitorFromPoint, MONITORINFO,
            MONITOR_DEFAULTTONEAREST,
        },
        UI::WindowsAndMessaging::{GetCursorPos, GetGUIThreadInfo, GUITHREADINFO},
    };
    unsafe {
        let mut point = POINT { x: 120, y: 120 };
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        let has_caret = GetGUIThreadInfo(0, &mut info) != 0 && !info.hwndCaret.is_null();
        if has_caret {
            point.x = info.rcCaret.left;
            point.y = info.rcCaret.bottom;
            let _ = ClientToScreen(info.hwndCaret, &mut point);
        } else {
            let _ = GetCursorPos(&mut point);
        }
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut monitor_info: MONITORINFO = std::mem::zeroed();
        monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mut work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        if GetMonitorInfoW(monitor, &mut monitor_info) != 0 {
            work = monitor_info.rcWork;
        }
        let mut x = point.x + 16;
        let mut y = point.y + 18;
        if x + width > work.right - 12 {
            x = point.x - width - 16;
        }
        if y + height > work.bottom - 12 {
            y = point.y - height - 18;
        }
        (
            x.clamp(work.left + 12, work.right - width - 12),
            y.clamp(work.top + 12, work.bottom - height - 12),
        )
    }
}

#[cfg(not(windows))]
fn overlay_anchor(_width: i32, _height: i32) -> (i32, i32) {
    let (x, y) = last_pointer();
    ((x + 16).max(12), (y + 18).max(12))
}
