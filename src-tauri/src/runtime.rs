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
        CURRENT_ONBOARDING_VERSION,
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
    transition: Mutex<()>,
    activation: ActivationGate,
    model_unloading: AtomicBool,
    microphone_verified: AtomicBool,
    hotkey_probe_armed: AtomicBool,
    hotkey_verified: AtomicBool,
    next_session: AtomicU64,
    current_session: AtomicU64,
    target: Mutex<Option<(u64, ForegroundTarget)>>,
}

#[derive(Default)]
struct ActivationGate {
    busy: AtomicBool,
    key_down: AtomicBool,
    owner: AtomicU64,
}

impl ActivationGate {
    fn try_begin(&self, session: u64) -> bool {
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
        self.owner.store(session, Ordering::Release);
        true
    }

    fn release_for_finish(&self) -> Option<u64> {
        if !self.key_down.swap(false, Ordering::AcqRel) {
            return None;
        }
        let owner = self.owner.load(Ordering::Acquire);
        (owner != 0 && self.busy.load(Ordering::Acquire)).then_some(owner)
    }

    fn release_session_for_finish(&self, session: u64) -> bool {
        self.owner.load(Ordering::Acquire) == session
            && self.key_down.swap(false, Ordering::AcqRel)
            && self.owner.load(Ordering::Acquire) == session
            && self.busy.load(Ordering::Acquire)
    }

    fn cancel(&self) -> Option<u64> {
        let owner = self.owner.swap(0, Ordering::AcqRel);
        if owner == 0 {
            return None;
        }
        self.key_down.store(false, Ordering::Release);
        self.busy.store(false, Ordering::Release);
        Some(owner)
    }

