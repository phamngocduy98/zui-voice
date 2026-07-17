import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  Check,
  ChevronRight,
  CircleAlert,
  ClipboardCheck,
  Download,
  Gauge,
  Headphones,
  Keyboard,
  Mic2,
  Power,
  RotateCcw,
  Settings2,
  Sparkles,
  X
} from "lucide-react";
import {
  cancelAssetDownload,
  debugStart,
  debugStop,
  getSetupStatus,
  getSnapshot,
  listInputDevices,
  onDownload,
  onSpectrum,
  onState,
  retryBackend,
  startAssetDownload,
  unloadModel,
  updateSettings
} from "./api";
import type { AppSettings, AppSnapshot, DictationState, DownloadProgress, SetupStatus } from "./types";

const emptyBins = Array.from({ length: 24 }, (_, i) => 0.16 + Math.sin(i * 1.7) * 0.03);

function Spectrum({ bins, active }: { bins: number[]; active: boolean }) {
  return (
    <div className={`spectrum ${active ? "is-active" : ""}`} aria-hidden="true">
      {bins.map((value, index) => (
        <i key={index} style={{ height: `${Math.max(8, Math.min(100, value * 100))}%` }} />
      ))}
    </div>
  );
}

const stateCopy = (state: DictationState) => {
  switch (state.phase) {
    case "recording": return ["Listening", "Speak naturally"];
    case "loading": return ["Warming up", state.detail];
    case "transcribing": return ["Transcribing", "Vietnamese · local"];
    case "pasting": return ["Inserting", "Restoring clipboard"];
    case "success": return ["Done", "Text inserted"];
    case "copied": return ["Copied", state.reason];
    case "error": return ["Couldn’t finish", state.error.message];
    case "setupRequired": return ["Setup needed", state.detail];
    default: return ["Ready", "Hold Right Alt"];
  }
};

function Overlay({ state, bins }: { state: DictationState; bins: number[] }) {
  const [title, detail] = stateCopy(state);
  const isRecording = state.phase === "recording";
  const isSuccess = state.phase === "success";
  const isCopied = state.phase === "copied";
  const isError = state.phase === "error";
  const busy = ["loading", "transcribing", "pasting"].includes(state.phase);

  return (
    <main className={`overlay-shell phase-${state.phase}`}>
      <div className="overlay-glow" />
      <div className="status-orb">
        {isSuccess ? <Check size={18} /> : isCopied ? <ClipboardCheck size={17} /> : isError ? <CircleAlert size={17} /> : <Mic2 size={18} />}
        {isRecording && <span className="pulse-ring" />}
      </div>
      <div className="overlay-copy">
        <strong>{title}</strong>
        <span>{detail}</span>
      </div>
      <Spectrum bins={isRecording ? bins : busy ? bins.map((_, i) => 0.2 + ((i * 7) % 8) / 15) : emptyBins} active={isRecording || busy} />
    </main>
  );
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button className={`toggle ${checked ? "on" : ""}`} onClick={() => onChange(!checked)} role="switch" aria-checked={checked} aria-label={label}>
      <span />
    </button>
  );
}

function Onboarding({ setup, onReady }: { setup: SetupStatus; onReady: () => void }) {
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: () => void = () => undefined;
    onDownload(setProgress).then((fn) => { unlisten = fn; });
    return () => unlisten();
  }, []);

  const download = async () => {
    setWorking(true);
    setError(null);
    try {
      const result = await startAssetDownload();
      if (result.complete) onReady();
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  };

  const cancel = async () => {
    await cancelAssetDownload();
  };

  return (
    <div className="onboarding">
      <div className="brand-mark"><Sparkles size={22} /></div>
      <p className="eyebrow">WELCOME TO</p>
      <h1>Zui. <em>Voice</em></h1>
      <p className="lede">Private Vietnamese dictation that stays on your computer.</p>
      <div className="setup-card">
        <div className={setup.serverFound ? "step complete" : "step"}><span>{setup.serverFound ? <Check /> : "1"}</span><div><strong>Speech engine</strong><small>Parakeet native runtime</small></div></div>
        <div className={setup.modelFound ? "step complete" : "step"}><span>{setup.modelFound ? <Check /> : "2"}</span><div><strong>Vietnamese model</strong><small>Q8 · approximately 875 MB</small></div></div>
        <div className="step"><span>3</span><div><strong>Permissions</strong><small>Microphone and accessibility</small></div></div>
      </div>
      {progress && <div className="progress"><i style={{ width: `${progress.percent ?? 0}%` }} /><span>{progress.asset} · {progress.percent?.toFixed(0) ?? "…"}%</span></div>}
      {error && <p className="inline-error">{error}</p>}
      <button className="primary" onClick={setup.complete ? onReady : working ? cancel : download} disabled={!setup.manifestConfigured && !setup.complete}>
        {setup.complete ? "Continue" : working ? <><X size={17} /> Cancel download</> : setup.manifestConfigured ? <><Download size={17} /> Download model</> : "Release manifest not configured"}
        {!working && <ChevronRight size={17} />}
      </button>
      <p className="privacy-note">Audio and transcripts are never uploaded. Model terms and third-party notices are available in Settings.</p>
    </div>
  );
}

