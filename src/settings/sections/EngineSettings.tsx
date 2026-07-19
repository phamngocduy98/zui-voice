import { Activity, RotateCcw } from "lucide-react";
import type { AppSettings } from "../../types";
import { SettingRow } from "../SettingsControls";
import type { EngineStatus } from "../types";

export function EngineSettings({ settings, status, unloading, checking, error, onUnload, onRetry, onIdleTimeoutChange }: {
  settings: AppSettings;
  status: EngineStatus;
  unloading: boolean;
  checking: boolean;
  error: string | null;
  onUnload: () => Promise<void>;
  onRetry: () => Promise<void>;
  onIdleTimeoutChange: (value: number) => void;
}) {
  const statusLabel = status === "ready" ? "Ready" : status === "loading" ? "Loading" : status === "error" ? "Error" : "Stopped";

  return (
    <>
      {error && <p className="save-error" role="alert">{error}</p>}
      <section className="settings-section">
        <h2>Transcription Model</h2>
        <div className="settings-card">
          <div className="engine-row">
            <span className="engine-icon"><Activity /></span>
            <div><strong>Parakeet CTC</strong><small>Vietnamese · Q8 · local only</small></div>
            <span className={"engine-status " + status}><i />{statusLabel}</span>
            <button type="button" className="secondary-button" disabled={unloading || status === "stopped"} onClick={() => void onUnload()}>{unloading ? "Unloading…" : "Unload"}</button>
          </div>
          <div className="range-setting">
            <div><strong>Unload after idle</strong><small>Release memory when inactive</small></div>
            <div className="range-control">
              <input aria-label="Unload after idle" type="range" min="60" max="1800" step="60" value={settings.modelIdleTimeoutSeconds} onChange={(event) => onIdleTimeoutChange(Number(event.target.value))} />
              <output>{Math.round(settings.modelIdleTimeoutSeconds / 60)} min</output>
            </div>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2>Diagnostics</h2>
        <div className="settings-card">
          <SettingRow title="Speech engine" detail="Check that the local engine is available">
            <button type="button" className="secondary-button" disabled={checking} onClick={() => void onRetry()}><RotateCcw />{checking ? "Checking…" : "Check Now"}</button>
          </SettingRow>
        </div>
      </section>
    </>
  );
}