    fn complete(&self, session: u64) -> bool {
        if self
            .owner
            .compare_exchange(session, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.key_down.store(false, Ordering::Release);
        self.busy.store(false, Ordering::Release);
        true
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
            transition: Mutex::new(()),
            activation: ActivationGate::default(),
            model_unloading: AtomicBool::new(false),
            microphone_verified: AtomicBool::new(false),
            hotkey_probe_armed: AtomicBool::new(false),
            hotkey_verified: AtomicBool::new(false),
            next_session: AtomicU64::new(0),
            current_session: AtomicU64::new(0),
            target: Mutex::new(None),
        }))
    }

    pub fn snapshot(&self) -> AppSnapshot {
        let settings = self.settings.get();
        AppSnapshot {
            onboarding_complete: settings.onboarding_version >= CURRENT_ONBOARDING_VERSION,
            settings,
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
        let should_cancel = self.settings.get().enabled && !settings.enabled;
        self.settings.save(settings)?;
        if should_cancel {
            self.cancel();
        }
        Ok(self.snapshot())
    }

    pub fn test_microphone(&self, preferred_name: Option<&str>) -> AppResult<()> {
        let _transition = self.transition.lock().expect("transition lock poisoned");
        if self.activation.is_busy() {
            return Err(AppError::new(
                "dictation_active",
                "Finish the current dictation before testing the microphone.",
            ));
        }
        self.microphone_verified.store(false, Ordering::Release);
        let session = self.next_session.fetch_add(1, Ordering::AcqRel) + 1;
        self.recorder.start(session, preferred_name, 1)?;
        std::thread::sleep(Duration::from_millis(400));
        self.recorder.finish_test(session)?;
        self.microphone_verified.store(true, Ordering::Release);
        Ok(())
    }

    pub fn begin_hotkey_test(&self) -> AppResult<()> {
        if self.activation.is_busy() {
            return Err(AppError::new(
                "dictation_active",
                "Finish the current dictation before testing the shortcut.",
            ));
        }
        self.hotkey_verified.store(false, Ordering::Release);
        self.hotkey_probe_armed.store(true, Ordering::Release);
        Ok(())
    }

    pub fn hotkey_test_passed(&self) -> bool {
        self.hotkey_verified.load(Ordering::Acquire)
    }

    pub fn cancel_hotkey_test(&self) {
        self.hotkey_probe_armed.store(false, Ordering::Release);
    }

    pub fn complete_onboarding(&self, input_device_name: Option<String>) -> AppResult<AppSnapshot> {
        if !self.assets.status().complete {
            return Err(AppError::new(
                "setup_incomplete",
                "Install the speech engine and Vietnamese model first.",
            ));
        }
        if !self.microphone_verified.load(Ordering::Acquire) {
            return Err(AppError::new(
                "microphone_not_verified",
                "Test microphone access before completing setup.",
            ));
        }
        if self.backend.status() != BackendStatus::Ready {
            return Err(AppError::new(
                "backend_not_verified",
                "Verify the local speech engine before completing setup.",
            ));
        }
        let mut settings = self.settings.get();
        settings.input_device_name = input_device_name;
        settings.onboarding_version = CURRENT_ONBOARDING_VERSION;
        self.update_settings(settings)
    }

    pub async fn unload_model(&self) -> AppResult<AppSnapshot> {
        self.model_unloading.store(true, Ordering::Release);
        {
            let _transition = self.transition.lock().expect("transition lock poisoned");
            self.current_session.store(0, Ordering::Release);
            let _ = self.activation.cancel();
            self.recorder.cancel();
            self.backend.cancel();
            self.target.lock().expect("target lock poisoned").take();
            hide_overlay(&self.app);
        }
        let shutdown = self.backend.shutdown().await;
        {
            let _transition = self.transition.lock().expect("transition lock poisoned");
            self.set_state(DictationState::Idle {
                backend_status: self.backend.status(),
            });
            self.model_unloading.store(false, Ordering::Release);
        }
        shutdown?;
        Ok(self.snapshot())
    }

    pub async fn retry_backend(&self) -> AppResult<AppSnapshot> {
        self.backend.ensure_ready().await?;
        let _transition = self.transition.lock().expect("transition lock poisoned");
        if !self.activation.is_busy() {
            self.set_state(DictationState::Idle {
                backend_status: self.backend.status(),
            });
        }
        Ok(self.snapshot())
    }

    pub fn press(self: &Arc<Self>) {
        if self.hotkey_probe_armed.swap(false, Ordering::AcqRel) {
            self.hotkey_verified.store(true, Ordering::Release);
            let _ = self.app.emit("voice://hotkey-test", ());
            return;
        }
        let _transition = self.transition.lock().expect("transition lock poisoned");
        let settings = self.settings.get();
        if self.model_unloading.load(Ordering::Acquire)
            || !settings.enabled
            || !self.assets.status().complete
            || settings.onboarding_version < CURRENT_ONBOARDING_VERSION
        {
            return;
        }
        let session = self.next_session.fetch_add(1, Ordering::AcqRel) + 1;
        if !self.activation.try_begin(session) {
            return;
        }
        self.current_session.store(session, Ordering::Release);
        *self.target.lock().expect("target lock poisoned") = Some((session, capture_foreground()));
        if let Err(error) = position_and_show_overlay(&self.app) {
            self.fail_locked(session, error);
            return;
        }
        if let Err(error) = self.recorder.start(
            session,
            settings.input_device_name.as_deref(),
            settings.max_recording_seconds,
        ) {
            self.fail_locked(session, error);
            return;
        }
        self.set_state(DictationState::Recording { elapsed_ms: 0 });

        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = runtime.backend.ensure_ready().await;
            let _transition = runtime.transition.lock().expect("transition lock poisoned");
            if !runtime.activation.is_busy() && !runtime.model_unloading.load(Ordering::Acquire) {
                runtime.set_state(DictationState::Idle {
                    backend_status: runtime.backend.status(),
                });
            }
        });

        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(settings.max_recording_seconds)).await;
            if runtime.is_current(session) && runtime.activation.release_session_for_finish(session)
            {
                runtime.finish(session).await;
            }
        });
    }

    pub fn release(self: &Arc<Self>) {
        let Some(session) = self.activation.release_for_finish() else {
            return;
        };
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move { runtime.finish(session).await });
    }

    pub fn cancel(&self) {
        let _transition = self.transition.lock().expect("transition lock poisoned");
        if self.activation.cancel().is_none() {
            return;
        }
        self.current_session.store(0, Ordering::Release);
        self.recorder.cancel();
        self.backend.cancel();
        self.target.lock().expect("target lock poisoned").take();
        self.set_state(DictationState::Idle {
            backend_status: self.backend.status(),
        });
        hide_overlay(&self.app);
    }

    async fn finish(self: Arc<Self>, session: u64) {
        if !self.is_current(session) {
            return;
        }
        let artifact = match self.recorder.stop(session) {
            Ok(artifact) => artifact,
            Err(error) => {
                if self.is_current(session) {
                    self.fail(session, error);
                }
                return;
            }
        };
        if !self.is_current(session) {
            remove_private_file(&artifact.path).await;
            return;
        }
        if self.backend.status() != BackendStatus::Ready
            && !self.set_state_for(
                session,
                DictationState::Loading {
                    detail: "Loading the Vietnamese model".into(),
                },
            )
        {
            remove_private_file(&artifact.path).await;
            return;
        }
        if let Err(error) = self.backend.ensure_ready().await {
            remove_private_file(&artifact.path).await;
            if self.is_current(session) {
                self.fail(session, error);
            }
            return;
        }
        if !self.is_current(session) {
            remove_private_file(&artifact.path).await;
            return;
        }
        if !self.set_state_for(session, DictationState::Transcribing) {
            remove_private_file(&artifact.path).await;
            return;
        }
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
                if self.is_current(session) {
                    self.fail(session, error);
                }
                return;
            }
        };
        if !self.is_current(session) {
            return;
        }
        if !self.set_state_for(session, DictationState::Pasting) {
            return;
        }
        let Some(target) = self.take_target(session) else {
            return;
        };
        let delivery = match self
            .clipboard
            .deliver(result.text, target, self.settings.get().clipboard_restore)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                if self.is_current(session) {
                    self.fail(session, error);
                }
                return;
            }
        };
        {
            let _transition = self.transition.lock().expect("transition lock poisoned");
            if !self.is_current(session) {
                return;
            }
            match delivery {
                Delivery::Pasted => self.set_state(DictationState::Success),
                Delivery::Copied(reason) => self.set_state(DictationState::Copied { reason }),
            }
            if !self.activation.complete(session) {
                return;
            }
        }
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(950)).await;
            let _transition = runtime.transition.lock().expect("transition lock poisoned");
            if runtime.is_current(session) {
                hide_overlay(&runtime.app);
                runtime.set_state(DictationState::Idle {
                    backend_status: runtime.backend.status(),
                });
            }
        });
    }

    fn fail(self: &Arc<Self>, session: u64, error: AppError) {
        let _transition = self.transition.lock().expect("transition lock poisoned");
        self.fail_locked(session, error);
    }

    fn fail_locked(self: &Arc<Self>, session: u64, error: AppError) {
        if !self.is_current(session) {
            return;
        }
        self.recorder.cancel_session(session);
        self.take_target(session);
        eprintln!("Zui. Voice error [{}]: {}", error.code, error.message);
        self.set_state(DictationState::Error { error });
        if !self.activation.complete(session) {
            return;
        }
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let _transition = runtime.transition.lock().expect("transition lock poisoned");
            if runtime.is_current(session) {
                hide_overlay(&runtime.app);
                runtime.set_state(DictationState::Idle {
                    backend_status: runtime.backend.status(),
                });
            }
        });
    }

    fn is_current(&self, session: u64) -> bool {
        self.current_session.load(Ordering::Acquire) == session
    }

    fn set_state_for(&self, session: u64, state: DictationState) -> bool {
        let _transition = self.transition.lock().expect("transition lock poisoned");
        if !self.is_current(session) {
            return false;
        }
        self.set_state(state);
        true
    }

    fn take_target(&self, session: u64) -> Option<ForegroundTarget> {
        let mut target = self.target.lock().expect("target lock poisoned");
        if target.as_ref().is_some_and(|(owner, _)| *owner == session) {
            target.take().map(|(_, target)| target)
        } else {
            None
        }
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
                    let session = runtime.current_session.load(Ordering::Acquire);
                    let _ = runtime.backend.shutdown().await;
                    let _transition = runtime.transition.lock().expect("transition lock poisoned");
                    if runtime.current_session.load(Ordering::Acquire) == session
                        && !runtime.activation.is_busy()
                    {
                        runtime.set_state(DictationState::Idle {
                            backend_status: runtime.backend.status(),
                        });
                    }
                }
            }
        });
    }
}

