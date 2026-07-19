import { useState } from "react";
import type { HoldKey } from "../types";
import { formatHotkeyLabel, holdKeyFromCode, type UiPlatform } from "../ui";

export type KeyRecorderState = "idle" | "listening" | "invalid";

export function KeyRecorder({ value, platform, disabled = false, onChange, onStateChange }: {
  value: HoldKey;
  platform: UiPlatform;
  disabled?: boolean;
  onChange: (value: HoldKey) => void;
  onStateChange?: (state: KeyRecorderState) => void;
}) {
  const [state, setState] = useState<KeyRecorderState>("idle");
  const updateState = (next: KeyRecorderState) => {
    setState(next);
    onStateChange?.(next);
  };
  const listening = state !== "idle";

  return (
    <button
      type="button"
      className={listening ? "key-recorder listening" : "key-recorder"}
      aria-label={`Hold key: ${formatHotkeyLabel(value, platform)}. Click to record a new key.`}
      aria-pressed={listening}
      disabled={disabled}
      onBlur={() => updateState("idle")}
      onClick={() => updateState(listening ? "idle" : "listening")}
      onKeyDown={(event) => {
        if (!listening) return;
        event.preventDefault();
        event.stopPropagation();
        if (event.code === "Escape") {
          updateState("idle");
          return;
        }
        const key = holdKeyFromCode(event.code);
        if (!key) {
          updateState("invalid");
          return;
        }
        onChange(key);
        updateState("idle");
      }}
    >
      {listening ? "Press a key..." : formatHotkeyLabel(value, platform)}
    </button>
  );
}
