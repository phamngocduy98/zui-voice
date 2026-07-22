use crate::types::{SystemAudioCapabilities, SystemAudioPermission};

/// ScreenCaptureKit packaging and entitlement behavior has not been proven against this app's
/// minimum deployment target, therefore this target is deliberately unavailable.
pub fn capabilities() -> SystemAudioCapabilities {
    SystemAudioCapabilities {
        available: false,
        permission: SystemAudioPermission::Unavailable,
        implementation: "ScreenCaptureKit".into(),
        detail: "System-output capture is not enabled in this build because ScreenCaptureKit support has not been validated for the bundled runtime. No microphone fallback is used.".into(),
    }
}
