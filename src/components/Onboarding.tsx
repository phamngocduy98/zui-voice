import { useEffect, useState } from "react";
import {
  Check,
  ChevronLeft,
  ChevronRight,
  Cpu,
  Download,
  Keyboard,
  LoaderCircle,
  Mic2,
  ShieldCheck,
  Sparkles,
  X
} from "lucide-react";
import {
  beginHotkeyTest,
  cancelAssetDownload,
  cancelHotkeyTest,
  completeOnboarding,
  getHotkeyTestStatus,
  listInputDevices,
  onDownload,
  onHotkeyTest,
  retryBackend,
  startAssetDownload,
  testMicrophone
} from "../api";
import type { DownloadProgress, SetupStatus } from "../types";
import type { UiPlatform } from "../ui";
import { errorMessage } from "../ui";

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
  if (value.includes("model")) return "Vietnamese model";
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

export function Onboarding({ setup, platform, hotkeyLabel, inputDeviceName, onReady }: {
  setup: SetupStatus;
  platform: UiPlatform;
  hotkeyLabel: string;
  inputDeviceName: string | null;
  onReady: () => void;
}) {
  const [stage, setStage] = useState<Stage>("install");
  const [setupState, setSetupState] = useState(setup);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [selectedDevice, setSelectedDevice] = useState(inputDeviceName);
  const [microphoneReady, setMicrophoneReady] = useState(false);
  const [shortcutReady, setShortcutReady] = useState(false);
  const [shortcutWaiting, setShortcutWaiting] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
    if (!shortcutWaiting) return;
    let disposed = false;
    const check = async () => {
      try {
        if (await getHotkeyTestStatus() && !disposed) {
          setShortcutReady(true);
          setShortcutWaiting(false);
          setError(null);
        }
      } catch (caught) {
        if (!disposed) {
          setShortcutWaiting(false);
          setError(errorMessage(caught));
        }
      }
    };
    void check();
    const timer = window.setInterval(check, 200);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [shortcutWaiting]);

  useEffect(() => {
    let disposed = false;
    let unlisten: () => void = () => undefined;
    onHotkeyTest(() => {
      setShortcutReady(true);
      setShortcutWaiting(false);
      setError(null);
    }).then((fn) => {
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
      if (result.complete) setStage("permissions");
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

  const checkMicrophone = async () => {
    setWorking(true);
    setMicrophoneReady(false);
    setError(null);
    try {
      await testMicrophone(selectedDevice);
      setMicrophoneReady(true);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setWorking(false);
    }
  };

  const checkShortcut = async () => {
    if (shortcutWaiting) {
      try {
        await cancelHotkeyTest();
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setShortcutWaiting(false);
      }
      return;
    }
    setShortcutReady(false);
    setError(null);
    try {
      await beginHotkeyTest();
      setShortcutWaiting(true);
    } catch (caught) {
      setError(errorMessage(caught));
    }
  };

  const verifyEngine = async () => {
    setStage("verifying");
    setWorking(true);
    setError(null);
    try {
      await retryBackend();
      setStage("tutorial");
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

  return (
    <main className="onboarding">
      <div className="onboarding-content">
        <div className="onboarding-stage" aria-label={`Setup step ${currentStage} of 4`}>
          {[1, 2, 3, 4].map((number) => <i className={number <= currentStage ? "active" : ""} key={number} />)}
        </div>

        {stage === "install" && (
          <>
            <div className="brand-mark"><Sparkles size={22} /></div>
            <p className="eyebrow">WELCOME TO</p>
            <h1>Zui. <em>Voice</em></h1>
            <p className="lede">Private Vietnamese dictation that stays on your computer.</p>
            <div className="setup-card">
              <div className={setupState.serverFound ? "step complete" : "step"}>
                <span>{setupState.serverFound ? <Check /> : "1"}</span>
                <div><strong>Speech engine</strong><small>Local Parakeet runtime</small></div>
              </div>
              <div className={setupState.modelFound ? "step complete" : "step"}>
                <span>{setupState.modelFound ? <Check /> : "2"}</span>
                <div><strong>Vietnamese model</strong><small>About 875 MB, downloaded once</small></div>
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
                onClick={working ? cancel : setupState.complete ? () => setStage("permissions") : download}
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
            <h1>Check your microphone</h1>
            <p className="lede">Zui needs a working input stream. The test is discarded immediately and is never transcribed.</p>
            <div className="setup-card permission-card">
              <div className={microphoneReady ? "step complete" : "step"}>
                <span>{microphoneReady ? <Check /> : <Mic2 />}</span>
                <div className="permission-copy">
                  <strong>Microphone access</strong>
                  <small>{microphoneReady ? "Audio input is available" : "Your system may ask for permission"}</small>
                  <select
                    aria-label="Microphone"
                    value={selectedDevice ?? ""}
                    onChange={(event) => {
                      setSelectedDevice(event.target.value || null);
                      setMicrophoneReady(false);
                    }}
                  >
                    <option value="">System default</option>
                    {devices.map((device) => <option value={device} key={device}>{device}</option>)}
                  </select>
                </div>
              </div>
              <div className={shortcutReady ? "step complete" : "step"}>
                <span>{shortcutReady ? <Check /> : <Keyboard />}</span>
                <div className="permission-copy">
                  <strong>Global hold key</strong>
                  <small>{shortcutReady ? `${hotkeyLabel} is available system-wide` : shortcutWaiting ? `Press ${hotkeyLabel} now` : "Optional check; system access can also be configured later"}</small>
                  <button type="button" className="inline-test-button" onClick={checkShortcut}>
                    {shortcutWaiting ? "Cancel test" : shortcutReady ? "Test again" : "Test shortcut"}
                  </button>
                </div>
              </div>
              <div className="step">
                <span><ShieldCheck /></span>
                <div><strong>{platform === "windows" ? "Automatic insertion" : "Safe clipboard delivery"}</strong><small>{platformDetail}</small></div>
              </div>
            </div>
            {error && <p className="inline-error" role="alert">{error}</p>}
            {microphoneReady && <p className="inline-success" role="status"><Check /> Microphone test passed</p>}
            <div className="onboarding-actions">
              <button type="button" className="secondary-button" onClick={checkMicrophone} disabled={working}>
                {working ? <><LoaderCircle className="spin" /> Testing…</> : <><Mic2 /> Test microphone</>}
              </button>
              <button type="button" className="primary" onClick={verifyEngine} disabled={!microphoneReady || working}>
                Continue <ChevronRight size={17} />
              </button>
            </div>
          </>
        )}

        {stage === "verifying" && (
          <>
            <div className="stage-icon"><Cpu /></div>
            <p className="eyebrow">STEP 3 OF 4</p>
            <h1>Verify the local engine</h1>
            <p className="lede">Zui is starting Parakeet and loading the Vietnamese model. The first load can take a moment.</p>
            <div className="engine-check" role="status" aria-live="polite">
              {working ? <LoaderCircle className="spin" /> : error ? <X /> : <Check />}
              <div>
                <strong>{working ? "Loading the model…" : error ? "Couldn’t start the engine" : "Engine ready"}</strong>
                <small>{working ? "Everything remains on this computer" : error ?? "Local health check passed"}</small>
              </div>
            </div>
            {error && <p className="inline-error" role="alert">{error}</p>}
            <div className="onboarding-actions">
              {!working && (
                <button type="button" className="secondary-button" onClick={() => setStage("permissions")}>
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
            <h1>Speak. Release. Done.</h1>
            <p className="lede">The model and microphone are ready. Here’s all you need to remember.</p>
            <ol className="tutorial-steps">
              <li><span>1</span><div><strong>Hold <kbd>{hotkeyLabel}</kbd></strong><small>Keep holding while you speak Vietnamese.</small></div></li>
              <li><span>2</span><div><strong>Release when finished</strong><small>Zui transcribes locally and clears the temporary recording.</small></div></li>
              <li><span>3</span><div><strong>{platform === "windows" ? "Continue typing" : "Paste the copied text"}</strong><small>{platform === "windows" ? "Text is inserted if the original app is still focused." : "Use your usual paste shortcut in the destination app."}</small></div></li>
            </ol>
            {error && <p className="inline-error" role="alert">{error}</p>}
            <div className="onboarding-actions">
              <button type="button" className="secondary-button" onClick={() => setStage("permissions")} disabled={working}>
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
