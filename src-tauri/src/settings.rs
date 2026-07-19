use crate::types::{AppError, AppResult, AppSettings, CURRENT_ONBOARDING_VERSION};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

pub struct SettingsStore {
    path: PathBuf,
    value: RwLock<AppSettings>,
    save_lock: Mutex<()>,
}

impl SettingsStore {
    pub fn load(app: &AppHandle) -> AppResult<Self> {
        let dir = app
            .path()
            .app_config_dir()
            .map_err(|e| AppError::fatal("config_dir", e.to_string()))?;
        fs::create_dir_all(&dir).map_err(|e| AppError::fatal("config_dir", e.to_string()))?;
        let path = dir.join("settings.json");
        let value = load_settings(&path)?;
        Ok(Self {
            path,
            value: RwLock::new(value),
            save_lock: Mutex::new(()),
        })
    }

    pub fn get(&self) -> AppSettings {
        self.value.read().expect("settings lock poisoned").clone()
    }

    pub fn save(&self, settings: AppSettings) -> AppResult<()> {
        let _save = self.save_lock.lock().expect("settings save lock poisoned");
        validate_settings(&settings)?;
        let json = serde_json::to_vec_pretty(&settings)
            .map_err(|e| AppError::new("settings_serialize", e.to_string()))?;
        write_atomically(&self.path, &json)?;
        *self.value.write().expect("settings lock poisoned") = settings;
        Ok(())
    }
}

fn load_settings(path: &Path) -> AppResult<AppSettings> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppSettings::default());
        }
        Err(error) => return Err(AppError::fatal("settings_read", error.to_string())),
    };
    let parsed = serde_json::from_str::<AppSettings>(&text)
        .map_err(|error| error.to_string())
        .and_then(|settings| {
            validate_settings(&settings)
                .map(|()| settings)
                .map_err(|error| error.message)
        });
    match parsed {
        Ok(settings) => Ok(settings),
        Err(error) => {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let backup = path.with_file_name(format!("settings.invalid-{timestamp}.json"));
            fs::rename(path, &backup).map_err(|backup_error| {
                AppError::fatal("settings_backup", backup_error.to_string())
            })?;
            eprintln!(
                "Zui. Voice ignored invalid settings ({error}); preserved them at {}",
                backup.display()
            );
            Ok(AppSettings::default())
        }
    }
}

fn write_atomically(path: &Path, contents: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("settings_write", "Settings path has no parent directory."))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| AppError::new("settings_write", e.to_string()))?;
    temporary
        .write_all(contents)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|e| AppError::new("settings_write", e.to_string()))?;
    temporary
        .persist(path)
        .map_err(|e| AppError::new("settings_write", e.error.to_string()))?;
    Ok(())
}

pub fn validate_settings(settings: &AppSettings) -> AppResult<()> {
    if settings.onboarding_version > CURRENT_ONBOARDING_VERSION {
        return Err(AppError::new(
            "invalid_onboarding_version",
            "The onboarding version is newer than this application supports.",
        ));
    }
    if settings.hotkey.key != "RightAlt" {
        return Err(AppError::new(
            "unsupported_hotkey",
            "This version supports Right Alt as the hold key.",
        ));
    }
    if settings.backend_id != "parakeet-vietnamese" {
        return Err(AppError::new(
            "unsupported_backend",
            "The selected transcription backend is not supported.",
        ));
    }
    if settings
        .input_device_name
        .as_ref()
        .is_some_and(|name| name.len() > 512 || name.contains(['\r', '\n']))
    {
        return Err(AppError::new(
            "invalid_input_device",
            "The microphone name is invalid.",
        ));
    }
    if !(1..=300).contains(&settings.max_recording_seconds) {
        return Err(AppError::new(
            "invalid_recording_limit",
            "Recording limit must be between 1 and 300 seconds.",
        ));
    }
    if !(60..=3600).contains(&settings.model_idle_timeout_seconds) {
        return Err(AppError::new(
            "invalid_idle_timeout",
            "Idle timeout must be between 1 and 60 minutes.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_within_limits() {
        assert!(validate_settings(&AppSettings::default()).is_ok());
    }

    #[test]
    fn rejects_recordings_longer_than_five_minutes() {
        let settings = AppSettings {
            max_recording_seconds: 301,
            ..AppSettings::default()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn migrates_settings_with_new_default_fields() {
        let settings: AppSettings = serde_json::from_str(r#"{"enabled":false}"#).expect("migrate");
        assert!(!settings.enabled);
        assert_eq!(settings.max_recording_seconds, 300);
        assert_eq!(settings.model_idle_timeout_seconds, 600);
        assert_eq!(settings.theme, crate::types::ThemePreference::System);
        assert_eq!(settings.onboarding_version, 0);
    }

    #[test]
    fn rejects_unknown_future_onboarding_versions() {
        let settings = AppSettings {
            onboarding_version: CURRENT_ONBOARDING_VERSION + 1,
            ..AppSettings::default()
        };
        assert_eq!(
            validate_settings(&settings)
                .expect_err("future onboarding version")
                .code,
            "invalid_onboarding_version"
        );
    }

    #[test]
    fn atomically_replaces_an_existing_settings_file() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("settings.json");
        fs::write(&path, b"old settings").expect("seed settings");

        write_atomically(&path, b"new settings").expect("replace settings");

        assert_eq!(fs::read(&path).expect("read settings"), b"new settings");
    }

    #[test]
    fn preserves_invalid_settings_before_recovering_defaults() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("settings.json");
        fs::write(&path, b"not valid json").expect("seed invalid settings");

        let settings = load_settings(&path).expect("recover settings");

        assert!(validate_settings(&settings).is_ok());
        assert!(!path.exists());
        assert!(fs::read_dir(directory.path())
            .expect("list settings backups")
            .any(|entry| entry
                .expect("settings backup")
                .file_name()
                .to_string_lossy()
                .starts_with("settings.invalid-")));
    }
}
