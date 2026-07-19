use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CURRENT_ONBOARDING_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BackendStatus {
    Missing,
    Stopped,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyBinding {
    pub key: String,
    pub consume: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub hotkey: HotkeyBinding,
    pub input_device_name: Option<String>,
    pub backend_id: String,
    pub locale: String,
    pub launch_at_login: bool,
    pub clipboard_restore: bool,
    pub max_recording_seconds: u64,
    pub model_idle_timeout_seconds: u64,
    pub enabled: bool,
    pub theme: ThemePreference,
    pub onboarding_version: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: HotkeyBinding {
                key: "RightAlt".into(),
                consume: true,
            },
            input_device_name: None,
            backend_id: crate::models::NEMOTRON_ID.into(),
            locale: "vi-VN".into(),
            launch_at_login: false,
            clipboard_restore: true,
            max_recording_seconds: 300,
            model_idle_timeout_seconds: 600,
            enabled: true,
            theme: ThemePreference::System,
            onboarding_version: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendDescriptor {
    pub id: String,
    pub name: String,
    pub language: String,
    pub description: String,
    pub model: String,
    pub installed: bool,
    pub locales: Vec<LanguageDescriptor>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LanguageTier {
    TranscriptionReady,
    BroadCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageDescriptor {
    pub locale: String,
    pub name: String,
    pub tier: LanguageTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable: true,
        }
    }

    pub fn fatal(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable: false,
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum DictationState {
    SetupRequired {
        detail: String,
    },
    Idle {
        #[serde(rename = "backendStatus")]
        backend_status: BackendStatus,
    },
    Recording {
        #[serde(rename = "elapsedMs")]
        elapsed_ms: u64,
    },
    Loading {
        detail: String,
    },
    Transcribing,
    Pasting,
    Success,
    Copied {
        reason: String,
    },
    Error {
        error: AppError,
    },
}

impl Default for DictationState {
    fn default() -> Self {
        Self::Idle {
            backend_status: BackendStatus::Stopped,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub settings: AppSettings,
    pub state: DictationState,
    pub backend: BackendDescriptor,
    pub backends: Vec<BackendDescriptor>,
    pub setup_complete: bool,
    pub onboarding_complete: bool,
    pub platform: String,
    pub wayland: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub backend_id: String,
    pub complete: bool,
    pub server_found: bool,
    pub model_found: bool,
    pub server_path: Option<PathBuf>,
    pub model_path: Option<PathBuf>,
    pub manifest_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadPhase {
    FetchingManifest,
    Downloading,
    Verifying,
    Installing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub phase: DownloadPhase,
    pub asset: String,
    pub received: u64,
    pub total: Option<u64>,
    pub percent: Option<f64>,
}
