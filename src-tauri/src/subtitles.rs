use crate::{
    backend::{ParakeetBackend, StreamingSession, StreamingTranscription},
    inference::InferenceArbiter,
    subtitle_stabilizer::SubtitleStabilizer,
    system_audio::{self, PcmFrame},
    types::{
        AppError, AppResult, SubtitlePosition, SubtitleSettings, SubtitleState, SubtitleText,
        SystemAudioCapabilities,
    },
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex, RwLock,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tauri::PhysicalPosition;
use tauri::{AppHandle, Emitter, Manager};

const PCM_QUEUE_FRAMES: usize = 250;
const FEED_SAMPLES: usize = 1_600;
const ROTATE_AFTER: Duration = Duration::from_secs(30);
const SILENCE_AFTER: Duration = Duration::from_millis(1_250);
const SILENCE_LEVEL: f32 = 0.006;

/// Owns one bounded capture/stream pair. Lifecycle operations are serialized so a concurrent
/// Start/Stop cannot leak a capture stream, worker, server stream, or visible subtitle window.
pub struct SubtitleRuntime {
    app: AppHandle,
    state: Arc<RwLock<SubtitleState>>,
    capture: Arc<Mutex<Option<Box<dyn system_audio::SystemAudioCapture>>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    lifecycle: Arc<Mutex<()>>,
    cancel: Mutex<Arc<AtomicBool>>,
    session: Arc<AtomicU64>,
    revision: Arc<AtomicU64>,
}

impl SubtitleRuntime {
    pub fn new(app: &AppHandle) -> Self {
        Self {
            app: app.clone(),
            state: Arc::new(RwLock::new(SubtitleState::Disabled)),
            capture: Arc::new(Mutex::new(None)),
            worker: Mutex::new(None),
            lifecycle: Arc::new(Mutex::new(())),
            cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            session: Arc::new(AtomicU64::new(0)),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn state(&self) -> SubtitleState {
        self.state
            .read()
            .expect("subtitle state lock poisoned")
            .clone()
    }

    pub fn capabilities(&self) -> SystemAudioCapabilities {
        system_audio::capabilities()
    }

    pub fn is_active(&self) -> bool {
        !matches!(
            self.state(),
            SubtitleState::Disabled | SubtitleState::Error { .. }
        )
    }

    fn set_state(&self, state: SubtitleState) {
        set_worker_state(&self.app, &self.state, state);
    }

    pub fn start(
        &self,
        backend: Arc<ParakeetBackend>,
        inference: Arc<InferenceArbiter>,
        language: String,
    ) -> AppResult<()> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .expect("subtitle lifecycle lock poisoned");
        if self.is_active() {
            return Ok(());
        }
        self.set_state(SubtitleState::Starting);
        let capability = self.capabilities();
        self.set_state(SubtitleState::RequestingPermission);
        if !capability.available {
            return self.fail(AppError::new(
                "system_audio_unsupported",
                format!("{} {}", capability.implementation, capability.detail),
            ));
        }

        let session = self.session.fetch_add(1, Ordering::AcqRel) + 1;
        self.revision.store(0, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        *self.cancel.lock().expect("subtitle cancel lock poisoned") = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(PCM_QUEUE_FRAMES);
        let capture_sink = system_audio::CaptureSink::new(sender, inference.capture_epoch());
        let stream = match tauri::async_runtime::block_on(backend.stream_create(&language)) {
            Ok(stream) => stream,
            Err(error) => return self.fail(error),
        };
        let capture = match system_audio::start_capture(capture_sink) {
            Ok(capture) => capture,
            Err(error) => {
                tauri::async_runtime::block_on(backend.stream_cancel(&stream));
                return self.fail(error);
            }
        };
        if let Err(error) = self.show_window() {
            let mut capture = capture;
            let _ = capture.stop();
            tauri::async_runtime::block_on(backend.stream_cancel(&stream));
            return self.fail(error);
        }
        *self.capture.lock().expect("subtitle capture lock poisoned") = Some(capture);

        let worker = std::thread::Builder::new()
            .name("zui-live-subtitles".into())
            .spawn({
                let worker = SubtitleWorker {
                    app: self.app.clone(),
                    session,
                    session_generation: self.session.clone(),
                    receiver,
                    stream,
                    backend,
                    inference,
                    cancel,
                    language,
                    state: self.state.clone(),
                    capture: self.capture.clone(),
                    lifecycle: self.lifecycle.clone(),
                    revision: self.revision.clone(),
                };
                move || subtitle_worker(worker)
            });
        match worker {
            Ok(worker) => {
                *self.worker.lock().expect("subtitle worker lock poisoned") = Some(worker);
                self.set_state(SubtitleState::Listening);
                Ok(())
            }
            Err(error) => {
                let _ = stop_capture(&self.capture);
                self.hide_window();
                self.fail(AppError::new("subtitle_worker", error.to_string()))
            }
        }
    }

    pub fn stop(&self) -> AppResult<()> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .expect("subtitle lifecycle lock poisoned");
        self.session.fetch_add(1, Ordering::AcqRel);
        if self.is_active() {
            self.set_state(SubtitleState::Stopping);
        }
        self.cancel
            .lock()
            .expect("subtitle cancel lock poisoned")
            .store(true, Ordering::Release);
        let stop_result = stop_capture(&self.capture);
        if let Some(worker) = self
            .worker
            .lock()
            .expect("subtitle worker lock poisoned")
            .take()
        {
            // The worker owns only bounded transport work. Join outside of the hot stop path so
            // native lifecycle callers are not blocked by its final cancellation request.
            std::thread::spawn(move || {
                let _ = worker.join();
            });
        }
        self.clear();
        self.hide_window();
        self.set_state(SubtitleState::Disabled);
        stop_result
    }

    fn fail<T>(&self, error: AppError) -> AppResult<T> {
        self.set_state(SubtitleState::Error {
            error: error.clone(),
        });
        Err(error)
    }

    pub fn clear(&self) {
        let _ = self
            .app
            .emit("subtitle://clear", self.session.load(Ordering::Acquire));
    }

    #[allow(dead_code)]
    pub fn emit_text(
        &self,
        utterance_id: u64,
        stable_text: String,
        unstable_text: String,
        is_final: bool,
    ) {
        emit_text(
            &self.app,
            &self.revision,
            self.session.load(Ordering::Acquire),
            utterance_id,
            stable_text,
            unstable_text,
            is_final,
        );
    }

    pub fn set_locked(&self, locked: bool) -> AppResult<()> {
        let window = self.app.get_webview_window("subtitle").ok_or_else(|| {
            AppError::new("subtitle_window", "The subtitle window is not available.")
        })?;
        window
            .set_ignore_cursor_events(locked)
            .map_err(|error| AppError::new("subtitle_window", error.to_string()))?;
        let _ = self.app.emit("subtitle://lock", locked);
        Ok(())
    }

    pub fn restore_position(&self, settings: &SubtitleSettings) -> AppResult<()> {
        let window = self.app.get_webview_window("subtitle").ok_or_else(|| {
            AppError::new("subtitle_window", "The subtitle window is not available.")
        })?;
        if let Some(position) = &settings.position {
            window
                .set_position(PhysicalPosition::new(
                    position.x.clamp(-32_000, 32_000),
                    position.y.clamp(-32_000, 32_000),
                ))
                .map_err(|error| AppError::new("subtitle_window", error.to_string()))?;
        } else {
            window
                .center()
                .map_err(|error| AppError::new("subtitle_window", error.to_string()))?;
        }
        Ok(())
    }

    pub fn current_position(&self) -> AppResult<SubtitlePosition> {
        let window = self.app.get_webview_window("subtitle").ok_or_else(|| {
            AppError::new("subtitle_window", "The subtitle window is not available.")
        })?;
        let position = window
            .outer_position()
            .map_err(|error| AppError::new("subtitle_window", error.to_string()))?;
        let monitor_id = window
            .current_monitor()
            .ok()
            .flatten()
            .and_then(|monitor| monitor.name().map(ToOwned::to_owned))
            .unwrap_or_else(|| "primary".into());
        Ok(SubtitlePosition {
            x: position.x,
            y: position.y,
            monitor_id,
        })
    }

    pub fn reset_position(&self) -> AppResult<()> {
        self.restore_position(&SubtitleSettings::default())
    }

    fn show_window(&self) -> AppResult<()> {
        self.app
            .get_webview_window("subtitle")
            .ok_or_else(|| {
                AppError::new("subtitle_window", "The subtitle window is not available.")
            })?
            .show()
            .map_err(|error| AppError::new("subtitle_window", error.to_string()))
    }

    fn hide_window(&self) {
        if let Some(window) = self.app.get_webview_window("subtitle") {
            let _ = window.hide();
        }
    }
}

struct SubtitleWorker {
    app: AppHandle,
    session: u64,
    session_generation: Arc<AtomicU64>,
    receiver: mpsc::Receiver<PcmFrame>,
    stream: StreamingSession,
    backend: Arc<ParakeetBackend>,
    inference: Arc<InferenceArbiter>,
    cancel: Arc<AtomicBool>,
    language: String,
    state: Arc<RwLock<SubtitleState>>,
    capture: Arc<Mutex<Option<Box<dyn system_audio::SystemAudioCapture>>>>,
    lifecycle: Arc<Mutex<()>>,
    revision: Arc<AtomicU64>,
}

fn subtitle_worker(worker: SubtitleWorker) {
    let SubtitleWorker {
        app,
        session,
        session_generation,
        receiver,
        mut stream,
        backend,
        inference,
        cancel,
        language,
        state,
        capture,
        lifecycle,
        revision,
    } = worker;
    let mut stabilizer = SubtitleStabilizer::default();
    let mut utterance = 1;
    let mut transcript = String::new();
    let mut pcm = Vec::with_capacity(FEED_SAMPLES);
    let mut stream_started = Instant::now();
    let mut silence_since: Option<Instant> = None;
    let mut accepted_epoch = inference.current_capture_epoch();
    let mut needs_fresh_stream = false;
    let mut paused = false;

    let mut failure: Option<AppError> = None;
    while !cancel.load(Ordering::Acquire) {
        let Ok(frame) = receiver.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        if frame.samples.is_empty() && frame.discontinuity {
            failure = Some(capture_failure_error());
            break;
        }
        let Some(quantum) = inference.try_begin_subtitle_quantum() else {
            pcm.clear();
            transcript.clear();
            stabilizer.reset();
            drain_pcm(&receiver);
            needs_fresh_stream = true;
            if !paused {
                let _ = app.emit("subtitle://clear", session);
                set_worker_state(&app, &state, SubtitleState::PausedForDictation);
                paused = true;
            }
            continue;
        };
        let current_epoch = inference.current_capture_epoch();
        if frame.epoch != current_epoch {
            pcm.clear();
            needs_fresh_stream = true;
            continue;
        }
        if frame.discontinuity || needs_fresh_stream || frame.epoch != accepted_epoch {
            // Stream rotation can wait on transport startup. It must never hold the inference
            // permit, otherwise dictation can be blocked for the entire startup timeout.
            drop(quantum);
            pcm.clear();
            transcript.clear();
            stabilizer.reset();
            tauri::async_runtime::block_on(backend.stream_cancel(&stream));
            stream = match tauri::async_runtime::block_on(backend.stream_create(&language)) {
                Ok(stream) => stream,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            };
            utterance += 1;
            accepted_epoch = frame.epoch;
            needs_fresh_stream = false;
            stream_started = Instant::now();
            silence_since = None;
            let _ = app.emit("subtitle://clear", session);
            // The frame belongs to the old stream/epoch; capture will supply a fresh frame.
            continue;
        }
        drop(quantum);
        if paused {
            set_worker_state(&app, &state, SubtitleState::Listening);
            paused = false;
        }

        let loud = frame
            .samples
            .iter()
            .any(|sample| (*sample as f32 / 32768.0).abs() >= SILENCE_LEVEL);
        if loud {
            silence_since = None;
        } else {
            silence_since.get_or_insert_with(Instant::now);
        }
        pcm.extend(
            frame
                .samples
                .into_iter()
                .map(|sample| sample as f32 / 32768.0),
        );
        if pcm.len() >= FEED_SAMPLES {
            // Reacquire the gate across the transport call so begin_dictation cannot race this feed.
            let Some(_quantum) = inference.try_begin_subtitle_quantum() else {
                pcm.clear();
                continue;
            };
            let feed = std::mem::take(&mut pcm);
            match tauri::async_runtime::block_on(backend.stream_feed(&stream, feed)) {
                Ok(result) => apply_delta(
                    CaptionEmitter {
                        app: &app,
                        session,
                        revision: &revision,
                        stabilizer: &mut stabilizer,
                        utterance,
                        transcript: &mut transcript,
                    },
                    result,
                    false,
                ),
                Err(error) if !cancel.load(Ordering::Acquire) => {
                    failure = Some(error);
                    break;
                }
                Err(_) => break,
            }
        }
        if stream_started.elapsed() >= ROTATE_AFTER
            || silence_since.is_some_and(|at| at.elapsed() >= SILENCE_AFTER)
        {
            if let Err(error) = rotate_stream(
                CaptionEmitter {
                    app: &app,
                    session,
                    revision: &revision,
                    stabilizer: &mut stabilizer,
                    utterance,
                    transcript: &mut transcript,
                },
                &backend,
                &mut stream,
                &cancel,
                &language,
            ) {
                failure = Some(error);
                break;
            }
            utterance += 1;
            stream_started = Instant::now();
            silence_since = None;
        }
    }
    tauri::async_runtime::block_on(backend.stream_cancel(&stream));
    if let Some(error) = failure {
        // Start may have already replaced this worker after Stop. Serialize cleanup with Start and
        // verify the generation before changing shared capture/window/state.
        let _lifecycle = lifecycle.lock().expect("subtitle lifecycle lock poisoned");
        if session_generation.load(Ordering::Acquire) != session {
            return;
        }
        cancel.store(true, Ordering::Release);
        let _ = stop_capture(&capture);
        set_worker_state(&app, &state, SubtitleState::Error { error });
        let _ = app.emit("subtitle://clear", session);
        if let Some(window) = app.get_webview_window("subtitle") {
            let _ = window.hide();
        }
    }
}

fn rotate_stream(
    emitter: CaptionEmitter<'_>,
    backend: &ParakeetBackend,
    stream: &mut StreamingSession,
    cancel: &AtomicBool,
    language: &str,
) -> AppResult<()> {
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    let result = tauri::async_runtime::block_on(backend.stream_finalize(stream))?;
    apply_delta(emitter, result, true);
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    *stream = tauri::async_runtime::block_on(backend.stream_create(language))?;
    Ok(())
}

fn stop_capture(
    capture: &Mutex<Option<Box<dyn system_audio::SystemAudioCapture>>>,
) -> AppResult<()> {
    capture
        .lock()
        .expect("subtitle capture lock poisoned")
        .take()
        .map(|mut capture| capture.stop())
        .unwrap_or(Ok(()))
}

fn drain_pcm(receiver: &mpsc::Receiver<PcmFrame>) {
    while receiver.try_recv().is_ok() {}
}

fn set_worker_state(app: &AppHandle, state: &RwLock<SubtitleState>, value: SubtitleState) {
    *state.write().expect("subtitle state lock poisoned") = value.clone();
    let _ = app.emit("subtitle://state", value);
}

struct CaptionEmitter<'a> {
    app: &'a AppHandle,
    session: u64,
    revision: &'a AtomicU64,
    stabilizer: &'a mut SubtitleStabilizer,
    utterance: u64,
    transcript: &'a mut String,
}

fn apply_delta(emitter: CaptionEmitter<'_>, result: StreamingTranscription, finalizing: bool) {
    // The server owns token boundaries and whitespace. Chunks can split a contraction, CJK text,
    // or a word; inserting/trimming here corrupts the authoritative transcript.
    append_delta(emitter.transcript, &result.text);
    if emitter.transcript.is_empty() {
        return;
    }
    if finalizing || result.eou != 0 {
        let text = emitter.stabilizer.finalize(emitter.transcript);
        emit_text(
            emitter.app,
            emitter.revision,
            emitter.session,
            emitter.utterance,
            text,
            String::new(),
            true,
        );
        emitter.transcript.clear();
        emitter.stabilizer.reset();
    } else {
        let (stable, unstable) = emitter.stabilizer.revise(emitter.transcript);
        emit_text(
            emitter.app,
            emitter.revision,
            emitter.session,
            emitter.utterance,
            stable,
            unstable,
            false,
        );
    }
}

fn append_delta(transcript: &mut String, delta: &str) {
    transcript.push_str(delta);
}

fn capture_failure_error() -> AppError {
    AppError::new(
        "system_audio_capture",
        "WASAPI loopback stopped unexpectedly. Stop and restart live subtitles.",
    )
}

fn emit_text(
    app: &AppHandle,
    revision: &AtomicU64,
    session: u64,
    utterance_id: u64,
    stable_text: String,
    unstable_text: String,
    is_final: bool,
) {
    let _ = app.emit(
        "subtitle://text",
        SubtitleText {
            session_id: session,
            revision: revision.fetch_add(1, Ordering::AcqRel) + 1,
            utterance_id,
            stable_text,
            unstable_text,
            is_final,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::append_delta;

    #[test]
    fn preserves_authoritative_streaming_delta_boundaries() {
        let mut text = String::from("don");
        append_delta(&mut text, "'t");
        append_delta(&mut text, " trim ");
        append_delta(&mut text, "this");
        assert_eq!(text, "don't trim this");
    }

    #[test]
    fn preserves_cjk_and_punctuation_deltas_without_respacing() {
        let mut text = String::from("你好");
        append_delta(&mut text, "世界");
        append_delta(&mut text, "。");
        assert_eq!(text, "你好世界。");
    }
}
