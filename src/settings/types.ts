export type SettingsSection = "dictation" | "audio" | "subtitles" | "engine" | "appearance" | "legal";
export type SaveState = "saved" | "saving" | "error";
export type EngineStatus = "missing" | "ready" | "loading" | "error" | "stopped";

export const sectionCopy: Record<SettingsSection, { title: string; detail: string }> = {
  dictation: { title: "Dictation", detail: "Choose how push-to-talk records and inserts text." },
  audio: { title: "Audio", detail: "Select and test your microphone." },
  subtitles: { title: "Live Subtitles", detail: "Show private, local captions for audio playing on your computer." },
  engine: { title: "Local Engine", detail: "Manage the private, on-device transcription model." },
  appearance: { title: "Appearance", detail: "Choose how Zui Voice looks on this device." },
  legal: { title: "Legal & Privacy", detail: "Review local-data behavior and third-party notices." }
};
