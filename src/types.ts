export type BackendStatus = "missing" | "stopped" | "loading" | "ready" | "error";

export interface HotkeyBinding {
  key: HoldKey;
  consume: boolean;
}
export type HoldKey = "RightAlt" | "RightControl" | "F8" | "F9";
export type ThemePreference = "system" | "light" | "dark";

export interface AppSettings {
  hotkey: HotkeyBinding;
  inputDeviceName: string | null;
  backendId: string;
  locale: string;
  launchAtLogin: boolean;
  clipboardRestore: boolean;
  maxRecordingSeconds: number;
  modelIdleTimeoutSeconds: number;
  enabled: boolean;
  theme: ThemePreference;
  onboardingVersion: number;
}

export interface BackendDescriptor {
  id: string;
  name: string;
  language: string;
  description: string;
  model: string;
  installed: boolean;
  locales: LanguageDescriptor[];
}

export interface LanguageDescriptor {
  locale: string;
  name: string;
  tier: "transcriptionReady" | "broadCoverage";
}

export interface AppError {
  code: string;
  message: string;
  recoverable: boolean;
}

export type DictationState =
  | { phase: "setupRequired"; detail: string }
  | { phase: "idle"; backendStatus: BackendStatus }
  | { phase: "recording"; elapsedMs: number }
  | { phase: "loading"; detail: string }
  | { phase: "transcribing" }
  | { phase: "pasting" }
  | { phase: "success" }
  | { phase: "copied"; reason: string }
  | { phase: "error"; error: AppError };

export interface AppSnapshot {
  settings: AppSettings;
  state: DictationState;
  backend: BackendDescriptor;
  backends: BackendDescriptor[];
  setupComplete: boolean;
  onboardingComplete: boolean;
  platform: string;
  wayland: boolean;
}

export interface SetupStatus {
  backendId: string;
  complete: boolean;
  serverFound: boolean;
  modelFound: boolean;
  serverPath: string | null;
  modelPath: string | null;
  manifestConfigured: boolean;
}

export interface DownloadProgress {
  phase: "fetchingManifest" | "downloading" | "verifying" | "installing";
  asset: string;
  received: number;
  total: number | null;
  percent: number | null;
}
