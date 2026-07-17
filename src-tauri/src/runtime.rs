use crate::{
    assets::AssetManager,
    audio::AudioRecorder,
    backend::{ParakeetBackend, TranscriptionBackend, TranscriptionRequest},
    clipboard::{ClipboardService, Delivery},
    platform::{
        capture_foreground, hide_overlay, is_wayland, platform_name, position_and_show_overlay,
        ForegroundTarget,
    },
    settings::SettingsStore,
    types::{
        AppError, AppResult, AppSettings, AppSnapshot, BackendStatus, DictationState, SetupStatus,
    },
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

pub struct AppRuntime {
    app: AppHandle,
    pub settings: SettingsStore,
    pub assets: Arc<AssetManager>,
    pub backend: Arc<dyn TranscriptionBackend>,
    recorder: AudioRecorder,
    clipboard: ClipboardService,
    state: RwLock<DictationState>,
    activation: ActivationGate,
    cancelled: AtomicBool,
    session: AtomicU64,
    target: Mutex<Option<ForegroundTarget>>,
}

#[derive(Default)]
struct ActivationGate {
    busy: AtomicBool,
    key_down: AtomicBool,
}

impl ActivationGate {
    fn try_begin(&self) -> bool {
        if self.key_down.swap(true, Ordering::AcqRel) {
            return false;
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.key_down.store(false, Ordering::Release);
            return false;
        }
        true
    }

    fn release_for_finish(&self) -> bool {
        self.key_down.swap(false, Ordering::AcqRel) && self.busy.load(Ordering::Acquire)
    }

    fn cancel(&self) -> bool {
        self.key_down.store(false, Ordering::Release);
        self.busy.swap(false, Ordering::AcqRel)
    }

    fn complete(&self) {
        self.key_down.store(false, Ordering::Release);
        self.busy.store(false, Ordering::Release);
    }

    fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }
}

impl AppRuntime {
    pub fn new(app: &AppHandle) -> AppResult<Arc<Self>> {
        let settings = SettingsStore::load(app)?;
        let assets = Arc::new(AssetManager::new(app)?);
        let backend: Arc<dyn TranscriptionBackend> = Arc::new(ParakeetBackend::new(assets.clone()));
        let initial = if assets.status().complete {
            DictationState::Idle {
                backend_status: BackendStatus::Stopped,
            }
        } else {
            DictationState::SetupRequired {
                detail: "Download the Vietnamese model and native runtime.".into(),
            }
        };
        Ok(Arc::new(Self {
            app: app.clone(),
            settings,
            assets,
            backend,
            recorder: AudioRecorder::new(app),
            clipboard: ClipboardService::new(),
            state: RwLock::new(initial),
            activation: ActivationGate::default(),
            cancelled: AtomicBool::new(false),
            session: AtomicU64::new(0),
            target: Mutex::new(None),
        }))
    }

