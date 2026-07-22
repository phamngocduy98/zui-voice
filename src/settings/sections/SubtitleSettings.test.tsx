// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SubtitleSettings } from "./SubtitleSettings";
import type { AppSnapshot } from "../../types";

const snapshot = (overrides: Partial<AppSnapshot> = {}): AppSnapshot => ({
  settings: {
    hotkey: { key: "RightAlt", consume: true }, inputDeviceName: null, backendId: "nemotron-3.5-asr-streaming-0.6b", locale: "en-US", launchAtLogin: false,
    clipboardRestore: true, maxRecordingSeconds: 30, modelIdleTimeoutSeconds: 600, enabled: true, theme: "system", onboardingVersion: 1,
    subtitles: { overlayLocked: false, position: null, maxLines: 3 }
  },
  state: { phase: "idle", backendStatus: "ready" }, backend: {} as AppSnapshot["backend"], backends: [], setupComplete: true, onboardingComplete: true,
  platform: "windows", wayland: false, subtitleState: { phase: "disabled" },
  systemAudioCapabilities: { available: false, permission: "unavailable", implementation: "WASAPI loopback", detail: "No output device" }, ...overrides
});

const renderSettings = (value = snapshot(), busy = false, error: string | null = null) => render(<SubtitleSettings snapshot={value} busy={busy} error={error} onStart={vi.fn()} onStop={vi.fn()} onLock={vi.fn()} onReset={vi.fn()} onOpenSystemSettings={vi.fn()} onLines={vi.fn()} />);

describe("SubtitleSettings", () => {
  it("shows unavailable details once in the bounded status callout and disables Start", () => {
    renderSettings();
    expect((screen.getByRole("button", { name: "Start" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getAllByText("No output device")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: /settings|learn more/i })).toBeNull();
  });

  it("uses explicit busy action labels without shrinking the controls", () => {
    renderSettings(snapshot({ systemAudioCapabilities: { available: true, permission: "notRequired", implementation: "WASAPI loopback", detail: "Ready" } }), true);
    const button = screen.getByRole("button", { name: "Starting…" });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(button.className).toContain("subtitle-action");
  });

  it("renders one state error and an actionable settings link only for permission remediation", () => {
    renderSettings(snapshot({
      subtitleState: { phase: "error", error: { code: "denied", message: "Allow system capture", recoverable: true } },
      systemAudioCapabilities: { available: false, permission: "denied", implementation: "WASAPI loopback", detail: "Allow system capture" }
    }), false, "Allow system capture");
    expect(screen.getAllByText("Allow system capture")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Open settings" })).toBeTruthy();
  });
});
