import { useEffect, useRef, useState } from "react";
import { listInputDevices, retryBackend, unloadModel, updateSettings } from "../api";
import type { AppSettings, AppSnapshot, ThemePreference } from "../types";
import type { UiPlatform } from "../ui";
import { errorMessage } from "../ui";
import { MacOverlayHeader } from "./MacOverlayHeader";
import { SettingsSidebar } from "./SettingsSidebar";
import { AppearanceSettings } from "./sections/AppearanceSettings";
import { AudioSettings } from "./sections/AudioSettings";
import { DictationSettings } from "./sections/DictationSettings";
import { EngineSettings } from "./sections/EngineSettings";
import { LegalSettings } from "./sections/LegalSettings";
import { sectionCopy, type EngineStatus, type SaveState, type SettingsSection } from "./types";

export function Settings({ snapshot, onChange, onThemePreview, platform, hotkeyLabel }: {
  snapshot: AppSnapshot;
  onChange: (value: AppSnapshot) => void;
  onThemePreview: (value: ThemePreference) => void;
  platform: UiPlatform;
  hotkeyLabel: string;
}) {
  const [draft, setDraft] = useState(snapshot.settings);
  const [devices, setDevices] = useState<string[]>([]);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [activeSection, setActiveSection] = useState<SettingsSection>("dictation");
  const [searchQuery, setSearchQuery] = useState("");
  const [unloading, setUnloading] = useState(false);
  const [checking, setChecking] = useState(false);
  const [engineError, setEngineError] = useState<string | null>(null);
  const saveTimer = useRef<number | null>(null);
  const revision = useRef(0);

  useEffect(() => { listInputDevices().then(setDevices).catch(() => setDevices([])); }, []);
  useEffect(() => setDraft(snapshot.settings), [snapshot.settings]);
  useEffect(() => () => {
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
  }, []);

  const commit = (next: AppSettings) => {
    setDraft(next);
    setSaveState("saving");
    const currentRevision = ++revision.current;
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(async () => {
      try {
        const nextSnapshot = await updateSettings(next);
        if (currentRevision !== revision.current) return;
        onChange(nextSnapshot);
        setSaveState("saved");
      } catch {
        if (currentRevision === revision.current) {
          setDraft(snapshot.settings);
          onThemePreview(snapshot.settings.theme);
          setSaveState("error");
        }
      }
    }, 350);
  };

  const patch = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => commit({ ...draft, [key]: value });
  const setTheme = (theme: ThemePreference) => {
    onThemePreview(theme);
    patch("theme", theme);
  };
  const unload = async () => {
    setUnloading(true);
    setEngineError(null);
    try {
      onChange(await unloadModel());
    } catch {
      setEngineError("Couldn’t unload the model. Try again.");
    } finally {
      setUnloading(false);
    }
  };
  const retry = async () => {
    setChecking(true);
    setEngineError(null);
    try {
      onChange(await retryBackend());
    } catch (caught) {
      setEngineError(errorMessage(caught));
    } finally {
      setChecking(false);
    }
  };

  const activeCopy = sectionCopy[activeSection];
  const backendStatus: EngineStatus = snapshot.state.phase === "idle"
    ? snapshot.state.backendStatus
    : snapshot.state.phase === "loading"
      ? "loading"
      : snapshot.state.phase === "error"
        ? "error"
        : "ready";

  return (
    <div className="app-frame">
      <div className="settings-window" data-platform={platform}>
        {platform === "macos" && <MacOverlayHeader title={activeCopy.title} />}

        <div className="window-body">
          <SettingsSidebar
            activeSection={activeSection}
            onSectionChange={setActiveSection}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
          />

          <main className="settings-main">
            <div className="settings-content">
              {platform !== "macos" && <h1 className="pane-title">{activeCopy.title}</h1>}
              <p className="pane-intro">{activeCopy.detail}</p>
              {saveState === "error" && <p className="save-error" role="alert">Couldn’t save your changes. Try again.</p>}

              {activeSection === "dictation" && (
                <DictationSettings
                  settings={draft}
                  hotkeyLabel={hotkeyLabel}
                  onEnabledChange={(value) => patch("enabled", value)}
                  onConsumeChange={(consume) => commit({ ...draft, hotkey: { ...draft.hotkey, consume } })}
                  onClipboardRestoreChange={(value) => patch("clipboardRestore", value)}
                  onLaunchAtLoginChange={(value) => patch("launchAtLogin", value)}
                />
              )}
              {activeSection === "audio" && (
                <AudioSettings
                  settings={draft}
                  devices={devices}
                  onInputDeviceChange={(value) => patch("inputDeviceName", value)}
                  onRecordingLimitChange={(value) => patch("maxRecordingSeconds", value)}
                />
              )}
              {activeSection === "engine" && (
                <EngineSettings
                  settings={draft}
                  status={backendStatus}
                  unloading={unloading}
                  checking={checking}
                  error={engineError}
                  onUnload={unload}
                  onRetry={retry}
                  onIdleTimeoutChange={(value) => patch("modelIdleTimeoutSeconds", value)}
                />
              )}
              {activeSection === "appearance" && <AppearanceSettings settings={draft} onThemeChange={setTheme} />}
              {activeSection === "legal" && <LegalSettings />}
            </div>
          </main>
        </div>
      </div>
    </div>
  );
}
