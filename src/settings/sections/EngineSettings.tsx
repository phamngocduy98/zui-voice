import { Activity, Download, RotateCcw } from "lucide-react";
import type { AppSettings, BackendDescriptor } from "../../types";
import type { EngineStatus } from "../types";

export function EngineSettings({ settings, backends, status, unloading, installing, checking, error, onLocaleChange, onInstall, onUnload, onRetry, onIdleTimeoutChange }: {
  settings: AppSettings;
  backends: BackendDescriptor[];
  status: EngineStatus;
  unloading: boolean;
  installing: boolean;
  checking: boolean;
  error: string | null;
  onLocaleChange: (value: string) => void;
  onInstall: () => Promise<void>;
  onUnload: () => Promise<void>;
  onRetry: () => Promise<void>;
  onIdleTimeoutChange: (value: number) => void;
}) {
  const statusLabel = status === "ready" ? "Ready" : status === "loading" ? "Loading" : status === "error" ? "Error" : "Stopped";
  const selected = backends.find((backend) => backend.id === settings.backendId) ?? backends[0];
  const readyLocales = selected?.locales.filter((locale) => locale.tier === "transcriptionReady") ?? [];
  const broadLocales = selected?.locales.filter((locale) => locale.tier === "broadCoverage") ?? [];

  return (
    <>
      {error && <p className="save-error" role="alert">{error}</p>}
      <section className="settings-section">
        <h2>Transcription Model</h2>
        <div className="settings-card">
          <div className="engine-row">
            <span className="engine-icon"><Activity /></span>
            <div><strong>{selected?.name ?? "Local speech model"}</strong><small>{selected?.installed ? "Loaded only when needed" : "Install this model to begin dictating"}</small></div>
            <span className={"engine-status " + (selected?.installed ? status : "stopped")}><i />{selected?.installed ? statusLabel : "Missing"}</span>
            {selected?.installed
              ? status === "stopped" || status === "error"
                ? <button type="button" className="secondary-button" disabled={checking} onClick={() => void onRetry()}><RotateCcw />{checking ? "Starting…" : status === "error" ? "Retry" : "Load"}</button>
                : <button type="button" className="secondary-button" disabled={unloading} onClick={() => void onUnload()}>{unloading ? "Unloading…" : "Unload"}</button>
              : <button type="button" className="secondary-button" disabled={installing} onClick={() => void onInstall()}><Download />{installing ? "Installing…" : "Install"}</button>}
          </div>
          <div className="setting-row language-row">
            <div className="setting-copy"><strong>Transcription language</strong><small>Use an explicit locale for the best recognition quality</small></div>
            <div className="setting-control">
              <select aria-label="Transcription language" value={settings.locale} onChange={(event) => onLocaleChange(event.target.value)}>
                <optgroup label="Transcription ready">
                  {readyLocales.map((locale) => <option value={locale.locale} key={locale.locale}>{locale.name}</option>)}
                </optgroup>
                {broadLocales.length > 0 && <optgroup label="Broad coverage">
                  {broadLocales.map((locale) => <option value={locale.locale} key={locale.locale}>{locale.name}</option>)}
                </optgroup>}
              </select>
            </div>
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

    </>
  );
}