function Settings({ snapshot, onChange }: { snapshot: AppSnapshot; onChange: (s: AppSnapshot) => void }) {
  const [draft, setDraft] = useState(snapshot.settings);
  const [devices, setDevices] = useState<string[]>([]);
  const [saved, setSaved] = useState(false);

  useEffect(() => { listInputDevices().then(setDevices).catch(() => setDevices([])); }, []);
  useEffect(() => setDraft(snapshot.settings), [snapshot.settings]);

  const patch = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setDraft((old) => ({ ...old, [key]: value }));
  const patchConsume = (consume: boolean) => setDraft((old) => ({ ...old, hotkey: { ...old.hotkey, consume } }));
  const save = async () => {
    const next = await updateSettings(draft);
    onChange(next);
    setSaved(true);
    setTimeout(() => setSaved(false), 1200);
  };

  return (
    <div className="app-frame">
      <header>
        <div className="mini-brand"><span><Mic2 size={16} /></span><strong>Zui. Voice</strong></div>
        <div className={`ready-chip ${snapshot.state.phase}`}><i />{snapshot.state.phase === "idle" ? "Ready" : stateCopy(snapshot.state)[0]}</div>
      </header>
      <div className="hero-panel">
        <div><p className="eyebrow">PUSH TO TALK</p><h2>Hold <kbd>Right Alt</kbd>.<br />Speak. Release.</h2><p>Your voice becomes text in the app you were using.</p></div>
        <div className="hero-orb"><Mic2 /><span /></div>
      </div>
      <section>
        <h3><Settings2 size={16} /> Dictation</h3>
        <div className="settings-list">
          <label><span className="setting-icon"><Power /></span><div><strong>Dictation enabled</strong><small>Listen for the global hold key</small></div><Toggle checked={draft.enabled} onChange={(v) => patch("enabled", v)} label="Dictation enabled" /></label>
          <label><span className="setting-icon"><Keyboard /></span><div><strong>Hold key</strong><small>Right Alt · press and hold to record</small></div><kbd>Right Alt</kbd></label>
          <label><span className="setting-icon"><Keyboard /></span><div><strong>Consume hold key</strong><small>Prevent Right Alt from reaching the focused app</small></div><Toggle checked={draft.hotkey.consume} onChange={patchConsume} label="Consume hold key" /></label>
          <label><span className="setting-icon"><Headphones /></span><div><strong>Microphone</strong><small>Uses the system default when unchanged</small></div><select value={draft.inputDeviceName ?? ""} onChange={(e) => patch("inputDeviceName", e.target.value || null)}><option value="">System default</option>{devices.map((d) => <option key={d}>{d}</option>)}</select></label>
          <label><span className="setting-icon"><ClipboardCheck /></span><div><strong>Restore clipboard</strong><small>Preserve common formats after insertion</small></div><Toggle checked={draft.clipboardRestore} onChange={(v) => patch("clipboardRestore", v)} label="Restore clipboard" /></label>
          <label><span className="setting-icon"><Power /></span><div><strong>Launch at login</strong><small>Keep dictation one key away</small></div><Toggle checked={draft.launchAtLogin} onChange={(v) => patch("launchAtLogin", v)} label="Launch at login" /></label>
        </div>
        <div className="range-row"><span>Recording limit</span><input type="range" min="30" max="300" step="30" value={draft.maxRecordingSeconds} onChange={(e) => patch("maxRecordingSeconds", Number(e.target.value))} /><output>{Math.round(draft.maxRecordingSeconds / 60)} min</output></div>
      </section>
      <section>
        <h3><Gauge size={16} /> Local engine</h3>
        <div className="engine-card"><div className="engine-icon"><Activity /></div><div><strong>Parakeet CTC</strong><span>Vietnamese · Q8 · local only</span></div><button className="ghost" onClick={() => unloadModel()}>Unload</button></div>
        <div className="range-row"><span>Unload after idle</span><input type="range" min="60" max="1800" step="60" value={draft.modelIdleTimeoutSeconds} onChange={(e) => patch("modelIdleTimeoutSeconds", Number(e.target.value))} /><output>{Math.round(draft.modelIdleTimeoutSeconds / 60)} min</output></div>
      </section>
      <footer>
        <button className="ghost" onClick={() => retryBackend()}><RotateCcw size={15} /> Check engine</button>
        <div className="dev-controls"><button onMouseDown={() => debugStart()} onMouseUp={() => debugStop()}>Test mic</button></div>
        <button className="primary compact" onClick={save}>{saved ? <><Check size={16} /> Saved</> : "Save changes"}</button>
      </footer>
    </div>
  );
}

export function App() {
  const isOverlay = useMemo(() => new URLSearchParams(location.search).get("view") === "overlay", []);
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [bins, setBins] = useState(emptyBins);

  useEffect(() => {
    getSnapshot().then((value) => { setSnapshot(value); setShowOnboarding(!value.setupComplete); });
    getSetupStatus().then(setSetup);
    const listeners: Array<() => void> = [];
    onState((state) => setSnapshot((old) => old ? { ...old, state } : old)).then((fn) => listeners.push(fn));
    onSpectrum(setBins).then((fn) => listeners.push(fn));
    return () => listeners.forEach((fn) => fn());
  }, []);

  if (!snapshot) return <div className="splash"><div className="brand-mark"><Mic2 /></div></div>;
  if (isOverlay) return <Overlay state={snapshot.state} bins={bins} />;
  if (showOnboarding && setup) return <Onboarding setup={setup} onReady={() => { setShowOnboarding(false); getSnapshot().then(setSnapshot); }} />;
  return <Settings snapshot={snapshot} onChange={setSnapshot} />;
}
