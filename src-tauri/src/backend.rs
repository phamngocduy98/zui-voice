use crate::{
    assets::AssetManager,
    cancellation::CancellationSignal,
    types::{AppError, AppResult, BackendDescriptor, BackendStatus},
};
use async_trait::async_trait;
use serde::Deserialize;
use std::{
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex as AsyncMutex;

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
    fn cancel(&self);
    async fn shutdown(&self) -> AppResult<()>;
}

pub struct ParakeetBackend {
    assets: Arc<AssetManager>,
    client: reqwest::Client,
    process: Mutex<Option<Child>>,
    #[cfg(windows)]
    process_job: Mutex<Option<windows_process::ProcessJob>>,
    port: Mutex<Option<u16>>,
    status: RwLock<BackendStatus>,
    ensure_lock: AsyncMutex<()>,
    cancellation: CancellationSignal,
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
            #[cfg(windows)]
            process_job: Mutex::new(None),
            port: Mutex::new(None),
            status: RwLock::new(BackendStatus::Stopped),
            ensure_lock: AsyncMutex::new(()),
            cancellation: CancellationSignal::default(),
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
            .is_some_and(|process| matches!(process.try_wait(), Ok(None)))
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
        #[cfg(windows)]
        let (child, process_job) = {
            let mut child = child;
            let process_job = match windows_process::ProcessJob::attach(&child) {
                Ok(job) => job,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AppError::new("backend_spawn", error.to_string()));
                }
            };
            (child, process_job)
        };
        *self.process.lock().expect("backend process lock poisoned") = Some(child);
        #[cfg(windows)]
        {
            *self
                .process_job
                .lock()
                .expect("backend process job lock poisoned") = Some(process_job);
        }
        *self.port.lock().expect("backend port lock poisoned") = Some(port);
        Ok(())
    }

    fn shutdown_process(&self) -> AppResult<()> {
        let child = self
            .process
            .lock()
            .expect("backend process lock poisoned")
            .take();
        #[cfg(windows)]
        let process_job = self
            .process_job
            .lock()
            .expect("backend process job lock poisoned")
            .take();
        let mut shutdown_error = None;
        if let Some(mut child) = child {
            let should_wait = match child.try_wait() {
                Ok(Some(_)) => false,
                Ok(None) => match child.kill() {
                    Ok(()) => true,
                    Err(error) => {
                        shutdown_error = Some(AppError::new("backend_shutdown", error.to_string()));
                        false
                    }
                },
                Err(error) => {
                    shutdown_error = Some(AppError::new("backend_shutdown", error.to_string()));
                    false
                }
            };
            #[cfg(windows)]
            drop(process_job);
            if should_wait {
                if let Err(error) = child.wait() {
                    shutdown_error = Some(AppError::new("backend_shutdown", error.to_string()));
                }
            }
        } else {
            #[cfg(windows)]
            drop(process_job);
        }
        #[cfg(windows)]
        if let Some(paths) = self.assets.resolve_paths() {
            if let Err(error) = windows_process::terminate_exact(&paths.server) {
                shutdown_error = Some(AppError::new("backend_shutdown", error.to_string()));
            }
        }
        *self.port.lock().expect("backend port lock poisoned") = None;
        self.set_status(BackendStatus::Stopped);
        shutdown_error.map_or(Ok(()), Err)
    }

    fn cancelled_error() -> AppError {
        AppError::new("cancelled", "Dictation was cancelled.")
    }

    fn cancelled_startup_error(&self) -> AppError {
        if self.status() == BackendStatus::Loading {
            if let Err(error) = self.shutdown_process() {
                self.set_status(BackendStatus::Error);
                eprintln!("Zui. Voice could not stop a cancelled backend startup: {error}");
            }
        }
        Self::cancelled_error()
    }

    async fn ensure_ready_for(&self, generation: u64) -> AppResult<()> {
        let _guard = self.ensure_lock.lock().await;
        if self.cancellation.is_cancelled(generation) {
            return Err(Self::cancelled_error());
        }
        let healthy = self
            .cancellation
            .run(generation, self.health())
            .await
            .ok_or_else(|| self.cancelled_startup_error())?;
        if healthy {
            self.set_status(BackendStatus::Ready);
            return Ok(());
        }

        self.set_status(BackendStatus::Loading);
        if let Err(error) = self.shutdown_process() {
            self.set_status(BackendStatus::Error);
            return Err(error);
        }
        if self.cancellation.is_cancelled(generation) {
            return Err(Self::cancelled_error());
        }
        if let Err(error) = self.spawn_server() {
            self.set_status(BackendStatus::Error);
            return Err(error);
        }

        let started = tokio::time::Instant::now();
        while started.elapsed() < Duration::from_secs(120) {
            if self.cancellation.is_cancelled(generation) {
                return Err(self.cancelled_startup_error());
            }
            if !self.process_is_alive() {
                self.set_status(BackendStatus::Error);
                return Err(AppError::new(
                    "backend_exited",
                    "Parakeet stopped while loading the model.",
                ));
            }
            let healthy = self
                .cancellation
                .run(generation, self.health())
                .await
                .ok_or_else(|| self.cancelled_startup_error())?;
            if healthy {
                self.set_status(BackendStatus::Ready);
                return Ok(());
            }
            self.cancellation
                .run(generation, tokio::time::sleep(Duration::from_millis(250)))
                .await
                .ok_or_else(|| self.cancelled_startup_error())?;
        }

        let shutdown_error = self.shutdown_process().err();
        self.set_status(BackendStatus::Error);
        if let Some(error) = shutdown_error {
            return Err(error);
        }
        Err(AppError::new(
            "backend_timeout",
            "Parakeet did not become ready within two minutes.",
        ))
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
        self.ensure_ready_for(self.cancellation.generation()).await
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
        let generation = self.cancellation.generation();
        self.ensure_ready_for(generation).await?;
        self.last_used.store(now_epoch_seconds(), Ordering::Relaxed);
        let audio = self
            .cancellation
            .run(generation, tokio::fs::read(request.audio_path))
            .await
            .ok_or_else(Self::cancelled_error)?
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
        let response = self
            .cancellation
            .run(generation, request)
            .await
            .ok_or_else(Self::cancelled_error)?
            .map_err(|e| AppError::new("transcription_failed", e.to_string()))?
            .error_for_status()
            .map_err(|e| AppError::new("transcription_failed", e.to_string()))?;
        let result: TranscriptionResponse = self
            .cancellation
            .run(generation, response.json())
            .await
            .ok_or_else(Self::cancelled_error)?
            .map_err(|e| AppError::new("transcription_response", e.to_string()))?;
        let text = result.text.trim().to_string();
        if text.is_empty() {
            return Err(AppError::new("no_speech", "No speech was detected."));
        }
        self.last_used.store(now_epoch_seconds(), Ordering::Relaxed);
        Ok(TranscriptionResult { text })
    }

    fn cancel(&self) {
        self.cancellation.cancel();
    }

    async fn shutdown(&self) -> AppResult<()> {
        self.cancel();
        let _guard = self.ensure_lock.lock().await;
        self.shutdown_process()
    }
}

