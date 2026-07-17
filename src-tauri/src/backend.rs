use crate::{
    assets::AssetManager,
    types::{AppError, AppResult, BackendDescriptor, BackendStatus},
};
use async_trait::async_trait;
use serde::Deserialize;
use std::{
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::Notify;

#[derive(Debug, Clone)]
pub struct TranscriptionRequest<'a> {
    pub audio_path: &'a Path,
    pub language: &'a str,
}

#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
}

#[async_trait]
pub trait TranscriptionBackend: Send + Sync {
    fn descriptor(&self) -> BackendDescriptor;
    fn status(&self) -> BackendStatus;
    fn last_used(&self) -> u64;
    async fn ensure_ready(&self) -> AppResult<()>;
    async fn health(&self) -> bool;
    async fn transcribe(&self, request: TranscriptionRequest<'_>)
        -> AppResult<TranscriptionResult>;
    fn reset_cancellation(&self);
    fn cancel(&self);
    async fn shutdown(&self) -> AppResult<()>;
}

pub struct ParakeetBackend {
    assets: Arc<AssetManager>,
    client: reqwest::Client,
    process: Mutex<Option<Child>>,
    port: Mutex<Option<u16>>,
    status: RwLock<BackendStatus>,
    ensure_lock: AsyncMutex<()>,
    cancelled: AtomicBool,
    cancel_notify: Notify,
    last_used: AtomicU64,
}

impl ParakeetBackend {
    pub fn new(assets: Arc<AssetManager>) -> Self {
        Self {
            assets,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(360))
                .build()
                .expect("valid HTTP client"),
            process: Mutex::new(None),
            port: Mutex::new(None),
            status: RwLock::new(BackendStatus::Stopped),
            ensure_lock: AsyncMutex::new(()),
            cancelled: AtomicBool::new(false),
            cancel_notify: Notify::new(),
            last_used: AtomicU64::new(now_epoch_seconds()),
        }
    }

    fn set_status(&self, value: BackendStatus) {
        *self.status.write().expect("backend status lock poisoned") = value;
    }

    fn base_url(&self) -> Option<String> {
        self.port
            .lock()
            .expect("backend port lock poisoned")
            .map(|port| format!("http://127.0.0.1:{port}"))
    }

    fn process_is_alive(&self) -> bool {
        let mut guard = self.process.lock().expect("backend process lock poisoned");
        guard
            .as_mut()
            .is_some_and(|process| process.try_wait().ok().flatten().is_none())
    }

    fn spawn_server(&self) -> AppResult<()> {
        let paths = self.assets.resolve_paths().ok_or_else(|| {
            AppError::new(
                "assets_missing",
                "Parakeet runtime or Vietnamese model is missing.",
            )
        })?;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| AppError::new("backend_port", e.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| AppError::new("backend_port", e.to_string()))?
            .port();
        drop(listener);

        let mut command = Command::new(&paths.server);
        command
            .arg("--model")
            .arg(&paths.model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command
            .spawn()
            .map_err(|e| AppError::new("backend_spawn", e.to_string()))?;
        *self.process.lock().expect("backend process lock poisoned") = Some(child);
        *self.port.lock().expect("backend port lock poisoned") = Some(port);
        Ok(())
    }
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
}

#[async_trait]
impl TranscriptionBackend for ParakeetBackend {
    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor::default()
    }

    fn status(&self) -> BackendStatus {
        self.status
            .read()
            .expect("backend status lock poisoned")
            .clone()
    }

    fn last_used(&self) -> u64 {
        self.last_used.load(Ordering::Relaxed)
    }

    async fn ensure_ready(&self) -> AppResult<()> {
        let _guard = self.ensure_lock.lock().await;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(AppError::new("cancelled", "Dictation was cancelled."));
        }
        if self.health().await {
            self.set_status(BackendStatus::Ready);
            return Ok(());
        }
        self.set_status(BackendStatus::Loading);
        self.shutdown().await?;
        self.spawn_server()?;
        let started = tokio::time::Instant::now();
        while started.elapsed() < Duration::from_secs(120) {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(AppError::new("cancelled", "Dictation was cancelled."));
            }
            if !self.process_is_alive() {
                self.set_status(BackendStatus::Error);
                return Err(AppError::new(
                    "backend_exited",
                    "Parakeet stopped while loading the model.",
                ));
            }
            if self.health().await {
                self.set_status(BackendStatus::Ready);
                return Ok(());
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                _ = self.cancel_notify.notified() => {
                    return Err(AppError::new("cancelled", "Dictation was cancelled."));
                }
            }
        }
        self.set_status(BackendStatus::Error);
        Err(AppError::new(
            "backend_timeout",
            "Parakeet did not become ready within two minutes.",
        ))
    }

    async fn health(&self) -> bool {
        if !self.process_is_alive() {
            return false;
        }
        let Some(base) = self.base_url() else {
            return false;
        };
        self.client
            .get(format!("{base}/health"))
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    async fn transcribe(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> AppResult<TranscriptionResult> {
        self.ensure_ready().await?;
        self.last_used.store(now_epoch_seconds(), Ordering::Relaxed);
        let audio = tokio::fs::read(request.audio_path)
            .await
            .map_err(|e| AppError::new("audio_read", e.to_string()))?;
        let part = reqwest::multipart::Part::bytes(audio)
            .file_name("recording.wav")
            .mime_str("audio/wav")
            .map_err(|e| AppError::new("transcription_request", e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "parakeet")
            .text("language", request.language.to_string())
            .text("response_format", "json");
        let base = self
            .base_url()
            .ok_or_else(|| AppError::new("backend_unavailable", "Parakeet is not running."))?;
        let request = self
            .client
            .post(format!("{base}/v1/audio/transcriptions"))
            .multipart(form)
            .send();
        let response = tokio::select! {
            response = request => response,
            _ = self.cancel_notify.notified() => {
                return Err(AppError::new("cancelled", "Dictation was cancelled."));
            }
        }
        .map_err(|e| AppError::new("transcription_failed", e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::new("transcription_failed", e.to_string()))?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(AppError::new("cancelled", "Dictation was cancelled."));
        }
        let result: TranscriptionResponse = response
            .json()
            .await
            .map_err(|e| AppError::new("transcription_response", e.to_string()))?;
        let text = result.text.trim().to_string();
        if text.is_empty() {
            return Err(AppError::new("no_speech", "No speech was detected."));
        }
        self.last_used.store(now_epoch_seconds(), Ordering::Relaxed);
        Ok(TranscriptionResult { text })
    }

    fn reset_cancellation(&self) {
        self.cancelled.store(false, Ordering::Release);
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.cancel_notify.notify_waiters();
    }

    async fn shutdown(&self) -> AppResult<()> {
        let child = self
            .process
            .lock()
            .expect("backend process lock poisoned")
            .take();
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
        *self.port.lock().expect("backend port lock poisoned") = None;
        self.set_status(BackendStatus::Stopped);
        Ok(())
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
    use super::TranscriptionResponse;

    #[test]
    fn parses_openai_compatible_response() {
        let response: TranscriptionResponse =
            serde_json::from_str(r#"{"text":"Xin chào Việt Nam"}"#).expect("valid response");
        assert_eq!(response.text, "Xin chào Việt Nam");
    }
}
