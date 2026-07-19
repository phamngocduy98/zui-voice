import { useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronLeft,
  ChevronRight,
  ClipboardCheck,
  Cpu,
  Download,
  Keyboard,
  LoaderCircle,
  Mic2,
  Sparkles,
  X
} from "lucide-react";
import {
  cancelAssetDownload,
  completeOnboarding,
  getSetupStatus,
  listInputDevices,
  onDownload,
  retryBackend,
  startAssetDownload,
  testMicrophone,
  updateSettings
} from "../api";
import type { AppSettings, AppSnapshot, DownloadProgress, SetupStatus } from "../types";
import type { UiPlatform } from "../ui";
import { errorMessage, formatHotkeyLabel } from "../ui";
import { KeyRecorder, type KeyRecorderState } from "./KeyRecorder";

type Stage = "install" | "permissions" | "verifying" | "tutorial";

const stageNumber: Record<Stage, number> = {
  install: 1,
  permissions: 2,
  verifying: 3,
  tutorial: 4
};

const phaseCopy: Record<DownloadProgress["phase"], string> = {
  fetchingManifest: "Checking the release",
  downloading: "Downloading",
  verifying: "Verifying checksum",
  installing: "Installing securely"
};

function assetLabel(asset: string) {
  const value = asset.toLowerCase();
  if (value.includes("manifest")) return "Setup information";
  if (value.includes("model")) return "Speech model";
  if (value.includes("runtime") || value.includes("server")) return "Speech engine";
  return "Required component";
}

function formatBytes(value: number) {
  if (value < 1024 * 1024) return `${Math.max(1, Math.round(value / 1024))} KB`;
  return `${(value / (1024 * 1024)).toFixed(value >= 100 * 1024 * 1024 ? 0 : 1)} MB`;
}

function isCancelled(error: unknown) {
  return Boolean(error && typeof error === "object" && "code" in error && error.code === "download_cancelled");
}

