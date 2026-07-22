use crate::types::{SystemAudioCapabilities, SystemAudioPermission};

/// PipeWire/portal and PulseAudio monitor behavior varies by desktop session. Until the native
/// PipeWire integration is packaged and tested, expose an actionable unsupported result only.
pub fn capabilities() -> SystemAudioCapabilities {
    SystemAudioCapabilities {
        available: false,
        permission: SystemAudioPermission::Unavailable,
        implementation: "PipeWire / PulseAudio monitor".into(),
        detail: "System-output capture is not enabled in this build because PipeWire portal and monitor capture have not been validated. Zui. Voice will never use a microphone as a fallback.".into(),
    }
}
