import { CircleAlert } from "lucide-react";
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

export function Overlay({ state, bins }: { state: DictationState; bins: number[] }) {
  const isRecording = state.phase === "recording";
  const isError = state.phase === "error";
  const busy = ["loading", "transcribing", "pasting"].includes(state.phase);

  return (
    <main className={"overlay-shell phase-" + state.phase}>
      {isError ? (
        <div className="overlay-error" role="alert">
          <CircleAlert aria-hidden="true" />
          <span>{state.error.message}</span>
        </div>
      ) : (
        <Spectrum
          bins={isRecording ? bins : busy ? bins.map((_, index) => 0.2 + ((index * 7) % 8) / 15) : EMPTY_SPECTRUM_BINS}
          active={isRecording || busy}
        />
      )}
    </main>
  );
}