export function Onboarding({ setup, snapshot, platform, inputDeviceName, onChange, onReady }: {
  setup: SetupStatus;
  snapshot: AppSnapshot;
  platform: UiPlatform;
  inputDeviceName: string | null;
  onChange: (value: AppSnapshot) => void;
  onReady: () => void;
}) {
  const onboardingRef = useRef<HTMLElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const initialStage = useRef(true);
  const [stage, setStage] = useState<Stage>("install");
  const [setupState, setSetupState] = useState(setup);
  const [selection, setSelection] = useState(snapshot.settings);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [selectedDevice, setSelectedDevice] = useState(inputDeviceName);
  const [keyRecorderState, setKeyRecorderState] = useState<KeyRecorderState>("idle");
  const [working, setWorking] = useState(false);
  const [selectionWorking, setSelectionWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectedBackend = snapshot.backends.find((backend) => backend.id === selection.backendId)
    ?? snapshot.backends[0];
  const configuredHotkeyLabel = formatHotkeyLabel(selection.hotkey.key, platform);

  const goToStage = (nextStage: Stage) => {
    setError(null);
    setStage(nextStage);
  };

  useEffect(() => {
    if (initialStage.current) {
      initialStage.current = false;
      return;
    }
    onboardingRef.current?.scrollTo?.({ top: 0 });
    headingRef.current?.focus({ preventScroll: true });
  }, [stage]);

  const saveSelection = async (next: AppSettings) => {
    setSelectionWorking(true);
    setError(null);
    try {
      const nextSnapshot = await updateSettings(next);
      setSelection(nextSnapshot.settings);
      onChange(nextSnapshot);
      setSetupState(await getSetupStatus());
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setSelectionWorking(false);
    }
  };

  const chooseBackend = (backendId: string) => {
    const backend = snapshot.backends.find((item) => item.id === backendId);
    if (!backend) return;
    const locale = backend.locales.some((item) => item.locale === selection.locale)
      ? selection.locale
      : backend.locales[0]?.locale ?? "vi-VN";
    void saveSelection({ ...selection, backendId, locale });
  };

  useEffect(() => {
    let disposed = false;
    let unlisten: () => void = () => undefined;
    onDownload(setProgress).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten();
    };
  }, []);

  useEffect(() => {
    if (!setupState.complete) return;
    let disposed = false;
    listInputDevices().then((value) => {
      if (!disposed) {
        setDevices(value);
        setSelectedDevice((current) => current && value.includes(current) ? current : null);
      }
    }).catch((caught) => {
      if (!disposed) setError(errorMessage(caught));
    });
    return () => { disposed = true; };
  }, [setupState.complete]);

  const download = async () => {
    setWorking(true);
    setProgress(null);
    setError(null);
    try {
      const result = await startAssetDownload();
      setSetupState(result);
      if (result.complete) goToStage("permissions");
      else setError("The download finished, but a required component is still missing.");
    } catch (caught) {
      setProgress(null);
      if (!isCancelled(caught)) setError(errorMessage(caught));
    } finally {
      setWorking(false);
    }
  };

  const cancel = async () => {
    setError(null);
    try {
      await cancelAssetDownload();
    } catch (caught) {
      setError(errorMessage(caught));
    }
  };

  const verifySetup = async () => {
    setWorking(true);
    setError(null);
    try {
      await testMicrophone(selectedDevice);
      goToStage("verifying");
      await retryBackend();
      goToStage("tutorial");
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setWorking(false);
    }
  };

  const verifyEngine = async () => {
    setWorking(true);
    setError(null);
    try {
      await retryBackend();
      goToStage("tutorial");
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setWorking(false);
    }
  };

  const finish = async () => {
    setWorking(true);
    setError(null);
    try {
      await completeOnboarding(selectedDevice);
      onReady();
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setWorking(false);
    }
  };

  const currentStage = stageNumber[stage];
  const platformDetail = platform === "windows"
    ? "Zui can insert the transcript into the app where dictation began."
    : platform === "macos"
      ? "macOS may request Microphone and Input Monitoring access. Transcripts are copied for you to paste manually."
      : "Transcripts are copied for manual paste because reliable foreground-app validation is not available on this platform.";
  const keyRecorderDetail = keyRecorderState === "invalid"
    ? "Use Right Alt, Right Control, F8, or F9"
    : keyRecorderState === "listening"
      ? "Press a key now; Escape cancels"
      : "Press and hold this key while speaking";

  return (
    <main className="onboarding" ref={onboardingRef}>
      <div className="onboarding-content">
        <div className="onboarding-stage" role="progressbar" aria-label="Setup progress" aria-valuemin={1} aria-valuemax={4} aria-valuenow={currentStage} aria-valuetext={`Step ${currentStage} of 4`}>
          {[1, 2, 3, 4].map((number) => <i className={number <= currentStage ? "active" : ""} aria-hidden="true" key={number} />)}
        </div>

        {stage === "install" && (
          <>
            <div className="brand-mark"><Sparkles size={22} /></div>
            <p className="eyebrow">WELCOME TO</p>
            <h1 ref={headingRef} tabIndex={-1}>Zui. <em>Voice</em></h1>
            <p className="lede">Private, local dictation with a model and language you choose.</p>
            <div className="onboarding-choices">
              <label>
                <span>Speech model</span>
                <select aria-label="Speech model" value={selection.backendId} disabled={working || selectionWorking} onChange={(event) => chooseBackend(event.target.value)}>
                  {snapshot.backends.map((backend) => <option value={backend.id} key={backend.id}>{backend.name} · {backend.language}</option>)}
                </select>
              </label>
              <label>
                <span>Transcription language</span>
                <select aria-label="Transcription language" value={selection.locale} disabled={working || selectionWorking} onChange={(event) => void saveSelection({ ...selection, locale: event.target.value })}>
                  {selectedBackend?.locales.map((locale) => <option value={locale.locale} key={locale.locale}>{locale.name}</option>)}
                </select>
              </label>
            </div>
            <div className="setup-card">
              <div className={setupState.serverFound ? "step complete" : "step"}>
                <span>{setupState.serverFound ? <Check /> : "1"}</span>
                <div><strong>Speech engine</strong><small>Local Parakeet runtime</small></div>
              </div>
              <div className={setupState.modelFound ? "step complete" : "step"}>
                <span>{setupState.modelFound ? <Check /> : "2"}</span>
                <div><strong>{selectedBackend?.name ?? "Speech model"}</strong><small>Downloaded once for private, offline use</small></div>
              </div>
              <div className={setupState.complete ? "step complete" : "step"}>
                <span>{setupState.complete ? <Check /> : "3"}</span>
                <div><strong>Verify locally</strong><small>{setupState.complete ? "Required files are installed" : "SHA-256 checked before installation"}</small></div>
              </div>
            </div>

            {progress && (
              <div
                className={`progress phase-${progress.phase}`}
                role="progressbar"
                aria-label={`${phaseCopy[progress.phase]} ${assetLabel(progress.asset)}`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={progress.percent ?? undefined}
              >
                <i style={{ width: `${progress.percent ?? (progress.phase === "fetchingManifest" ? 12 : 100)}%` }} />
                <span>
                  <strong>{phaseCopy[progress.phase]} {assetLabel(progress.asset)}</strong>
                  {progress.total !== null && progress.phase === "downloading" && (
                    <small>{formatBytes(progress.received)} of {formatBytes(progress.total)}</small>
                  )}
                </span>
              </div>
            )}
            {error && <p className="inline-error" role="alert">{error}</p>}
            <div className="onboarding-actions">
              <button
                type="button"
                className="primary"
                disabled={selectionWorking}
                onClick={working ? cancel : setupState.complete ? () => goToStage("permissions") : download}
              >
                {working
                  ? <><X size={17} /> Cancel download</>
                  : setupState.complete
                    ? <>Continue <ChevronRight size={17} /></>
                    : <><Download size={17} /> Download securely</>}
              </button>
            </div>
            <p className="privacy-note">Only the versioned, checksummed engine files are downloaded. Audio and transcripts stay on this computer.</p>
          </>
        )}

        {stage === "permissions" && (
          <>
            <div className="stage-icon"><Mic2 /></div>
            <p className="eyebrow">STEP 2 OF 4</p>
            <h1 ref={headingRef} tabIndex={-1}>Set up dictation</h1>
            <p className="lede">Choose the microphone and hold key you want to use for dictation.</p>
            <div className="setup-card permission-card">
              <div className="step">
                <span><Mic2 /></span>
                <div className="permission-copy">
                  <strong>Microphone</strong>
                  <small>Audio input for dictation</small>
                  <div className="permission-control-row">
                    <select
                      aria-label="Microphone"
                      value={selectedDevice ?? ""}
                      disabled={working || selectionWorking}
                      onChange={(event) => setSelectedDevice(event.target.value || null)}
                    >
                      <option value="">System default</option>
                      {devices.map((device) => <option value={device} key={device}>{device}</option>)}
                    </select>
                  </div>
                </div>
              </div>
              <div className="step">
                <span><Keyboard /></span>
                <div className="permission-copy">
                  <strong>Hold key</strong>
                  <small>{keyRecorderDetail}</small>
                  <div className="permission-control-row">
                    <KeyRecorder
                      value={selection.hotkey.key}
                      platform={platform}
                      disabled={selectionWorking || working}
                      onChange={(key) => void saveSelection({ ...selection, hotkey: { ...selection.hotkey, key } })}
                      onStateChange={setKeyRecorderState}
                    />
                  </div>
                </div>
              </div>
            </div>
            <div className="delivery-note">
              <ClipboardCheck />
              <div><strong>{platform === "windows" ? "Automatic insertion" : "Safe clipboard delivery"}</strong><small>{platformDetail}</small></div>
            </div>
            {error && <p className="inline-error" role="alert">{error}</p>}
            <div className="onboarding-actions">
              <button type="button" className="secondary-button" onClick={() => goToStage("install")} disabled={working || selectionWorking}>
                <ChevronLeft /> Back
              </button>
              <button type="button" className="primary" onClick={verifySetup} disabled={working || selectionWorking || keyRecorderState !== "idle"}>
                {working ? <><LoaderCircle className="spin" /> Checking microphone…</> : <>Continue <ChevronRight size={17} /></>}
              </button>
            </div>
          </>
        )}

        {stage === "verifying" && (
          <>
            <div className="stage-icon"><Cpu /></div>
            <p className="eyebrow">STEP 3 OF 4</p>
            <h1 ref={headingRef} tabIndex={-1}>Verify the local engine</h1>
            <p className="lede">Zui is starting the local engine and loading {selectedBackend?.name ?? "the selected model"}. The first load can take a moment.</p>
            <div className="engine-check" role={error ? "alert" : "status"} aria-live="polite">
              {working ? <LoaderCircle className="spin" /> : error ? <X /> : <Check />}
              <div>
                <strong>{working ? "Loading the model…" : error ? "Couldn’t start the engine" : "Engine ready"}</strong>
                <small>{working ? "Everything remains on this computer" : error ?? "Local health check passed"}</small>
              </div>
            </div>
            <div className="onboarding-actions">
              {!working && (
                <button type="button" className="secondary-button" onClick={() => goToStage("permissions")}>
                  <ChevronLeft /> Back
                </button>
              )}
              {!working && error && (
                <button type="button" className="primary" onClick={verifyEngine}>Try again <ChevronRight /></button>
              )}
            </div>
          </>
        )}

        {stage === "tutorial" && (
          <>
            <div className="stage-icon success"><Check /></div>
            <p className="eyebrow">READY TO DICTATE</p>
            <h1 ref={headingRef} tabIndex={-1}>Speak. Release. Done.</h1>
            <p className="lede">The model and microphone are ready. Here’s all you need to remember.</p>
            <ol className="tutorial-steps">
              <li><span>1</span><div><strong>Hold <kbd>{configuredHotkeyLabel}</kbd></strong><small>Keep holding while you speak {selectedBackend?.locales.find((locale) => locale.locale === selection.locale)?.name ?? "your selected language"}.</small></div></li>
              <li><span>2</span><div><strong>Release when finished</strong><small>Zui transcribes locally and clears the temporary recording.</small></div></li>
              <li><span>3</span><div><strong>{platform === "windows" ? "Continue typing" : "Paste the copied text"}</strong><small>{platform === "windows" ? "Text is inserted if the original app is still focused." : "Use your usual paste shortcut in the destination app."}</small></div></li>
            </ol>
            {error && <p className="inline-error" role="alert">{error}</p>}
            <div className="onboarding-actions">
              <button type="button" className="secondary-button" onClick={() => goToStage("permissions")} disabled={working}>
                <ChevronLeft /> Back
              </button>
              <button type="button" className="primary" onClick={finish} disabled={working}>
                {working ? <><LoaderCircle className="spin" /> Finishing…</> : <>Start using Zui <ChevronRight /></>}
              </button>
            </div>
            <p className="privacy-note">Model terms, third-party notices, and privacy details are available under Legal in Settings.</p>
          </>
        )}
      </div>
    </main>
  );
}
