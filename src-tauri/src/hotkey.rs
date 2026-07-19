use crate::types::{AppError, AppResult};
#[cfg(not(windows))]
use std::sync::atomic::AtomicI64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed(HoldKey),
    Released(HoldKey),
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldKey {
    RightAlt,
    RightControl,
    F8,
    F9,
}

impl HoldKey {
    pub const fn id(self) -> &'static str {
        match self {
            Self::RightAlt => "RightAlt",
            Self::RightControl => "RightControl",
            Self::F8 => "F8",
            Self::F9 => "F9",
        }
    }
}

pub fn is_supported_hold_key(value: &str) -> bool {
    [
        HoldKey::RightAlt,
        HoldKey::RightControl,
        HoldKey::F8,
        HoldKey::F9,
    ]
    .iter()
    .any(|key| key.id() == value)
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
            Input::KeyboardAndMouse::{
                VK_CONTROL, VK_ESCAPE, VK_F8, VK_F9, VK_MENU, VK_RCONTROL, VK_RMENU,
            },
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
                UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
                WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
            },
        },
    };

    static HANDLER: OnceLock<Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>> = OnceLock::new();
    struct KeyState {
        latch: PhysicalKeyLatch,
        consume: AtomicBool,
    }

    impl KeyState {
        const fn new() -> Self {
            Self {
                latch: PhysicalKeyLatch(AtomicBool::new(false)),
                consume: AtomicBool::new(false),
            }
        }
    }

    static RIGHT_ALT: KeyState = KeyState::new();
    static RIGHT_CONTROL: KeyState = KeyState::new();
    static F8: KeyState = KeyState::new();
    static F9: KeyState = KeyState::new();

    pub fn start(handler: Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>) -> AppResult<()> {
        HANDLER.set(handler).map_err(|_| {
            AppError::new("hotkey_running", "The hotkey listener is already running.")
        })?;
        let (startup_sender, startup_receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("zui-hotkey".into())
            .spawn(move || unsafe {
                let hook =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(callback), std::ptr::null_mut(), 0);
                if hook.is_null() {
                    let _ = startup_sender.send(Err(std::io::Error::last_os_error().to_string()));
                    return;
                }
                let _ = startup_sender.send(Ok(()));
                let mut message: MSG = std::mem::zeroed();
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = UnhookWindowsHookEx(hook);
            })
            .map_err(|e| AppError::new("hotkey_start", e.to_string()))?;
        match startup_receiver.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(AppError::new("hotkey_start", error)),
            Err(error) => Err(AppError::new(
                "hotkey_start",
                format!("Hotkey listener did not start: {error}"),
            )),
        }
    }

    unsafe extern "system" fn callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let data = &*(lparam as *const KBDLLHOOKSTRUCT);
            let down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
            let up = wparam == WM_KEYUP as usize || wparam == WM_SYSKEYUP as usize;
            let extended = data.flags & 0x01 != 0;
            let hold_key =
                if data.vkCode == VK_RMENU as u32 || (data.vkCode == VK_MENU as u32 && extended) {
                    Some((HoldKey::RightAlt, &RIGHT_ALT))
                } else if data.vkCode == VK_RCONTROL as u32
                    || (data.vkCode == VK_CONTROL as u32 && extended)
                {
                    Some((HoldKey::RightControl, &RIGHT_CONTROL))
                } else if data.vkCode == VK_F8 as u32 {
                    Some((HoldKey::F8, &F8))
                } else if data.vkCode == VK_F9 as u32 {
                    Some((HoldKey::F9, &F9))
                } else {
                    None
                };
            if let Some((key, state)) = hold_key {
                if down && state.latch.press() {
                    if let Some(handler) = HANDLER.get() {
                        state
                            .consume
                            .store(handler(HotkeyEvent::Pressed(key)), Ordering::Release);
                    }
                } else if up && state.latch.release() {
                    if let Some(handler) = HANDLER.get() {
                        state
                            .consume
                            .store(handler(HotkeyEvent::Released(key)), Ordering::Release);
                    }
                }
                if state.consume.load(Ordering::Acquire) {
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

    fn hold_key(key: Key) -> Option<(HoldKey, usize)> {
        match key {
            Key::AltGr => Some((HoldKey::RightAlt, 0)),
            Key::ControlRight => Some((HoldKey::RightControl, 1)),
            Key::F8 => Some((HoldKey::F8, 2)),
            Key::F9 => Some((HoldKey::F9, 3)),
            _ => None,
        }
    }

    pub fn start(handler: Arc<dyn Fn(HotkeyEvent) -> bool + Send + Sync>) -> AppResult<()> {
        let latches = Arc::new([
            PhysicalKeyLatch::default(),
            PhysicalKeyLatch::default(),
            PhysicalKeyLatch::default(),
            PhysicalKeyLatch::default(),
        ]);
        let (exit_sender, exit_receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("zui-hotkey".into())
            .spawn(move || {
                let result = listen(move |event| match event.event_type {
                    EventType::KeyPress(key)
                        if hold_key(key).is_some_and(|(_, index)| latches[index].press()) =>
                    {
                        let (key, _) = hold_key(key).expect("supported key");
                        let _ = handler(HotkeyEvent::Pressed(key));
                    }
                    EventType::KeyRelease(key)
                        if hold_key(key).is_some_and(|(_, index)| latches[index].release()) =>
                    {
                        let (key, _) = hold_key(key).expect("supported key");
                        let _ = handler(HotkeyEvent::Released(key));
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
                let _ = exit_sender.send(result.map_err(|error| format!("{error:?}")));
            })
            .map_err(|e| AppError::new("hotkey_start", e.to_string()))?;
        match exit_receiver.recv_timeout(std::time::Duration::from_millis(250)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
            Ok(Err(error)) => Err(AppError::new("hotkey_start", error)),
            Ok(Ok(())) => Err(AppError::new(
                "hotkey_start",
                "The hotkey listener stopped unexpectedly.",
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(AppError::new(
                "hotkey_start",
                "The hotkey listener exited during startup.",
            )),
        }
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
