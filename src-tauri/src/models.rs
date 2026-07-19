use crate::types::{BackendDescriptor, LanguageDescriptor, LanguageTier};

pub const NEMOTRON_ID: &str = "nemotron-3.5-asr-streaming-0.6b";
pub const NEMOTRON_MODEL_FILENAME: &str = "nemotron-3.5-asr-streaming-0.6b-q8_0.gguf";

#[derive(Debug, Clone, Copy)]
pub struct BackendDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    pub description: &'static str,
    pub model_filename: &'static str,
    pub locales: &'static [LocaleDefinition],
}

#[derive(Debug, Clone, Copy)]
pub struct LocaleDefinition {
    pub locale: &'static str,
    pub name: &'static str,
    pub tier: LanguageTier,
}

const NEMOTRON_LOCALES: &[LocaleDefinition] = &[
    ready("en-US", "English (United States)"),
    ready("en-GB", "English (United Kingdom)"),
    ready("es-US", "Spanish (United States)"),
    ready("es-ES", "Spanish (Spain)"),
    ready("fr-FR", "French (France)"),
    ready("fr-CA", "French (Canada)"),
    ready("de-DE", "German (Germany)"),
    ready("it-IT", "Italian (Italy)"),
    ready("pt-BR", "Portuguese (Brazil)"),
    ready("pt-PT", "Portuguese (Portugal)"),
    ready("nl-NL", "Dutch (Netherlands)"),
    ready("ru-RU", "Russian (Russia)"),
    ready("ja-JP", "Japanese (Japan)"),
    ready("ko-KR", "Korean (South Korea)"),
    ready("hi-IN", "Hindi (India)"),
    ready("ar-AR", "Arabic"),
    ready("tr-TR", "Turkish (Turkey)"),
    ready("vi-VN", "Vietnamese (Vietnam)"),
    ready("uk-UA", "Ukrainian (Ukraine)"),
    broad("pl-PL", "Polish (Poland)"),
    broad("sv-SE", "Swedish (Sweden)"),
    broad("da-DK", "Danish (Denmark)"),
    broad("nb-NO", "Norwegian Bokmal (Norway)"),
    broad("fi-FI", "Finnish (Finland)"),
    broad("cs-CZ", "Czech (Czechia)"),
    broad("bg-BG", "Bulgarian (Bulgaria)"),
    broad("hr-HR", "Croatian (Croatia)"),
    broad("sk-SK", "Slovak (Slovakia)"),
    broad("zh-CN", "Mandarin (China)"),
    broad("ro-RO", "Romanian (Romania)"),
    broad("hu-HU", "Hungarian (Hungary)"),
    broad("et-EE", "Estonian (Estonia)"),
];

const fn ready(locale: &'static str, name: &'static str) -> LocaleDefinition {
    LocaleDefinition {
        locale,
        name,
        tier: LanguageTier::TranscriptionReady,
    }
}

const fn broad(locale: &'static str, name: &'static str) -> LocaleDefinition {
    LocaleDefinition {
        locale,
        name,
        tier: LanguageTier::BroadCoverage,
    }
}

pub const BACKENDS: &[BackendDefinition] = &[BackendDefinition {
    id: NEMOTRON_ID,
    name: "Nemotron 3.5 ASR",
    language: "Multilingual",
    description: "32 production-ready locales, Q8, local only",
    model_filename: NEMOTRON_MODEL_FILENAME,
    locales: NEMOTRON_LOCALES,
}];

pub fn backend(id: &str) -> Option<&'static BackendDefinition> {
    BACKENDS.iter().find(|backend| backend.id == id)
}

pub fn descriptors() -> Vec<BackendDescriptor> {
    BACKENDS
        .iter()
        .map(|backend| backend.descriptor(false))
        .collect()
}

impl BackendDefinition {
    pub fn supports_locale(&self, locale: &str) -> bool {
        self.locales.iter().any(|item| item.locale == locale)
    }

    pub fn descriptor(&self, installed: bool) -> BackendDescriptor {
        BackendDescriptor {
            id: self.id.into(),
            name: self.name.into(),
            language: self.language.into(),
            description: self.description.into(),
            model: self.model_filename.into(),
            installed,
            locales: self
                .locales
                .iter()
                .map(|locale| LanguageDescriptor {
                    locale: locale.locale.into(),
                    name: locale.name.into(),
                    tier: locale.tier,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nemotron_exposes_only_the_32_production_locales() {
        assert_eq!(BACKENDS.len(), 1);
        let nemotron = backend(NEMOTRON_ID).expect("Nemotron backend");
        assert_eq!(nemotron.locales.len(), 32);
        assert_eq!(
            nemotron
                .locales
                .iter()
                .filter(|locale| locale.tier == LanguageTier::TranscriptionReady)
                .count(),
            19
        );
        assert_eq!(
            nemotron
                .locales
                .iter()
                .filter(|locale| locale.tier == LanguageTier::BroadCoverage)
                .count(),
            13
        );
    }
}