    pub fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            settings: self.settings.get(),
            state: self.state.read().expect("state lock poisoned").clone(),
            backend: self.backend.descriptor(),
            setup_complete: self.assets.status().complete,
            platform: platform_name(),
            wayland: is_wayland(),
        }
    }

    pub fn setup_status(&self) -> SetupStatus {
        self.assets.status()
    }

    pub fn set_state(&self, state: DictationState) {
        *self.state.write().expect("state lock poisoned") = state.clone();
        let _ = self.app.emit("voice://state", state);
    }

    pub fn update_settings(&self, settings: AppSettings) -> AppResult<AppSnapshot> {
        self.settings.save(settings)?;
        Ok(self.snapshot())
    }

    pub fn press(self: &Arc<Self>) {
        let settings = self.settings.get();
        if !settings.enabled || !self.assets.status().complete {
            return;
        }
        if !self.activation.try_begin() {
            return;
        }
        self.cancelled.store(false, Ordering::Release);
        self.backend.reset_cancellation();
        let session = self.session.fetch_add(1, Ordering::AcqRel) + 1;
        *self.target.lock().expect("target lock poisoned") = Some(capture_foreground());
        if let Err(error) = position_and_show_overlay(&self.app) {
            self.fail(error);
            return;
        }
        if let Err(error) = self.recorder.start(settings.input_device_name.as_deref()) {
            self.fail(error);
            return;
        }
        self.set_state(DictationState::Recording { elapsed_ms: 0 });

        let backend = self.backend.clone();
        tauri::async_runtime::spawn(async move {
            let _ = backend.ensure_ready().await;
        });

        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(settings.max_recording_seconds)).await;
            if runtime.session.load(Ordering::Acquire) == session
                && runtime.activation.release_for_finish()
            {
                runtime.finish().await;
            }
        });
    }

    pub fn release(self: &Arc<Self>) {
        if !self.activation.release_for_finish() {
            return;
        }
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move { runtime.finish().await });
    }

    pub fn cancel(&self) {
        if !self.activation.cancel() {
            return;
        }
        self.cancelled.store(true, Ordering::Release);
        self.session.fetch_add(1, Ordering::AcqRel);
        self.recorder.cancel();
        self.backend.cancel();
        self.set_state(DictationState::Idle {
            backend_status: self.backend.status(),
        });
        hide_overlay(&self.app);
    }

    async fn finish(self: Arc<Self>) {
        let artifact = match self.recorder.stop() {
            Ok(artifact) => artifact,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        if self.cancelled.load(Ordering::Acquire) {
            remove_private_file(&artifact.path).await;
            return;
        }
        if self.backend.status() != BackendStatus::Ready {
            self.set_state(DictationState::Loading {
                detail: "Loading the Vietnamese model".into(),
            });
        }
        if let Err(error) = self.backend.ensure_ready().await {
            remove_private_file(&artifact.path).await;
            self.fail(error);
            return;
        }
        if self.cancelled.load(Ordering::Acquire) {
            remove_private_file(&artifact.path).await;
            return;
        }
        self.set_state(DictationState::Transcribing);
        let result = self
            .backend
            .transcribe(TranscriptionRequest {
                audio_path: &artifact.path,
                language: "vi",
            })
            .await;
        remove_private_file(&artifact.path).await;
        let result = match result {
            Ok(value) => value,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        if self.cancelled.load(Ordering::Acquire) {
            self.complete_idle();
            return;
        }
        self.set_state(DictationState::Pasting);
        let target = self
            .target
            .lock()
            .expect("target lock poisoned")
            .take()
            .unwrap_or_else(capture_foreground);
        match self
            .clipboard
            .deliver(result.text, target, self.settings.get().clipboard_restore)
            .await
        {
            Ok(Delivery::Pasted) => self.set_state(DictationState::Success),
            Ok(Delivery::Copied(reason)) => self.set_state(DictationState::Copied { reason }),
            Err(error) => {
                self.fail(error);
                return;
            }
        }
        self.activation.complete();
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(950)).await;
            hide_overlay(&runtime.app);
            runtime.set_state(DictationState::Idle {
                backend_status: runtime.backend.status(),
            });
        });
    }

    fn fail(self: &Arc<Self>, error: AppError) {
        eprintln!("Zui. Voice error [{}]: {}", error.code, error.message);
        self.recorder.cancel();
        self.activation.complete();
        self.set_state(DictationState::Error { error });
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            hide_overlay(&runtime.app);
            runtime.set_state(DictationState::Idle {
                backend_status: runtime.backend.status(),
            });
        });
    }

    fn complete_idle(&self) {
        self.activation.complete();
        self.set_state(DictationState::Idle {
            backend_status: self.backend.status(),
        });
        hide_overlay(&self.app);
    }

    pub fn start_idle_supervisor(self: &Arc<Self>) {
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let timeout = runtime.settings.get().model_idle_timeout_seconds;
                let idle = now_epoch_seconds().saturating_sub(runtime.backend.last_used());
                if !runtime.activation.is_busy()
                    && runtime.backend.status() == BackendStatus::Ready
                    && idle >= timeout
                {
                    let _ = runtime.backend.shutdown().await;
                    runtime.set_state(DictationState::Idle {
                        backend_status: BackendStatus::Stopped,
                    });
                }
            }
        });
    }
}

async fn remove_private_file(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            let _ = error;
        }
    }
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::ActivationGate;

    #[test]
    fn repeated_press_is_debounced() {
        let gate = ActivationGate::default();
        assert!(gate.try_begin());
        assert!(!gate.try_begin());
        assert!(gate.release_for_finish());
    }

    #[test]
    fn press_while_busy_does_not_latch_the_key() {
        let gate = ActivationGate::default();
        assert!(gate.try_begin());
        assert!(gate.release_for_finish());
        assert!(!gate.try_begin());
        gate.complete();
        assert!(gate.try_begin());
    }

    #[test]
    fn cancel_resets_activation() {
        let gate = ActivationGate::default();
        assert!(gate.try_begin());
        assert!(gate.cancel());
        assert!(gate.try_begin());
    }
}
