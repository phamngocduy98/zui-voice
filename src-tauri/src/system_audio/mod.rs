//! System-output audio contracts. Platform implementations intentionally fail closed until
//! their native capture and permissions have been validated for the shipped runtime.
#![allow(dead_code)]

#[cfg(not(target_os = "windows"))]
use crate::types::AppError;
use crate::types::{AppResult, SystemAudioCapabilities, SystemAudioPermission};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::SyncSender,
    Arc,
};

pub const CANONICAL_SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = 320; // 20 ms at 16 kHz

#[derive(Debug, Clone)]
pub struct PcmFrame {
    pub samples: Vec<i16>,
    pub discontinuity: bool,
    pub epoch: u64,
}

#[derive(Clone)]
pub struct CaptureSink {
    sender: SyncSender<PcmFrame>,
    epoch: Arc<AtomicU64>,
    discontinuity: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

impl CaptureSink {
    pub fn new(sender: SyncSender<PcmFrame>, epoch: Arc<AtomicU64>) -> Self {
        Self {
            sender,
            epoch,
            discontinuity: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn sender(&self) -> &SyncSender<PcmFrame> {
        &self.sender
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn mark_discontinuity(&self) {
        self.discontinuity.store(true, Ordering::Release);
    }

    pub fn take_discontinuity(&self) -> bool {
        self.discontinuity.swap(false, Ordering::AcqRel)
    }

    pub fn mark_failed(&self) {
        self.failed.store(true, Ordering::Release);
        self.mark_discontinuity();
    }

    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

/// An idempotent native-capture session. Implementations must never write audio to disk.
pub trait SystemAudioCapture: Send {
    fn stop(&mut self) -> AppResult<()>;
}

pub fn capabilities() -> SystemAudioCapabilities {
    #[cfg(target_os = "windows")]
    return windows::capabilities();
    #[cfg(target_os = "macos")]
    return macos::capabilities();
    #[cfg(target_os = "linux")]
    return linux::capabilities();
    #[allow(unreachable_code)]
    SystemAudioCapabilities {
        available: false,
        permission: SystemAudioPermission::Unavailable,
        implementation: "unsupported".into(),
        detail: "System-audio subtitles are not available on this operating system.".into(),
    }
}

pub fn start_capture(sink: CaptureSink) -> AppResult<Box<dyn SystemAudioCapture>> {
    #[cfg(target_os = "windows")]
    return windows::start_capture(sink);
    #[cfg(not(target_os = "windows"))]
    {
        let capability = capabilities();
        let _ = sink;
        Err(AppError::new(
            "system_audio_unsupported",
            format!("{} {}", capability.implementation, capability.detail),
        ))
    }
}

/// Stateful windowed-sinc resampler. The 32-tap low-pass filter attenuates frequencies above
/// the 16 kHz Nyquist limit before downsampling; retained history and phase preserve chunk
/// continuity without the aliasing introduced by linear interpolation.
#[derive(Debug, Clone)]
pub struct StreamingResampler {
    source_rate: u32,
    phase: f64,
    history: Vec<f32>,
}

impl StreamingResampler {
    const HALF_TAPS: isize = 16;

    pub fn new(source_rate: u32) -> Self {
        Self {
            source_rate,
            phase: 0.0,
            history: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.history.clear();
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        if self.source_rate == CANONICAL_SAMPLE_RATE {
            return input.to_vec();
        }
        let mut source = std::mem::take(&mut self.history);
        source.extend_from_slice(input);
        let ratio = CANONICAL_SAMPLE_RATE as f64 / self.source_rate as f64;
        let step = 1.0 / ratio;
        let cutoff = ratio.min(1.0) as f32;
        let mut output = Vec::new();
        while self.phase + (Self::HALF_TAPS as f64) < source.len() as f64 {
            output.push(windowed_sinc(&source, self.phase, cutoff));
            self.phase += step;
        }
        let retain = (Self::HALF_TAPS * 2) as usize;
        let consumed = self.phase.floor().max(0.0) as usize;
        let start = consumed.saturating_sub(retain).min(source.len());
        self.history = source[start..].to_vec();
        self.phase -= start as f64;
        output
    }
}

fn windowed_sinc(source: &[f32], position: f64, cutoff: f32) -> f32 {
    let center = position.floor() as isize;
    let mut sum = 0.0;
    let mut weight_sum = 0.0;
    for offset in -StreamingResampler::HALF_TAPS..=StreamingResampler::HALF_TAPS {
        let index = center + offset;
        if index < 0 || index >= source.len() as isize {
            continue;
        }
        let x = (position - index as f64) as f32;
        let normalized = x * cutoff;
        let sinc = if normalized.abs() < f32::EPSILON {
            1.0
        } else {
            (std::f32::consts::PI * normalized).sin() / (std::f32::consts::PI * normalized)
        };
        let window_position = x / (StreamingResampler::HALF_TAPS as f32 + 1.0);
        let window = 0.5 + 0.5 * (std::f32::consts::PI * window_position).cos();
        let weight = cutoff * sinc * window;
        sum += source[index as usize] * weight;
        weight_sum += weight;
    }
    if weight_sum.abs() > f32::EPSILON {
        sum / weight_sum
    } else {
        0.0
    }
}

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_resampler_preserves_chunk_continuity() {
        let input: Vec<f32> = (0..960).map(|value| value as f32).collect();
        let mut one_chunk = StreamingResampler::new(48_000);
        let expected = one_chunk.process(&input);
        let mut chunked = StreamingResampler::new(48_000);
        let mut actual = chunked.process(&input[..480]);
        actual.extend(chunked.process(&input[480..]));
        assert_eq!(actual.len(), expected.len());
        for (left, right) in actual.iter().zip(expected.iter()) {
            assert!((left - right).abs() < 0.001);
        }
    }
}
