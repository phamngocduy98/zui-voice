//! Scheduling boundary for model consumers. Dictation owns the permit; live subtitles may only
//! begin a bounded quantum when no dictation is active. This boundary intentionally has no audio
//! storage and cannot replay paused live input.
#![allow(dead_code)]
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

pub struct InferenceArbiter {
    dictation_active: AtomicBool,
    capture_epoch: Arc<AtomicU64>,
    /// Serializes the check-and-begin edge so dictation cannot start between a subtitle check and
    /// a transport feed. The guard is intentionally held only while the feed is launched.
    gate: Mutex<()>,
}

impl Default for InferenceArbiter {
    fn default() -> Self {
        Self {
            dictation_active: AtomicBool::new(false),
            capture_epoch: Arc::new(AtomicU64::new(0)),
            gate: Mutex::new(()),
        }
    }
}

impl InferenceArbiter {
    pub fn begin_dictation(&self) {
        let _gate = self.gate.lock().expect("inference gate poisoned");
        self.dictation_active.store(true, Ordering::Release);
        self.capture_epoch.fetch_add(1, Ordering::AcqRel);
    }
    pub fn end_dictation(&self) {
        let _gate = self.gate.lock().expect("inference gate poisoned");
        self.capture_epoch.fetch_add(1, Ordering::AcqRel);
        self.dictation_active.store(false, Ordering::Release);
    }
    pub fn capture_epoch(&self) -> Arc<AtomicU64> {
        self.capture_epoch.clone()
    }

    pub fn current_capture_epoch(&self) -> u64 {
        self.capture_epoch.load(Ordering::Acquire)
    }
    pub fn may_run_subtitle_quantum(&self) -> bool {
        !self.dictation_active.load(Ordering::Acquire)
    }
    pub fn try_begin_subtitle_quantum(&self) -> Option<SubtitleQuantum<'_>> {
        let gate = self.gate.lock().expect("inference gate poisoned");
        (!self.dictation_active.load(Ordering::Acquire)).then_some(SubtitleQuantum { _gate: gate })
    }
}

pub struct SubtitleQuantum<'a> {
    _gate: std::sync::MutexGuard<'a, ()>,
}

#[cfg(test)]
mod tests {
    use super::InferenceArbiter;
    #[test]
    fn dictation_has_strict_priority() {
        let arbiter = InferenceArbiter::default();
        assert!(arbiter.may_run_subtitle_quantum());
        arbiter.begin_dictation();
        assert!(!arbiter.may_run_subtitle_quantum());
        arbiter.end_dictation();
        assert!(arbiter.may_run_subtitle_quantum());
    }

    #[test]
    fn capture_epoch_invalidates_audio_at_both_dictation_edges() {
        let arbiter = InferenceArbiter::default();
        assert_eq!(arbiter.current_capture_epoch(), 0);
        arbiter.begin_dictation();
        assert_eq!(arbiter.current_capture_epoch(), 1);
        arbiter.end_dictation();
        assert_eq!(arbiter.current_capture_epoch(), 2);
    }
}
