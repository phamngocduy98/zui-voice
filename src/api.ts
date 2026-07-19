import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AppSnapshot,
  DictationState,
  DownloadProgress,
  SetupStatus
} from "./types";

const inTauri = () => "__TAURI_INTERNALS__" in window;

const fallback: AppSnapshot = {
  settings: {
    hotkey: { key: "RightAlt", consume: true },
    inputDeviceName: null,
    backendId: "parakeet-vietnamese",
    launchAtLogin: false,
    clipboardRestore: true,
    maxRecordingSeconds: 300,
    modelIdleTimeoutSeconds: 600,
    enabled: true,
    theme: "system",
    onboardingVersion: 1
  },
  state: { phase: "idle", backendStatus: "stopped" },
  backend: {
    id: "parakeet-vietnamese",
    name: "Parakeet CTC",
    language: "Vietnamese",
    model: "parakeet-ctc-0.6b-Vietnamese-q8_0.gguf"
  },
  setupComplete: true,
  onboardingComplete: true,
  platform: "browser",
  wayland: false
};

export async function getSnapshot(): Promise<AppSnapshot> {
  return inTauri() ? invoke("get_app_snapshot") : fallback;
}

export async function getSetupStatus(): Promise<SetupStatus> {
  return inTauri()
    ? invoke("get_setup_status")
    : { complete: true, serverFound: true, modelFound: true, serverPath: null, modelPath: null, manifestConfigured: false };
}

export async function updateSettings(settings: AppSettings): Promise<AppSnapshot> {
  return inTauri() ? invoke("update_settings", { settings }) : { ...fallback, settings };
}

export async function listInputDevices(): Promise<string[]> {
  return inTauri() ? invoke("list_input_devices") : ["Default microphone"];
}

export async function testMicrophone(preferredName: string | null): Promise<void> {
  if (inTauri()) await invoke("test_microphone", { preferredName });
}

export async function beginHotkeyTest(): Promise<void> {
  if (inTauri()) await invoke("begin_hotkey_test");
}

export async function getHotkeyTestStatus(): Promise<boolean> {
  return inTauri() ? invoke("get_hotkey_test_status") : false;
}

export async function cancelHotkeyTest(): Promise<void> {
  if (inTauri()) await invoke("cancel_hotkey_test");
}

export async function completeOnboarding(inputDeviceName: string | null): Promise<AppSnapshot> {
  return inTauri()
    ? invoke("complete_onboarding", { inputDeviceName })
    : fallback;
}

export async function startAssetDownload(): Promise<SetupStatus> {
  return invoke("download_assets");
}

export async function cancelAssetDownload(): Promise<void> {
  if (inTauri()) await invoke("cancel_asset_download");
}

export async function unloadModel(): Promise<AppSnapshot> {
  return inTauri()
    ? invoke("unload_model")
    : { ...fallback, state: { phase: "idle", backendStatus: "stopped" } };
}

export async function retryBackend(): Promise<AppSnapshot> {
  return inTauri() ? invoke("retry_backend") : fallback;
}

export async function debugStart(): Promise<void> {
  if (inTauri()) await invoke("debug_start_dictation");
}

export async function debugStop(): Promise<void> {
  if (inTauri()) await invoke("debug_stop_dictation");
}

export async function onState(handler: (state: DictationState) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<DictationState>("voice://state", (event) => handler(event.payload)) : () => undefined;
}

export async function onSpectrum(handler: (bins: number[]) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<number[]>("voice://spectrum", (event) => handler(event.payload)) : () => undefined;
}

export async function onDownload(handler: (value: DownloadProgress) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<DownloadProgress>("voice://download-progress", (event) => handler(event.payload)) : () => undefined;
}

export async function onHotkeyTest(handler: () => void): Promise<UnlistenFn> {
  return inTauri() ? listen("voice://hotkey-test", handler) : () => undefined;
}
