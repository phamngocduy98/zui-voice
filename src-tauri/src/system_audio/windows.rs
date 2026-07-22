use super::{CaptureSink, PcmFrame, StreamingResampler, SystemAudioCapture, FRAME_SAMPLES};
use crate::types::{AppError, AppResult, SystemAudioCapabilities, SystemAudioPermission};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, Stream, StreamConfig,
};
use std::sync::{mpsc::TrySendError, Arc, Mutex};

/// Windows shared-mode capture of the default render endpoint. CPAL 0.18 marks an input stream
/// created on an eRender device as `AUDCLNT_STREAMFLAGS_LOOPBACK`; it never selects a microphone.
pub fn capabilities() -> SystemAudioCapabilities {
    let host = cpal::default_host();
    let available = host.default_output_device().is_some();
    SystemAudioCapabilities {
        available,
        permission: if available { SystemAudioPermission::NotRequired } else { SystemAudioPermission::Unavailable },
        implementation: "WASAPI loopback".into(),
        detail: if available {
            "Captures the current default Windows output device. Captions stay in bounded memory."
        } else {
            "No default Windows output device is available. Connect or enable an output device, then try again."
        }.into(),
    }
}

pub fn start_capture(sink: CaptureSink) -> AppResult<Box<dyn SystemAudioCapture>> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| {
            AppError::new(
                "system_audio_device",
                "Windows has no default output device for WASAPI loopback capture.",
            )
        })?;
    // The render mix format is the only shared-mode loopback format guaranteed by WASAPI.
    let supported = device.default_output_config().map_err(cpal_error)?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let pipeline = Arc::new(Mutex::new(PcmPipeline::new(
        config.channels,
        config.sample_rate,
        sink.clone(),
    )));

    macro_rules! input_stream {
        ($type:ty, $convert:expr) => {{
            let pipeline = pipeline.clone();
            let errors = sink.clone();
            device.build_input_stream(
                config.clone(),
                move |data: &[$type], _| push(&pipeline, data, $convert),
                move |error| {
                    eprintln!("Zui. Voice WASAPI loopback stream error: {error}");
                    errors.mark_failed();
                },
                None,
            )
        }};
    }
    let stream = match sample_format {
        SampleFormat::F32 => input_stream!(f32, |v| v),
        SampleFormat::F64 => input_stream!(f64, |v| v as f32),
        SampleFormat::I16 => input_stream!(i16, |v| v as f32 / i16::MAX as f32),
        SampleFormat::I32 => input_stream!(i32, |v| v as f32 / i32::MAX as f32),
        SampleFormat::I64 => input_stream!(i64, |v| v as f32 / i64::MAX as f32),
        SampleFormat::I8 => input_stream!(i8, |v| v as f32 / i8::MAX as f32),
        SampleFormat::U8 => input_stream!(u8, |v| (v as f32 - 128.0) / 128.0),
        unsupported => {
            return Err(AppError::new(
                "system_audio_format",
                format!("WASAPI loopback format {unsupported} is not supported by this build."),
            ))
        }
    }
    .map_err(cpal_error)?;
    stream.play().map_err(cpal_error)?;
    Ok(Box::new(WindowsCapture { stream }))
}

fn cpal_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "system_audio_capture",
        format!("WASAPI loopback could not start: {error}"),
    )
}

fn push<T>(pipeline: &Mutex<PcmPipeline>, input: &[T], convert: impl Fn(T) -> f32)
where
    T: Copy,
{
    // Audio callbacks must never block. The bounded queue drops complete 20 ms frames whenever
    // inference cannot keep up; the next delivered frame carries a discontinuity marker.
    if let Ok(mut pipeline) = pipeline.try_lock() {
        pipeline.push(input, convert);
    }
}

struct PcmPipeline {
    channels: usize,
    resampler: StreamingResampler,
    pending: Vec<i16>,
    sink: CaptureSink,
    dropped: bool,
}
impl PcmPipeline {
    fn new(channels: u16, rate: u32, sink: CaptureSink) -> Self {
        Self {
            channels: usize::from(channels.max(1)),
            resampler: StreamingResampler::new(rate),
            pending: Vec::with_capacity(FRAME_SAMPLES * 2),
            sink,
            dropped: false,
        }
    }
    fn push<T: Copy>(&mut self, input: &[T], convert: impl Fn(T) -> f32) {
        if self.sink.take_discontinuity() {
            self.pending.clear();
            self.resampler.reset();
            self.dropped = true;
        }
        if self.sink.is_failed() {
            let _ = self.sink.sender().try_send(PcmFrame {
                samples: Vec::new(),
                discontinuity: true,
                epoch: self.sink.epoch(),
            });
            return;
        }
        let mono: Vec<f32> = input
            .chunks(self.channels)
            .map(|frame| {
                frame.iter().copied().map(&convert).sum::<f32>() / frame.len().max(1) as f32
            })
            .collect();
        self.pending
            .extend(self.resampler.process(&mono).into_iter().map(to_i16));
        while self.pending.len() >= FRAME_SAMPLES {
            let samples: Vec<i16> = self.pending.drain(..FRAME_SAMPLES).collect();
            match self.sink.sender().try_send(PcmFrame {
                samples,
                discontinuity: self.dropped,
                epoch: self.sink.epoch(),
            }) {
                Ok(()) => self.dropped = false,
                Err(TrySendError::Full(_)) => {
                    self.pending.clear();
                    self.resampler.reset();
                    self.dropped = true;
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
    }
}
fn to_i16(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}
struct WindowsCapture {
    stream: Stream,
}
impl SystemAudioCapture for WindowsCapture {
    fn stop(&mut self) -> AppResult<()> {
        self.stream.pause().map_err(cpal_error)
    }
}

#[cfg(test)]
mod tests {
    use super::to_i16;
    #[test]
    fn pcm_conversion_clamps_without_wrapping() {
        assert_eq!(to_i16(-2.0), i16::MIN + 1);
        assert_eq!(to_i16(2.0), i16::MAX);
    }
}
