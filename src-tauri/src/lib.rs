mod assets;
mod audio;
mod backend;
mod cancellation;
mod clipboard;
mod hotkey;
mod platform;
mod runtime;
mod settings;
mod types;

use crate::{
    audio::AudioRecorder,
    hotkey::{HotkeyEvent, HotkeyService, NativeHotkeyService},
    runtime::AppRuntime,
    settings::validate_settings,
    types::{AppError, AppResult, AppSettings, AppSnapshot, SetupStatus},
};
use std::sync::{Arc, OnceLock};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};
use tauri_plugin_autostart::ManagerExt;

const AUTOSTART_ARG: &str = "--autostart";

struct ManagedState<T> {
    runtime: OnceLock<Arc<T>>,
    ready: tokio::sync::Notify,
}

impl<T> Default for ManagedState<T> {
    fn default() -> Self {
        Self {
            runtime: OnceLock::new(),
            ready: tokio::sync::Notify::new(),
        }
    }
}

impl<T> ManagedState<T> {
    fn initialize(&self, runtime: Arc<T>) -> AppResult<()> {
        self.runtime.set(runtime).map_err(|_| {
            AppError::fatal(
                "runtime_initialized",
                "The application runtime is already initialized.",
            )
        })?;
        self.ready.notify_waiters();
        Ok(())
    }

    fn require(&self) -> AppResult<Arc<T>> {
        self.runtime
            .get()
            .cloned()
            .ok_or_else(|| AppError::new("runtime_not_ready", "The application is still starting."))
    }

    async fn wait(&self) -> Arc<T> {
        loop {
            if let Some(runtime) = self.runtime.get() {
                return runtime.clone();
            }
            let notified = self.ready.notified();
            if let Some(runtime) = self.runtime.get() {
                return runtime.clone();
            }
            notified.await;
        }
    }
}

type RuntimeState = ManagedState<AppRuntime>;

fn is_autostart_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == AUTOSTART_ARG)
}

#[tauri::command]
async fn get_app_snapshot(app: AppHandle) -> AppResult<AppSnapshot> {
    Ok(app.state::<RuntimeState>().wait().await.snapshot())
}

#[tauri::command]
async fn get_setup_status(app: AppHandle) -> AppResult<SetupStatus> {
    Ok(app.state::<RuntimeState>().wait().await.setup_status())
}

#[tauri::command]
fn list_input_devices() -> AppResult<Vec<String>> {
    AudioRecorder::list_devices()
}

#[tauri::command]
fn test_microphone(
    runtime: State<'_, RuntimeState>,
    preferred_name: Option<String>,
) -> AppResult<()> {
    runtime
        .require()?
        .test_microphone(preferred_name.as_deref())
}

#[tauri::command]
fn begin_hotkey_test(runtime: State<'_, RuntimeState>) -> AppResult<()> {
    runtime.require()?.begin_hotkey_test()
}

#[tauri::command]
fn get_hotkey_test_status(runtime: State<'_, RuntimeState>) -> AppResult<bool> {
    Ok(runtime.require()?.hotkey_test_passed())
}

#[tauri::command]
fn cancel_hotkey_test(runtime: State<'_, RuntimeState>) -> AppResult<()> {
    runtime.require()?.cancel_hotkey_test();
    Ok(())
}

#[tauri::command]
fn complete_onboarding(
    runtime: State<'_, RuntimeState>,
    input_device_name: Option<String>,
) -> AppResult<AppSnapshot> {
    runtime.require()?.complete_onboarding(input_device_name)
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    runtime: State<'_, RuntimeState>,
    settings: AppSettings,
) -> AppResult<AppSnapshot> {
    let runtime = runtime.require()?;
    validate_settings(&settings)?;
    let previous = runtime.settings.get();
    let launch_at_login_changed = settings.launch_at_login != previous.launch_at_login;
    #[cfg(debug_assertions)]
    if launch_at_login_changed && settings.launch_at_login {
        return Err(AppError::new(
            "autostart_development_build",
            "Open at Login is only available in an installed release build.",
        ));
    }
    if launch_at_login_changed {
        set_autostart(&app, settings.launch_at_login)?;
    }
    match runtime.update_settings(settings) {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => {
            if launch_at_login_changed {
                let _ = set_autostart(&app, previous.launch_at_login);
            }
            Err(error)
        }
    }
}

fn set_autostart(app: &AppHandle, enabled: bool) -> AppResult<()> {
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|e| AppError::new("autostart", e.to_string()))
    } else {
        app.autolaunch()
            .disable()
            .map_err(|e| AppError::new("autostart", e.to_string()))
    }
}

#[tauri::command]
async fn download_assets(runtime: State<'_, RuntimeState>) -> AppResult<SetupStatus> {
    let runtime = runtime.require()?;
    let status = runtime.assets.download().await?;
    runtime.set_state(crate::types::DictationState::Idle {
        backend_status: crate::types::BackendStatus::Stopped,
    });
    Ok(status)
}

