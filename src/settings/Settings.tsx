import { useEffect, useRef, useState } from "react";
import { getSnapshot, listInputDevices, openSystemAudioPermissionSettings, resetSubtitleOverlayPosition, retryBackend, setSubtitleOverlayLocked, startAssetDownload, startSubtitles, stopSubtitles, unloadModel, updateSettings } from "../api";
import type { AppSettings, AppSnapshot, ThemePreference } from "../types";
import { errorMessage, type UiPlatform } from "../ui";
import { MacOverlayHeader } from "./MacOverlayHeader";
import { SettingsSidebar } from "./SettingsSidebar";
import { AppearanceSettings } from "./sections/AppearanceSettings";
import { AudioSettings } from "./sections/AudioSettings";
import { DictationSettings } from "./sections/DictationSettings";
import { EngineSettings } from "./sections/EngineSettings";
import { LegalSettings } from "./sections/LegalSettings";
import { SubtitleSettings } from "./sections/SubtitleSettings";
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
  const [installing, setInstalling] = useState(false);
  const [engineError, setEngineError] = useState<string | null>(null);
  const [subtitleBusy, setSubtitleBusy] = useState(false);
  const [subtitleError, setSubtitleError] = useState<string | null>(null);
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
  const updateSubtitlePreferences = async (change: Partial<AppSettings["subtitles"]>) => {
    // Subtitle controls commit immediately with the complete latest draft. Cancelling the pending
    // general save therefore cannot discard another setting or later overwrite this preference.
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    const requestRevision = ++revision.current;
    const latest = { ...draft, subtitles: { ...draft.subtitles, ...change } };
    setDraft(latest);
    try {
      const next = await updateSettings(latest);
      if (requestRevision === revision.current) onChange(next);
    } catch (caught) {
      if (requestRevision === revision.current) {
        setSubtitleError(errorMessage(caught));
        setDraft(snapshot.settings);
      }
    }
  };
  const setSubtitleLocked = async (locked: boolean) => {
    setSubtitleBusy(true);
    setSubtitleError(null);
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    const requestRevision = ++revision.current;
    const latest = { ...draft, subtitles: { ...draft.subtitles, overlayLocked: locked } };
    setDraft(latest);
    try {
      await updateSettings(latest);
      const next = await setSubtitleOverlayLocked(locked);
      if (requestRevision === revision.current) onChange(next);
    } catch (caught) {
      if (requestRevision === revision.current) {
        setSubtitleError(errorMessage(caught));
        setDraft(snapshot.settings);
      }
    } finally {
      if (requestRevision === revision.current) setSubtitleBusy(false);
    }
  };
  const install = async () => {
    setInstalling(true);
    setEngineError(null);
    try {
      await startAssetDownload();
      const nextSnapshot = await getSnapshot();
      onChange(nextSnapshot);
      setDraft(nextSnapshot.settings);
    } catch (caught) {
      setEngineError(errorMessage(caught));
    } finally {
      setInstalling(false);
    }
  };
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

  const runSubtitleAction = async (action: () => Promise<AppSnapshot>) => {
    setSubtitleBusy(true);
    setSubtitleError(null);
    try { onChange(await action()); }
    catch (caught) {
      // A failed activation is already represented by subtitleState.error in the returned/event
      // snapshot when the runtime can publish one. Keep only local transport failures here.
      const message = errorMessage(caught);
      if (snapshot.subtitleState.phase !== "error" || snapshot.subtitleState.error.message !== message) {
        setSubtitleError(message);
      }
    } finally { setSubtitleBusy(false); }
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
                  platform={platform}
                  onEnabledChange={(value) => patch("enabled", value)}
                  onHotkeyChange={(key) => commit({ ...draft, hotkey: { ...draft.hotkey, key } })}
                  onConsumeChange={(consume) => commit({ ...draft, hotkey: { ...draft.hotkey, consume } })}
                  onRecordingLimitChange={(value) => patch("maxRecordingSeconds", value)}
                  onClipboardRestoreChange={(value) => patch("clipboardRestore", value)}
                  onLaunchAtLoginChange={(value) => patch("launchAtLogin", value)}
                />
              )}
              {activeSection === "audio" && (
                <AudioSettings
                  settings={draft}
                  devices={devices}
                  onInputDeviceChange={(value) => patch("inputDeviceName", value)}
                />
              )}
              {activeSection === "subtitles" && (
                <SubtitleSettings
                  snapshot={{ ...snapshot, settings: draft }}
                  busy={subtitleBusy}
                  error={subtitleError}
                  onStart={() => void runSubtitleAction(startSubtitles)}
                  onStop={() => void runSubtitleAction(stopSubtitles)}
                  onLock={(locked) => void setSubtitleLocked(locked)}
                  onReset={() => void runSubtitleAction(resetSubtitleOverlayPosition)}
                  onOpenSystemSettings={() => { void openSystemAudioPermissionSettings().catch((caught) => setSubtitleError(errorMessage(caught))); }}
                  onLines={(maxLines) => void updateSubtitlePreferences({ maxLines })}
                />
              )}
              {activeSection === "engine" && (
                <EngineSettings
                  settings={draft}
                  backends={snapshot.backends}
                  status={backendStatus}
                  unloading={unloading}
                  installing={installing}
                  checking={checking}
                  error={engineError}
                  onLocaleChange={(value) => patch("locale", value)}
                  onInstall={install}
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
