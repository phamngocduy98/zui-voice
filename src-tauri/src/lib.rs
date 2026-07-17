mod assets;
mod audio;
mod backend;
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
    types::{AppError, AppResult, AppSettings, AppSnapshot, SetupStatus},
};
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
fn get_app_snapshot(runtime: State<'_, Arc<AppRuntime>>) -> AppSnapshot {
    runtime.snapshot()
}

#[tauri::command]
fn get_setup_status(runtime: State<'_, Arc<AppRuntime>>) -> SetupStatus {
    runtime.setup_status()
}

#[tauri::command]
fn list_input_devices() -> AppResult<Vec<String>> {
    AudioRecorder::list_devices()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    settings: AppSettings,
) -> AppResult<AppSnapshot> {
    if settings.launch_at_login {
        app.autolaunch()
            .enable()
            .map_err(|e| AppError::new("autostart", e.to_string()))?;
    } else if app.autolaunch().is_enabled().unwrap_or(false) {
        app.autolaunch()
            .disable()
            .map_err(|e| AppError::new("autostart", e.to_string()))?;
    }
    runtime.update_settings(settings)
}

#[tauri::command]
async fn download_assets(runtime: State<'_, Arc<AppRuntime>>) -> AppResult<SetupStatus> {
    let status = runtime.assets.download().await?;
    runtime.set_state(crate::types::DictationState::Idle {
        backend_status: crate::types::BackendStatus::Stopped,
    });
    Ok(status)
}

#[tauri::command]
fn cancel_asset_download(runtime: State<'_, Arc<AppRuntime>>) {
    runtime.assets.cancel_download();
}

#[tauri::command]
async fn unload_model(runtime: State<'_, Arc<AppRuntime>>) -> AppResult<()> {
    runtime.backend.shutdown().await
}

#[tauri::command]
async fn retry_backend(runtime: State<'_, Arc<AppRuntime>>) -> AppResult<()> {
    runtime.backend.reset_cancellation();
    runtime.backend.ensure_ready().await
}

#[tauri::command]
fn debug_start_dictation(runtime: State<'_, Arc<AppRuntime>>) {
    runtime.inner().press();
}

#[tauri::command]
fn debug_stop_dictation(runtime: State<'_, Arc<AppRuntime>>) {
    runtime.inner().release();
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
                let runtime = app.state::<Arc<AppRuntime>>();
                let mut settings = runtime.settings.get();
                settings.enabled = !settings.enabled;
                if !settings.enabled {
                    runtime.cancel();
                }
                let _ = runtime.update_settings(settings);
            }
            "unload" => {
                let backend = app.state::<Arc<AppRuntime>>().backend.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = backend.shutdown().await;
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
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_settings(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let runtime = AppRuntime::new(app.handle())?;
            app.manage(runtime.clone());
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
                callback_runtime.settings.get().hotkey.consume
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
