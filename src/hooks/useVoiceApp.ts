import { useEffect, useState } from "react";
import { getSetupStatus, getSnapshot, onSpectrum, onState, onSubtitleLock, onSubtitleState } from "../api";
import type { AppSnapshot, DictationState, SetupStatus, ThemePreference } from "../types";
import { EMPTY_SPECTRUM_BINS, errorMessage } from "../ui";

export function useVoiceApp() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [bins, setBins] = useState(EMPTY_SPECTRUM_BINS);
  const [themePreference, setThemePreference] = useState<ThemePreference>("system");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  useEffect(() => {
    let disposed = false;
    let latestState: DictationState | null = null;
    let latestSubtitleState: AppSnapshot["subtitleState"] | null = null;
    let latestSubtitleLocked: boolean | null = null;
    const listeners: Array<() => void> = [];
    const keepListener = (unlisten: () => void) => {
      if (disposed) unlisten();
      else listeners.push(unlisten);
    };
    const load = async () => {
      setLoadError(null);
      try {
        await Promise.all([
          onState((state) => {
            latestState = state;
            setSnapshot((old) => old ? { ...old, state } : old);
          }).then(keepListener),
          onSubtitleState((subtitleState) => {
            latestSubtitleState = subtitleState;
            setSnapshot((old) => old ? { ...old, subtitleState } : old);
          }).then(keepListener),
          onSubtitleLock((overlayLocked) => {
            latestSubtitleLocked = overlayLocked;
            setSnapshot((old) => old ? { ...old, settings: { ...old.settings, subtitles: { ...old.settings.subtitles, overlayLocked } } } : old);
          }).then(keepListener),
          onSpectrum(setBins).then(keepListener)
        ]);
        const [value, setupStatus] = await Promise.all([getSnapshot(), getSetupStatus()]);
        if (disposed) return;
        const current = {
          ...value,
          state: latestState ?? value.state,
          subtitleState: latestSubtitleState ?? value.subtitleState,
          settings: latestSubtitleLocked === null ? value.settings : {
            ...value.settings,
            subtitles: { ...value.settings.subtitles, overlayLocked: latestSubtitleLocked }
          }
        };
        setSnapshot(current);
        setSetup(setupStatus);
        setThemePreference(current.settings.theme);
        setShowOnboarding(!current.onboardingComplete || !setupStatus.complete);
      } catch (caught) {
        if (!disposed) setLoadError(errorMessage(caught));
      }
    };
    void load();
    return () => {
      disposed = true;
      listeners.forEach((unlisten) => unlisten());
    };
  }, [reloadToken]);

  return {
    bins,
    loadError,
    reload: () => setReloadToken((value) => value + 1),
    setSnapshot,
    setThemePreference,
    setup,
    showOnboarding,
    snapshot,
    themePreference
  };
}
