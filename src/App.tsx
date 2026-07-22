import { CircleAlert, Mic2 } from "lucide-react";
import { Onboarding } from "./components/Onboarding";
import { Overlay } from "./components/Overlay";
import { SubtitleOverlay } from "./components/SubtitleOverlay";
import { useAppTheme } from "./hooks/useAppTheme";
import { useVoiceApp } from "./hooks/useVoiceApp";
import { Settings } from "./settings/Settings";
import { formatHotkeyLabel, normalizeUiPlatform } from "./ui";

export function App() {
  const view = new URLSearchParams(location.search).get("view");
  const isOverlay = view === "overlay";
  const isSubtitle = view === "subtitle";
  const {
    bins,
    loadError,
    reload,
    setSnapshot,
    setThemePreference,
    setup,
    showOnboarding,
    snapshot,
    themePreference
  } = useVoiceApp();

  useAppTheme(themePreference);

  if (loadError && !snapshot) {
    return (
      <div className="splash splash-error" role="alert">
        <CircleAlert />
        <strong>Couldn’t load Zui. Voice</strong>
        <span>{loadError}</span>
        <button type="button" className="secondary-button" onClick={reload}>Try Again</button>
      </div>
    );
  }
  if (!snapshot) return <div className="splash"><div className="brand-mark"><Mic2 /></div></div>;

  const platform = normalizeUiPlatform(snapshot.platform);
  const hotkeyLabel = formatHotkeyLabel(snapshot.settings.hotkey.key, platform);

  if (isOverlay) {
    return <Overlay state={snapshot.state} bins={bins} />;
  }
  if (isSubtitle) {
    return <SubtitleOverlay settings={snapshot.settings.subtitles} />;
  }
  if (showOnboarding && setup) {
    return (
      <Onboarding
        setup={setup}
        snapshot={snapshot}
        platform={platform}
        inputDeviceName={snapshot.settings.inputDeviceName}
        onChange={setSnapshot}
        onReady={reload}
      />
    );
  }
  return (
    <Settings
      snapshot={snapshot}
      onChange={setSnapshot}
      onThemePreview={setThemePreference}
      platform={platform}
      hotkeyLabel={hotkeyLabel}
    />
  );
}
