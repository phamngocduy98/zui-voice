use crate::types::{AppError, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const MIN_DURATION: Duration = Duration::from_millis(250);
const SILENCE_RMS: f32 = 0.006;

pub struct RecordingArtifact {
    pub path: PathBuf,
}

struct ActiveRecording {
    session: u64,
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    stream_error: Arc<Mutex<Option<AppError>>>,
    sample_rate: u32,
    started: Instant,
}

pub struct AudioRecorder {
    app: AppHandle,
    active: Mutex<Option<ActiveRecording>>,
}

impl AudioRecorder {
    pub fn new(app: &AppHandle) -> Self {
        if let Ok(cache) = app.path().app_cache_dir() {
            if let Err(error) = cleanup_stale_recordings(&cache) {
                eprintln!("Zui. Voice could not clean stale private audio: {error}");
            }
        }
        Self {
            app: app.clone(),
            active: Mutex::new(None),
        }
    }

    pub fn list_devices() -> AppResult<Vec<String>> {
        let host = cpal::default_host();
        let mut names = host
            .input_devices()
            .map_err(|e| AppError::new("microphone_list", e.to_string()))?
            .filter_map(|device| device.description().ok().map(|d| d.name().to_string()))
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        Ok(names)
    }

    pub fn start(
        &self,
        session: u64,
        preferred_name: Option<&str>,
        max_recording_seconds: u64,
    ) -> AppResult<()> {
        let mut active = self.active.lock().expect("audio lock poisoned");
        if active.is_some() {
            return Err(AppError::new(
                "already_recording",
                "A recording is already active.",
            ));
        }
        let host = cpal::default_host();
        let device = if let Some(name) = preferred_name {
            host.input_devices()
                .map_err(|e| AppError::new("microphone_list", e.to_string()))?
                .find(|device| device.description().ok().is_some_and(|d| d.name() == name))
                .or_else(|| host.default_input_device())
        } else {
            host.default_input_device()
        }
        .ok_or_else(|| AppError::new("microphone_missing", "No input device is available."))?;
        let supported = device
            .default_input_config()
            .map_err(|e| AppError::new("microphone_config", e.to_string()))?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();
        let initial_capacity =
            sample_rate as usize * usize::try_from(max_recording_seconds.min(15)).unwrap_or(15);
        let max_samples =
            usize::try_from(sample_rate as u64 * max_recording_seconds).unwrap_or(usize::MAX);
        let samples = Arc::new(Mutex::new(Vec::with_capacity(initial_capacity)));
        let stream_error = Arc::new(Mutex::new(None));
        let error_app = self.app.clone();
        let callback_error = stream_error.clone();
        let err_fn = move |error: cpal::Error| {
            let error = AppError::new("microphone_stream", error.to_string());
            if let Ok(mut current) = callback_error.lock() {
                *current = Some(error.clone());
            }
            let _ = error_app.emit("voice://error", error);
        };
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(
                &device,
                &config,
                samples.clone(),
                self.app.clone(),
                channels,
                max_samples,
                err_fn,
            ),
            cpal::SampleFormat::I16 => build_stream::<i16>(
                &device,
                &config,
                samples.clone(),
                self.app.clone(),
                channels,
                max_samples,
                err_fn,
            ),
            cpal::SampleFormat::U16 => build_stream::<u16>(
                &device,
                &config,
                samples.clone(),
                self.app.clone(),
                channels,
                max_samples,
                err_fn,
            ),
            other => {
                return Err(AppError::new(
                    "microphone_format",
                    format!("Unsupported microphone format: {other:?}"),
                ))
            }
        }?;
        stream
            .play()
            .map_err(|e| AppError::new("microphone_start", e.to_string()))?;
        *active = Some(ActiveRecording {
            session,
            stream,
            samples,
            stream_error,
            sample_rate,
            started: Instant::now(),
        });
        Ok(())
    }

    pub fn cancel(&self) {
        self.active.lock().expect("audio lock poisoned").take();
    }

    pub fn cancel_session(&self, session: u64) {
        let mut active = self.active.lock().expect("audio lock poisoned");
        if active
            .as_ref()
            .is_some_and(|recording| recording.session == session)
        {
            active.take();
        }
    }

    pub fn finish_test(&self, session: u64) -> AppResult<()> {
        let recording = {
            let mut active = self.active.lock().expect("audio lock poisoned");
            match active.as_ref() {
                Some(recording) if recording.session == session => active.take(),
                Some(_) => {
                    return Err(AppError::new(
                        "recording_replaced",
                        "A newer recording is already active.",
                    ))
                }
                None => None,
            }
        }
        .ok_or_else(|| AppError::new("not_recording", "No microphone test is active."))?;
        drop(recording.stream);
        if let Some(error) = recording
            .stream_error
            .lock()
            .expect("stream error lock poisoned")
            .take()
        {
            return Err(error);
        }
        if recording
            .samples
            .lock()
            .expect("audio samples lock poisoned")
            .is_empty()
        {
            return Err(AppError::new(
                "microphone_no_data",
                "The microphone opened, but it did not provide audio. Check system permissions.",
            ));
        }
        Ok(())
    }

    pub fn stop(&self, session: u64) -> AppResult<RecordingArtifact> {
        let recording = {
            let mut active = self.active.lock().expect("audio lock poisoned");
            match active.as_ref() {
                Some(recording) if recording.session == session => active.take(),
                Some(_) => {
                    return Err(AppError::new(
                        "recording_replaced",
                        "A newer recording is already active.",
                    ))
                }
                None => None,
            }
        }
        .ok_or_else(|| AppError::new("not_recording", "No recording is active."))?;
        let duration = recording.started.elapsed();
        drop(recording.stream);
        if let Some(error) = recording
            .stream_error
            .lock()
            .expect("stream error lock poisoned")
            .take()
        {
            return Err(error);
        }
        if duration < MIN_DURATION {
            return Err(AppError::new(
                "too_short",
                "Hold the key a little longer, then speak.",
            ));
        }
        let mono = recording
            .samples
            .lock()
            .expect("sample lock poisoned")
            .clone();
        let samples = resample_linear(&mono, recording.sample_rate, TARGET_SAMPLE_RATE);
        let rms = signal_rms(&samples);
        if rms < SILENCE_RMS {
            return Err(AppError::new("no_speech", "No speech was detected."));
        }
        let cache = self
            .app
            .path()
            .app_cache_dir()
            .map_err(|e| AppError::new("cache_dir", e.to_string()))?;
        std::fs::create_dir_all(&cache).map_err(|e| AppError::new("cache_dir", e.to_string()))?;
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = cache.join(format!("dictation-{id}.wav"));
        write_wav(&path, &samples)?;
        Ok(RecordingArtifact { path })
    }
}