impl Drop for ParakeetBackend {
    fn drop(&mut self) {
        if let Ok(process) = self.process.get_mut() {
            if let Some(mut child) = process.take() {
                match child.try_wait() {
                    Ok(Some(_)) => {}
                    Ok(None) if child.kill().is_ok() => {
                        let _ = child.wait();
                    }
                    _ => {}
                }
            }
        }
        #[cfg(windows)]
        if let Ok(process_job) = self.process_job.get_mut() {
            process_job.take();
        }
    }
}

#[cfg(windows)]
mod windows_process {
    use std::{
        ffi::OsString,
        io,
        mem::{size_of, zeroed},
        os::windows::{
            ffi::OsStringExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        path::Path,
        process::Child,
        ptr::null,
    };
    use windows_sys::Win32::{
        Foundation::{INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
            },
        },
    };

    pub struct ProcessJob {
        _handle: OwnedHandle,
    }

    impl ProcessJob {
        pub fn attach(child: &Child) -> io::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(null(), null());
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let job = OwnedHandle::from_raw_handle(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Self { _handle: job })
            }
        }
    }

    pub fn terminate_exact(executable: &Path) -> io::Result<()> {
        let target_name = executable.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Backend path has no filename")
        })?;
        let target_path = normalized_path(executable);
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let snapshot = OwnedHandle::from_raw_handle(snapshot);
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snapshot.as_raw_handle(), &mut entry) == 0 {
                return Err(io::Error::last_os_error());
            }

            loop {
                let name_length = entry
                    .szExeFile
                    .iter()
                    .position(|character| *character == 0)
                    .unwrap_or(entry.szExeFile.len());
                let process_name = OsString::from_wide(&entry.szExeFile[..name_length]);
                if process_name.eq_ignore_ascii_case(target_name)
                    && entry.th32ProcessID != std::process::id()
                {
                    terminate_if_path_matches(entry.th32ProcessID, &target_path)?;
                }
                if Process32NextW(snapshot.as_raw_handle(), &mut entry) == 0 {
                    break;
                }
            }
        }
        Ok(())
    }

    unsafe fn terminate_if_path_matches(process_id: u32, target_path: &str) -> io::Result<()> {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
            0,
            process_id,
        );
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = OwnedHandle::from_raw_handle(handle);
        let mut path_buffer = vec![0u16; 32_768];
        let mut path_length = path_buffer.len() as u32;
        if QueryFullProcessImageNameW(
            process.as_raw_handle(),
            0,
            path_buffer.as_mut_ptr(),
            &mut path_length,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process_path = OsString::from_wide(&path_buffer[..path_length as usize]);
        if normalized_path(Path::new(&process_path)) != target_path {
            return Ok(());
        }
        if TerminateProcess(process.as_raw_handle(), 1) == 0 {
            return Err(io::Error::last_os_error());
        }
        if WaitForSingleObject(process.as_raw_handle(), 5_000) != WAIT_OBJECT_0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Backend process did not exit within five seconds",
            ));
        }
        Ok(())
    }

    fn normalized_path(path: &Path) -> String {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_lowercase()
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
