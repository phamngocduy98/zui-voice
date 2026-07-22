import { useEffect, useRef } from "react";
import { GripHorizontal, Pause, Radio, TriangleAlert } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSubtitles } from "../hooks/useSubtitles";
import type { SubtitleSettings } from "../types";

export function SubtitleOverlay({ settings: initialSettings }: { settings: SubtitleSettings }) {
  const { state, text, settings: liveSettings } = useSubtitles();
  const settings = liveSettings ?? initialSettings;
  const status = state.phase === "pausedForDictation" ? "Paused while dictating" : state.phase === "error" ? state.error.message : "Live subtitles";
  const icon = state.phase === "error" ? <TriangleAlert /> : state.phase === "pausedForDictation" ? <Pause /> : <Radio />;
  const textRef = useRef<HTMLParagraphElement>(null);
  useEffect(() => { textRef.current?.scrollTo?.({ top: textRef.current.scrollHeight }); }, [text?.revision]);

  return (
    <main className={`subtitle-shell phase-${state.phase}`} style={{ ["--subtitle-lines" as string]: settings.maxLines }}>
      {!settings.overlayLocked && <div className="subtitle-drag" aria-hidden="true" onMouseDown={() => { if ("__TAURI_INTERNALS__" in window) void getCurrentWindow().startDragging(); }}><GripHorizontal /></div>}
      <div className="subtitle-status" aria-live="polite">{icon}<span>{status}</span></div>
      <p ref={textRef} className="subtitle-text" aria-live={text?.isFinal ? "polite" : "off"}>
        <span className="subtitle-stable">{text?.stableText}</span>
        <span className="subtitle-unstable">{text?.unstableText}</span>
      </p>
    </main>
  );
}
