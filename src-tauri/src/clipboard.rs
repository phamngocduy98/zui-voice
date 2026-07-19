use crate::{
    platform::{is_wayland, target_is_current, ForegroundTarget},
    types::{AppError, AppResult},
};
use clipboard_rs::{common::ContentFormat, Clipboard, ClipboardContext};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Pasted,
    Copied(String),
}

pub struct ClipboardService;

impl ClipboardService {
    pub fn new() -> Self {
        Self
    }

    pub async fn deliver(
        &self,
        text: String,
        target: ForegroundTarget,
        restore: bool,
    ) -> AppResult<Delivery> {
        tokio::task::spawn_blocking(move || {
            if !target_is_current(target) {
                set_text(&text)?;
                return Ok(Delivery::Copied("Target changed · paste manually".into()));
            }
            if is_wayland() {
                set_text(&text)?;
                return Ok(Delivery::Copied("Wayland · paste manually".into()));
            }

            let context = ClipboardContext::new()
                .map_err(|e| AppError::new("clipboard_open", e.to_string()))?;
            let snapshot = if restore {
                Some(
                    context
                        .get(&[
                            ContentFormat::Text,
                            ContentFormat::Html,
                            ContentFormat::Rtf,
                            ContentFormat::Image,
                            ContentFormat::Files,
                        ])
                        .map_err(|e| AppError::new("clipboard_snapshot", e.to_string()))?,
                )
            } else {
                None
            };
            context
                .set_text(text.clone())
                .map_err(|e| AppError::new("clipboard_write", e.to_string()))?;
            if simulate_paste().is_err() {
                return Ok(Delivery::Copied(
                    "Paste unavailable · paste manually".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(220));
            if let Some(snapshot) = snapshot {
                let current = context.get_text().ok();
                if should_restore(current.as_deref(), &text) && snapshot.is_empty() {
                    let _ = context.clear();
                } else if should_restore(current.as_deref(), &text) {
                    context
                        .set(snapshot)
                        .map_err(|e| AppError::new("clipboard_restore", e.to_string()))?;
                }
            }
            Ok(Delivery::Pasted)
        })
        .await
        .map_err(|e| AppError::new("clipboard_task", e.to_string()))?
    }
}

fn should_restore(current_text: Option<&str>, temporary_text: &str) -> bool {
    current_text == Some(temporary_text)
}

fn set_text(text: &str) -> AppResult<()> {
    ClipboardContext::new()
        .map_err(|e| AppError::new("clipboard_open", e.to_string()))?
        .set_text(text.to_string())
        .map_err(|e| AppError::new("clipboard_write", e.to_string()))
}

fn simulate_paste() -> AppResult<()> {
    use rdev::{simulate, EventType, Key, SimulateError};
    #[cfg(target_os = "macos")]
    let modifier = Key::MetaLeft;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::ControlLeft;

    let send = |event| {
        simulate(&event).map_err(|error: SimulateError| {
            AppError::new(
                "paste_injection",
                format!("Could not inject paste shortcut: {error:?}"),
            )
        })
    };
    send(EventType::KeyPress(modifier))?;
    let result = send(EventType::KeyPress(Key::KeyV));
    // Always release both keys after a partial injection failure so a failed
    // paste cannot leave the user's modifier state stuck.
    let key_release = send(EventType::KeyRelease(Key::KeyV));
    let modifier_release = send(EventType::KeyRelease(modifier));
    result.and(key_release).and(modifier_release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_variants_are_distinct() {
        assert_ne!(Delivery::Pasted, Delivery::Copied("copied".into()));
    }

    #[test]
    fn clipboard_restore_never_overwrites_new_user_content() {
        assert!(should_restore(
            Some("temporary transcript"),
            "temporary transcript"
        ));
        assert!(!should_restore(
            Some("user copied this"),
            "temporary transcript"
        ));
        assert!(!should_restore(None, "temporary transcript"));
    }
}