fn cleanup_stale_recordings(cache: &std::path::Path) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(cache) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if entry.file_type()?.is_file()
            && file_name.starts_with("dictation-")
            && (file_name.ends_with(".wav") || file_name.ends_with(".wav.tmp"))
        {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn signal_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }
}

trait ToF32 {
    fn to_f32(self) -> f32;
}
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }
}
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    app: AppHandle,
    channels: usize,
    max_samples: usize,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> AppResult<cpal::Stream>
where
    T: cpal::SizedSample + ToF32 + Copy,
{
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                let converted: Vec<f32> = data.iter().copied().map(ToF32::to_f32).collect();
                let mono = downmix(&converted, channels);
                if let Ok(mut target) = samples.lock() {
                    let remaining = max_samples.saturating_sub(target.len());
                    target.extend(mono.iter().take(remaining));
                }
                if last_emit.elapsed() >= Duration::from_millis(30) {
                    last_emit = Instant::now();
                    let _ = app.emit("voice://spectrum", spectrum_bins(&mono, 24));
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| AppError::new("microphone_stream", e.to_string()))
}

fn spectrum_bins(samples: &[f32], count: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; count];
    }
    let width = (samples.len() / count).max(1);
    (0..count)
        .map(|index| {
            let start = index * width;
            let end = ((index + 1) * width).min(samples.len());
            if start >= end {
                return 0.05;
            }
            let rms = (samples[start..end]
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                / (end - start) as f32)
                .sqrt();
            (rms * 5.5).clamp(0.05, 1.0)
        })
        .collect()
}

fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn resample_linear(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if input.is_empty() || source_rate == target_rate {
        return input.to_vec();
    }
    let output_len = (input.len() as u64 * target_rate as u64 / source_rate as u64) as usize;
    let ratio = source_rate as f64 / target_rate as f64;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let right = (left + 1).min(input.len() - 1);
            let fraction = (position - left as f64) as f32;
            input[left] * (1.0 - fraction) + input[right] * fraction
        })
        .collect()
}

fn write_wav(path: &std::path::Path, samples: &[f32]) -> AppResult<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("audio_write", "Audio path has no parent directory."))?;
    let mut temporary = tempfile::Builder::new()
        .prefix("dictation-")
        .suffix(".wav.tmp")
        .tempfile_in(parent)
        .map_err(|e| AppError::new("audio_write", e.to_string()))?;
    {
        let mut writer = hound::WavWriter::new(temporary.as_file_mut(), spec)
            .map_err(|e| AppError::new("audio_write", e.to_string()))?;
        for sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(value)
                .map_err(|e| AppError::new("audio_write", e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| AppError::new("audio_write", e.to_string()))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| AppError::new("audio_write", e.to_string()))?;
    temporary
        .persist(path)
        .map_err(|e| AppError::new("audio_write", e.error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_to_expected_length() {
        let input = vec![0.0; 48_000];
        assert_eq!(resample_linear(&input, 48_000, 16_000).len(), 16_000);
    }

    #[test]
    fn downmixes_stereo() {
        assert_eq!(downmix(&[1.0, -1.0, 0.5, 0.5], 2), vec![0.0, 0.5]);
    }

    #[test]
    fn identifies_silent_audio() {
        assert!(signal_rms(&vec![0.001; 16_000]) < SILENCE_RMS);
        assert!(signal_rms(&vec![0.1; 16_000]) > SILENCE_RMS);
    }

    #[test]
    fn writes_pcm16_mono_wav() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("recording.wav");
        write_wav(&path, &[0.0, 0.5, -0.5]).expect("write wav");
        let mut reader = hound::WavReader::open(path).expect("read wav");
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, TARGET_SAMPLE_RATE);
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.samples::<i16>().count(), 3);
    }

    #[test]
    fn startup_cleanup_only_removes_private_recording_artifacts() {
        let directory = tempfile::tempdir().expect("temp directory");
        let stale = directory.path().join("dictation-123.wav");
        let unrelated_wav = directory.path().join("keep.wav");
        let similarly_named = directory.path().join("dictation-not-a-wave.txt");
        std::fs::write(&stale, b"private audio").expect("seed stale recording");
        std::fs::write(&unrelated_wav, b"unrelated").expect("seed unrelated wav");
        std::fs::write(&similarly_named, b"unrelated").expect("seed unrelated text");

        cleanup_stale_recordings(directory.path()).expect("clean stale recordings");

        assert!(!stale.exists());
        assert!(unrelated_wav.exists());
        assert!(similarly_named.exists());
    }
}