#[tauri::command]
fn cancel_asset_download(runtime: State<'_, RuntimeState>) -> AppResult<()> {
    runtime.require()?.assets.cancel_download();
    Ok(())
}

#[tauri::command]
async fn unload_model(runtime: State<'_, RuntimeState>) -> AppResult<AppSnapshot> {
    runtime.require()?.unload_model().await
}

#[tauri::command]
async fn retry_backend(runtime: State<'_, RuntimeState>) -> AppResult<AppSnapshot> {
    runtime.require()?.retry_backend().await
}

#[tauri::command]
fn debug_start_dictation(runtime: State<'_, RuntimeState>) -> AppResult<()> {
    runtime.require()?.press();
    Ok(())
}

#[tauri::command]
fn debug_stop_dictation(runtime: State<'_, RuntimeState>) -> AppResult<()> {
    runtime.require()?.release();
    Ok(())
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Zui. Voice", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Enable / disable", true, None::<&str>)?;
    let unload = MenuItem::with_id(app, "unload", "Unload model", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &toggle, &unload, &quit])?;
    TrayIconBuilder::with_id("zui-tray")
        .icon(tray_image())
        .tooltip("Zui. Voice — hold Right Alt to dictate")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_settings(app),
            "toggle" => {
                let state = app.state::<RuntimeState>();
                let Ok(runtime) = state.require() else {
                    return;
                };
                let mut settings = runtime.settings.get();
                settings.enabled = !settings.enabled;
                if !settings.enabled {
                    runtime.cancel();
                }
                let _ = runtime.update_settings(settings);
            }
            "unload" => {
                let state = app.state::<RuntimeState>();
                let Ok(runtime) = state.require() else {
                    return;
                };
                tauri::async_runtime::spawn(async move {
                    let _ = runtime.unload_model().await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(event, tauri::tray::TrayIconEvent::DoubleClick { .. }) {
                show_settings(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn tray_image() -> Image<'static> {
    let size = 32usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let index = (y * size + x) * 4;
            if dx * dx + dy * dy <= 14.5 * 14.5 {
                pixels[index] = 126;
                pixels[index + 1] = 108;
                pixels[index + 2] = 244;
                pixels[index + 3] = 255;
                if x > 12 && x < 19 && y > 7 && y < 20 {
                    pixels[index] = 255;
                    pixels[index + 1] = 255;
                    pixels[index + 2] = 255;
                }
            }
        }
    }
    Image::new_owned(pixels, size as u32, size as u32)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !is_autostart_launch(args.iter().map(String::as_str)) {
                show_settings(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .manage(RuntimeState::default())
        .setup(|app| {
            let runtime = AppRuntime::new(app.handle())?;
            app.state::<RuntimeState>().initialize(runtime.clone())?;
            if !is_autostart_launch(std::env::args()) {
                show_settings(app.handle());
            }
            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.set_ignore_cursor_events(true);
            }
            build_tray(app)?;

            let service = NativeHotkeyService::new();
            let (hotkey_sender, hotkey_receiver) = std::sync::mpsc::channel::<HotkeyEvent>();
            let control_runtime = runtime.clone();
            std::thread::Builder::new()
                .name("zui-dictation-control".into())
                .spawn(move || {
                    while let Ok(event) = hotkey_receiver.recv() {
                        match event {
                            HotkeyEvent::Pressed => control_runtime.press(),
                            HotkeyEvent::Released => control_runtime.release(),
                            HotkeyEvent::Cancel => control_runtime.cancel(),
                        }
                    }
                })
                .map_err(|error| AppError::fatal("control_thread", error.to_string()))?;

            let callback_runtime = runtime.clone();
            service.start(Arc::new(move |event| {
                let _ = hotkey_sender.send(event);
                let settings = callback_runtime.settings.get();
                settings.enabled
                    && callback_runtime.assets.status().complete
                    && settings.hotkey.consume
            }))?;
            runtime.start_idle_supervisor();
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            get_setup_status,
            list_input_devices,
            test_microphone,
            begin_hotkey_test,
            get_hotkey_test_status,
            cancel_hotkey_test,
            complete_onboarding,
            update_settings,
            download_assets,
            cancel_asset_download,
            unload_model,
            retry_backend,
            debug_start_dictation,
            debug_stop_dictation
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zui. Voice");
}

#[cfg(test)]
mod tests {
    use super::{is_autostart_launch, ManagedState};

    #[test]
    fn recognizes_autostart_argument() {
        assert!(is_autostart_launch(["zui-voice", "--autostart"]));
        assert!(!is_autostart_launch(["zui-voice"]));
    }

    #[test]
    fn runtime_placeholder_returns_a_structured_startup_error() {
        let state = ManagedState::<()>::default();

        assert_eq!(
            state
                .require()
                .expect_err("runtime is not initialized")
                .code,
            "runtime_not_ready"
        );
    }
}
