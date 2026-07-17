export type BackendStatus = "missing" | "stopped" | "loading" | "ready" | "error";

export interface HotkeyBinding {
  key: string;
  consume: boolean;
}
export interface AppSettings {
  hotkey: HotkeyBinding;
  inputDeviceName: string | null;
  backendId: string;
  launchAtLogin: boolean;
  clipboardRestore: boolean;
  maxRecordingSeconds: number;
  modelIdleTimeoutSeconds: number;
  enabled: boolean;
}

export interface BackendDescriptor {
  id: string;
  name: string;
  language: string;
  model: string;
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
  setupComplete: boolean;
  platform: string;
  wayland: boolean;
}

export interface SetupStatus {
  complete: boolean;
  serverFound: boolean;
  modelFound: boolean;
  serverPath: string | null;
  modelPath: string | null;
  manifestConfigured: boolean;
}

export interface DownloadProgress {
  asset: string;
  received: number;
  total: number | null;
  percent: number | null;
}
