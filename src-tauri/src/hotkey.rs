use crate::types::{AppError, AppResult};
#[cfg(not(windows))]
use std::sync::atomic::AtomicI64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
    Cancel,
}

#[derive(Default)]
struct PhysicalKeyLatch(AtomicBool);

impl PhysicalKeyLatch {
    fn press(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }

    fn release(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

pub trait HotkeyService: Send + Sync {
    fn start(&self, handler: Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>) -> AppResult<()>;
}

#[cfg(not(windows))]
static POINTER_X: AtomicI64 = AtomicI64::new(120);
#[cfg(not(windows))]
static POINTER_Y: AtomicI64 = AtomicI64::new(120);

#[cfg(not(windows))]
pub fn last_pointer() -> (i32, i32) {
    (
        POINTER_X.load(Ordering::Relaxed) as i32,
        POINTER_Y.load(Ordering::Relaxed) as i32,
    )
}

pub struct NativeHotkeyService;

impl NativeHotkeyService {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
mod windows_hook {
    use super::*;
    use std::sync::OnceLock;
    use windows_sys::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        UI::{
            Input::KeyboardAndMouse::{VK_ESCAPE, VK_RMENU},
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
                HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
                WM_SYSKEYDOWN, WM_SYSKEYUP,
            },
        },
    };

    static HANDLER: OnceLock<Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>> = OnceLock::new();
    static RIGHT_ALT: PhysicalKeyLatch = PhysicalKeyLatch(AtomicBool::new(false));
    static CONSUME_RIGHT_ALT: AtomicBool = AtomicBool::new(true);

    pub fn start(handler: Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>) -> AppResult<()> {
        HANDLER.set(handler).map_err(|_| {
            AppError::new("hotkey_running", "The hotkey listener is already running.")
        })?;
        std::thread::Builder::new()
            .name("zui-hotkey".into())
            .spawn(move || unsafe {
                let hook =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(callback), std::ptr::null_mut(), 0);
                if hook.is_null() {
                    return;
                }
                let mut message: MSG = std::mem::zeroed();
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            })
            .map_err(|e| AppError::new("hotkey_start", e.to_string()))?;
        Ok(())
    }

    unsafe extern "system" fn callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let data = &*(lparam as *const KBDLLHOOKSTRUCT);
            let down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
            let up = wparam == WM_KEYUP as usize || wparam == WM_SYSKEYUP as usize;
            if data.vkCode == VK_RMENU as u32 {
                if down && RIGHT_ALT.press() {
                    if let Some(handler) = HANDLER.get() {
                        let consume = handler(HotkeyEvent::Pressed);
                        CONSUME_RIGHT_ALT.store(consume, Ordering::Release);
                    }
                } else if up && RIGHT_ALT.release() {
                    if let Some(handler) = HANDLER.get() {
                        let consume = handler(HotkeyEvent::Released);
                        CONSUME_RIGHT_ALT.store(consume, Ordering::Release);
                    }
                }
                if CONSUME_RIGHT_ALT.load(Ordering::Acquire) {
                    return 1;
                }
            }
            if data.vkCode == VK_ESCAPE as u32 && down {
                if let Some(handler) = HANDLER.get() {
                    let _ = handler(HotkeyEvent::Cancel);
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }
}

#[cfg(not(windows))]
mod portable_hook {
    use super::*;
    use rdev::{listen, EventType, Key};

    pub fn start(handler: Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>) -> AppResult<()> {
        let right_alt = Arc::new(PhysicalKeyLatch::default());
        std::thread::Builder::new()
            .name("zui-hotkey".into())
            .spawn(move || {
                let _ = listen(move |event| match event.event_type {
                    EventType::KeyPress(Key::AltGr) if right_alt.press() => {
                        let _ = handler(HotkeyEvent::Pressed);
                    }
                    EventType::KeyRelease(Key::AltGr) if right_alt.release() => {
                        let _ = handler(HotkeyEvent::Released);
                    }
                    EventType::KeyPress(Key::Escape) => {
                        let _ = handler(HotkeyEvent::Cancel);
                    }
                    EventType::MouseMove { x, y } => {
                        POINTER_X.store(x as i64, Ordering::Relaxed);
                        POINTER_Y.store(y as i64, Ordering::Relaxed);
                    }
                    _ => {}
                });
            })
            .map_err(|e| AppError::new("hotkey_start", e.to_string()))?;
        Ok(())
    }
}

impl HotkeyService for NativeHotkeyService {
    fn start(&self, handler: Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>) -> AppResult<()> {
        #[cfg(windows)]
        return windows_hook::start(handler);
        #[cfg(not(windows))]
        return portable_hook::start(handler);
    }
}

#[cfg(test)]
mod tests {
    use super::PhysicalKeyLatch;

    #[test]
    fn emits_one_press_until_physical_release() {
        let key = PhysicalKeyLatch::default();
        assert!(key.press());
        assert!(!key.press());
        assert!(!key.press());
        assert!(key.release());
        assert!(!key.release());
        assert!(key.press());
    }
}