async fn remove_private_file(path: &Path) {
    for attempt in 0..3 {
        match tokio::fs::remove_file(path).await {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) if attempt == 2 => {
                eprintln!("Zui. Voice could not remove private audio: {error}");
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
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
        assert!(gate.try_begin(1));
        assert!(!gate.try_begin(2));
        assert_eq!(gate.release_for_finish(), Some(1));
    }

    #[test]
    fn press_while_busy_does_not_latch_the_key() {
        let gate = ActivationGate::default();
        assert!(gate.try_begin(1));
        assert_eq!(gate.release_for_finish(), Some(1));
        assert!(!gate.try_begin(2));
        assert!(gate.complete(1));
        assert!(gate.try_begin(3));
    }

    #[test]
    fn cancel_resets_activation() {
        let gate = ActivationGate::default();
        assert!(gate.try_begin(1));
        assert_eq!(gate.cancel(), Some(1));
        assert!(gate.try_begin(2));
    }

    #[test]
    fn stale_completion_cannot_release_a_new_activation() {
        let gate = ActivationGate::default();
        assert!(gate.try_begin(1));
        assert_eq!(gate.cancel(), Some(1));
        assert!(gate.try_begin(2));
        assert!(!gate.complete(1));
        assert!(gate.is_busy());
    }
}
