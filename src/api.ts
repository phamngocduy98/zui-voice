import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AppSnapshot,
  DictationState,
  DownloadProgress,
  SetupStatus,
  SubtitleState,
  SubtitleText,
  SystemAudioCapabilities
} from "./types";

const inTauri = () => "__TAURI_INTERNALS__" in window;

const ready = (locale: string, name: string) => ({ locale, name, tier: "transcriptionReady" as const });
const broad = (locale: string, name: string) => ({ locale, name, tier: "broadCoverage" as const });
const fallbackBackends = [
  {
    id: "nemotron-3.5-asr-streaming-0.6b",
    name: "Nemotron 3.5 ASR",
    language: "Multilingual",
    description: "32 production-ready locales, Q8, local only",
    model: "nemotron-3.5-asr-streaming-0.6b-q8_0.gguf",
    installed: true,
    locales: [
      ready("en-US", "English (United States)"), ready("en-GB", "English (United Kingdom)"),
      ready("es-US", "Spanish (United States)"), ready("es-ES", "Spanish (Spain)"),
      ready("fr-FR", "French (France)"), ready("fr-CA", "French (Canada)"),
      ready("de-DE", "German (Germany)"), ready("it-IT", "Italian (Italy)"),
      ready("pt-BR", "Portuguese (Brazil)"), ready("pt-PT", "Portuguese (Portugal)"),
      ready("nl-NL", "Dutch (Netherlands)"), ready("ru-RU", "Russian (Russia)"),
      ready("ja-JP", "Japanese (Japan)"), ready("ko-KR", "Korean (South Korea)"),
      ready("hi-IN", "Hindi (India)"), ready("ar-AR", "Arabic"),
      ready("tr-TR", "Turkish (Turkey)"), ready("vi-VN", "Vietnamese (Vietnam)"),
      ready("uk-UA", "Ukrainian (Ukraine)"), broad("pl-PL", "Polish (Poland)"),
      broad("sv-SE", "Swedish (Sweden)"), broad("cs-CZ", "Czech (Czechia)"),
      broad("nb-NO", "Norwegian Bokmal (Norway)"), broad("da-DK", "Danish (Denmark)"),
      broad("bg-BG", "Bulgarian (Bulgaria)"), broad("fi-FI", "Finnish (Finland)"),
      broad("hr-HR", "Croatian (Croatia)"), broad("sk-SK", "Slovak (Slovakia)"),
      broad("zh-CN", "Mandarin (China)"), broad("hu-HU", "Hungarian (Hungary)"),
      broad("ro-RO", "Romanian (Romania)"), broad("et-EE", "Estonian (Estonia)")
    ]
  }
];

const fallback: AppSnapshot = {
  settings: {
    hotkey: { key: "RightAlt", consume: true },
    inputDeviceName: null,
    backendId: "nemotron-3.5-asr-streaming-0.6b",
    locale: "vi-VN",
    launchAtLogin: false,
    clipboardRestore: true,
    maxRecordingSeconds: 300,
    modelIdleTimeoutSeconds: 600,
    enabled: true,
    theme: "system",
    onboardingVersion: 1,
    subtitles: { overlayLocked: false, position: null, maxLines: 3 }
  },
  state: { phase: "idle", backendStatus: "stopped" },
  backend: fallbackBackends[0],
  backends: fallbackBackends,
  setupComplete: true,
  onboardingComplete: true,
  platform: "browser",
  wayland: false,
  subtitleState: { phase: "disabled" },
  systemAudioCapabilities: {
    available: false,
    permission: "unavailable",
    implementation: "Browser preview",
    detail: "System-output capture is available only in a supported desktop release."
  }
};

export async function getSnapshot(): Promise<AppSnapshot> {
  return inTauri() ? invoke("get_app_snapshot") : fallback;
}

export async function getSetupStatus(): Promise<SetupStatus> {
  return inTauri()
    ? invoke("get_setup_status")
    : { backendId: fallback.settings.backendId, complete: true, serverFound: true, modelFound: true, serverPath: null, modelPath: null, manifestConfigured: false };
}

export async function updateSettings(settings: AppSettings): Promise<AppSnapshot> {
  if (inTauri()) return invoke("update_settings", { settings });
  const backend = fallbackBackends.find((item) => item.id === settings.backendId) ?? fallback.backend;
  return { ...fallback, settings, backend };
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

export async function confirmHotkeyTest(): Promise<boolean> {
  return inTauri() ? invoke("confirm_hotkey_test") : true;
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

export async function getSystemAudioCapabilities(): Promise<SystemAudioCapabilities> {
  return inTauri() ? invoke("get_system_audio_capabilities") : fallback.systemAudioCapabilities;
}

export async function openSystemAudioPermissionSettings(): Promise<void> {
  if (inTauri()) await invoke("open_system_audio_permission_settings");
}

export async function startSubtitles(): Promise<AppSnapshot> {
  return inTauri() ? invoke("start_subtitles") : Promise.reject(new Error(fallback.systemAudioCapabilities.detail));
}

export async function stopSubtitles(): Promise<AppSnapshot> {
  return inTauri() ? invoke("stop_subtitles") : { ...fallback, subtitleState: { phase: "disabled" } };
}

export async function setSubtitleOverlayLocked(locked: boolean): Promise<AppSnapshot> {
  return inTauri()
    ? invoke("set_subtitle_overlay_locked", { locked })
    : { ...fallback, settings: { ...fallback.settings, subtitles: { ...fallback.settings.subtitles, overlayLocked: locked } } };
}

export async function resetSubtitleOverlayPosition(): Promise<AppSnapshot> {
  return inTauri() ? invoke("reset_subtitle_overlay_position") : fallback;
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

export async function onSubtitleState(handler: (state: SubtitleState) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<SubtitleState>("subtitle://state", (event) => handler(event.payload)) : () => undefined;
}

export async function onSubtitleText(handler: (text: SubtitleText) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<SubtitleText>("subtitle://text", (event) => handler(event.payload)) : () => undefined;
}

export async function onSubtitleClear(handler: (sessionId: number) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<number>("subtitle://clear", (event) => handler(event.payload)) : () => undefined;
}

export async function onSubtitleLock(handler: (locked: boolean) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<boolean>("subtitle://lock", (event) => handler(event.payload)) : () => undefined;
}

export async function onSubtitleSettings(handler: (settings: AppSettings["subtitles"]) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<AppSettings["subtitles"]>("subtitle://settings", (event) => handler(event.payload)) : () => undefined;
}

export async function onDownload(handler: (value: DownloadProgress) => void): Promise<UnlistenFn> {
  return inTauri() ? listen<DownloadProgress>("voice://download-progress", (event) => handler(event.payload)) : () => undefined;
}

export async function onHotkeyTest(handler: () => void): Promise<UnlistenFn> {
  return inTauri() ? listen("voice://hotkey-test", handler) : () => undefined;
}
