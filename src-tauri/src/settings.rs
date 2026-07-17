use crate::types::{AppError, AppResult, AppSettings};
use std::{fs, path::PathBuf, sync::RwLock};
use tauri::{AppHandle, Manager};

pub struct SettingsStore {
    path: PathBuf,
    value: RwLock<AppSettings>,
}

impl SettingsStore {
    pub fn load(app: &AppHandle) -> AppResult<Self> {
        let dir = app
            .path()
            .app_config_dir()
            .map_err(|e| AppError::fatal("config_dir", e.to_string()))?;
        fs::create_dir_all(&dir).map_err(|e| AppError::fatal("config_dir", e.to_string()))?;
        let path = dir.join("settings.json");
        let value = if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default()
        } else {
            AppSettings::default()
        };
        Ok(Self {
            path,
            value: RwLock::new(value),
        })
    }

    pub fn get(&self) -> AppSettings {
        self.value.read().expect("settings lock poisoned").clone()
    }

    pub fn save(&self, settings: AppSettings) -> AppResult<()> {
        validate(&settings)?;
        let json = serde_json::to_vec_pretty(&settings)
            .map_err(|e| AppError::new("settings_serialize", e.to_string()))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json).map_err(|e| AppError::new("settings_write", e.to_string()))?;
        fs::rename(&temporary, &self.path)
            .map_err(|e| AppError::new("settings_write", e.to_string()))?;
        *self.value.write().expect("settings lock poisoned") = settings;
        Ok(())
    }
}

fn validate(settings: &AppSettings) -> AppResult<()> {
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
        assert!(validate(&AppSettings::default()).is_ok());
    }

    #[test]
    fn rejects_recordings_longer_than_five_minutes() {
        let settings = AppSettings {
            max_recording_seconds: 301,
            ..AppSettings::default()
        };
        assert!(validate(&settings).is_err());
    }

    #[test]
    fn migrates_settings_with_new_default_fields() {
        let settings: AppSettings = serde_json::from_str(r#"{"enabled":false}"#).expect("migrate");
        assert!(!settings.enabled);
        assert_eq!(settings.max_recording_seconds, 300);
        assert_eq!(settings.model_idle_timeout_seconds, 600);
    }
}
