import { Check, CircleAlert, ClipboardCheck, Mic2 } from "lucide-react";
import type { DictationState } from "../types";
import { EMPTY_SPECTRUM_BINS } from "../ui";

function Spectrum({ bins, active }: { bins: number[]; active: boolean }) {
  return (
    <div className={"spectrum " + (active ? "is-active" : "")} aria-hidden="true">
      {bins.map((value, index) => (
        <i key={index} style={{ height: Math.max(8, Math.min(100, value * 100)) + "%" }} />
      ))}
    </div>
  );
}

function stateCopy(state: DictationState, hotkeyLabel: string) {
  switch (state.phase) {
    case "recording": return ["Listening", "Speak naturally"];
    case "loading": return ["Warming up", state.detail];
    case "transcribing": return ["Transcribing", "Vietnamese · local"];
    case "pasting": return ["Inserting", "Restoring clipboard"];
    case "success": return ["Done", "Text inserted"];
    case "copied": return ["Copied", state.reason];
    case "error": return ["Couldn’t finish", state.error.message];
    case "setupRequired": return ["Setup needed", state.detail];
    default: return ["Ready", `Hold ${hotkeyLabel}`];
  }
}

export function Overlay({ state, bins, hotkeyLabel }: { state: DictationState; bins: number[]; hotkeyLabel: string }) {
  const [title, detail] = stateCopy(state, hotkeyLabel);
  const isRecording = state.phase === "recording";
  const isSuccess = state.phase === "success";
  const isCopied = state.phase === "copied";
  const isError = state.phase === "error";
  const busy = ["loading", "transcribing", "pasting"].includes(state.phase);

  return (
    <main className={"overlay-shell phase-" + state.phase}>
      <div className="overlay-glow" />
      <div className="status-orb">
        {isSuccess ? <Check size={18} /> : isCopied ? <ClipboardCheck size={17} /> : isError ? <CircleAlert size={17} /> : <Mic2 size={18} />}
        {isRecording && <span className="pulse-ring" />}
      </div>
      <div className="overlay-copy">
        <strong>{title}</strong>
        <span>{detail}</span>
      </div>
      <Spectrum
        bins={isRecording ? bins : busy ? bins.map((_, index) => 0.2 + ((index * 7) % 8) / 15) : EMPTY_SPECTRUM_BINS}
        active={isRecording || busy}
      />
    </main>
  );
}
