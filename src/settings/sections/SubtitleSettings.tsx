import { Headphones, RotateCcw } from "lucide-react";
import type { AppSnapshot } from "../../types";

const phaseLabel: Record<string, string> = {
  disabled: "Off",
  starting: "Preparing capture…",
  requestingPermission: "Checking access…",
  listening: "Listening",
  pausedForDictation: "Paused while dictation has priority",
  stopping: "Stopping…"
};

export function SubtitleSettings({
  snapshot,
  busy,
  error,
  onStart,
  onStop,
  onLock,
  onReset,
  onOpenSystemSettings,
  onLines
}: {
  snapshot: AppSnapshot;
  busy: boolean;
  error: string | null;
  onStart: () => void;
  onStop: () => void;
  onLock: (locked: boolean) => void;
  onReset: () => void;
  onOpenSystemSettings: () => void;
  onLines: (value: number) => void;
}) {
  const { subtitleState: state, systemAudioCapabilities: capability, settings } = snapshot;
  const active = !["disabled", "error"].includes(state.phase);
  const unavailable = !capability.available;
  const canOpenSystemSettings = capability.permission === "denied" || capability.permission === "notDetermined";
  const stateMessage = state.phase === "error"
    ? state.error.message
    : unavailable
      ? capability.detail
      : phaseLabel[state.phase] ?? state.phase;
  const actionLabel = busy ? (active ? "Stopping…" : "Starting…") : active ? "Stop" : "Start";

  return (
    <>
      <section className="settings-section">
        <h2>Live subtitles</h2>
        <div className="settings-card">
          <div className="engine-row subtitle-engine-row">
            <span className="engine-icon"><Headphones /></span>
            <div><strong>Audio playing on this computer</strong><small>{capability.implementation}</small></div>
            <button
              type="button"
              className="secondary-button subtitle-action"
              disabled={busy || (!active && unavailable)}
              onClick={active ? onStop : onStart}
            >{actionLabel}</button>
          </div>
          <div className={`subtitle-status-callout${state.phase === "error" ? " is-error" : ""}`} role={state.phase === "error" ? "alert" : "status"}>
            <div><strong>Status</strong><small>{stateMessage}</small></div>
            {canOpenSystemSettings && <button type="button" className="secondary-button subtitle-settings-action" disabled={busy} onClick={onOpenSystemSettings}>Open settings</button>}
          </div>
        </div>
        {!error || error === (state.phase === "error" ? state.error.message : "") ? null : <p className="save-error" role="alert">{error}</p>}
        <p className="legal-footnote">Subtitles are opt-in. They capture computer audio only while active, remain local in bounded memory, keep no history, and pause/drop audio while dictation has priority.</p>
      </section>
      <section className="settings-section">
        <h2>Overlay</h2>
        <div className="settings-card">
          <div className="setting-row">
            <div className="setting-copy"><strong>Lock subtitle position</strong><small>When locked, the subtitle panel is click-through. Unlock it here or from the tray to move it.</small></div>
            <button className={`toggle ${settings.subtitles.overlayLocked ? "on" : ""}`} type="button" aria-label="Lock subtitle position" aria-pressed={settings.subtitles.overlayLocked} onClick={() => onLock(!settings.subtitles.overlayLocked)}><span /></button>
          </div>
          <div className="setting-row"><div className="setting-copy"><strong>Reset position</strong><small>Return the subtitle panel to its default visible location.</small></div><button type="button" className="secondary-button" onClick={onReset}><RotateCcw /> Reset</button></div>
          <div className="range-setting"><div><strong>Maximum lines</strong><small>Keep the live panel compact.</small></div><div className="range-control"><input aria-label="Maximum subtitle lines" type="range" min="1" max="6" value={settings.subtitles.maxLines} onChange={(event) => onLines(Number(event.target.value))} /><output>{settings.subtitles.maxLines}</output></div></div>
        </div>
      </section>
    </>
  );
}
