import { useEffect, useRef, useState } from "react";
import { getSnapshot, onSubtitleClear, onSubtitleLock, onSubtitleSettings, onSubtitleState, onSubtitleText } from "../api";
import type { SubtitleSettings, SubtitleState, SubtitleText } from "../types";

export interface SubtitleDisplay {
  state: SubtitleState;
  text: SubtitleText | null;
  settings: SubtitleSettings | null;
}

/** Subscribe before fetching state so fast startup events cannot be lost. */
export function useSubtitles() {
  const [display, setDisplay] = useState<SubtitleDisplay>({ state: { phase: "disabled" }, text: null, settings: null });
  const session = useRef(0);
  const revision = useRef(0);

  useEffect(() => {
    let disposed = false;
    let stateEventSeen = false;
    let settingsEventSeen = false;
    let latestOverlayLocked: boolean | null = null;
    const listeners: Array<() => void> = [];
    const retain = (unlisten: () => void) => disposed ? unlisten() : listeners.push(unlisten);
    void Promise.all([
      onSubtitleState((state) => {
        stateEventSeen = true;
        setDisplay((current) => ({ ...current, state }));
      }).then(retain),
      onSubtitleText((text) => {
        if (text.sessionId < session.current || (text.sessionId === session.current && text.revision <= revision.current)) return;
        session.current = text.sessionId;
        revision.current = text.revision;
        setDisplay((current) => ({ ...current, text }));
      }).then(retain),
      onSubtitleClear((sessionId) => {
        if (sessionId < session.current) return;
        session.current = sessionId;
        revision.current = 0;
        setDisplay((current) => ({ ...current, text: null }));
      }).then(retain),
      onSubtitleSettings((settings) => {
        settingsEventSeen = true;
        setDisplay((current) => ({ ...current, settings }));
      }).then(retain),
      onSubtitleLock((overlayLocked) => {
        latestOverlayLocked = overlayLocked;
        setDisplay((current) => ({
          ...current,
          settings: current.settings ? { ...current.settings, overlayLocked } : current.settings
        }));
      }).then(retain)
    ]).then(async () => {
      const snapshot = await getSnapshot();
      if (disposed) return;
      setDisplay((current) => ({
        ...current,
        state: stateEventSeen ? current.state : snapshot.subtitleState,
        settings: settingsEventSeen
          ? current.settings
          : { ...snapshot.settings.subtitles, overlayLocked: latestOverlayLocked ?? snapshot.settings.subtitles.overlayLocked }
      }));
    }).catch(() => undefined);
    return () => { disposed = true; listeners.forEach((unlisten) => unlisten()); };
  }, []);

  return display;
}
