// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import * as api from "./api";

const windowApi = vi.hoisted(() => ({
  startDragging: vi.fn(),
  toggleMaximize: vi.fn(),
  setTheme: vi.fn().mockResolvedValue(undefined)
}));

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => windowApi }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  Reflect.deleteProperty(window, "matchMedia");
  Reflect.deleteProperty(document.documentElement.dataset, "theme");
  document.documentElement.style.removeProperty("color-scheme");
  window.history.replaceState({}, "", "/");
});

describe("Zui. Voice settings shell", () => {
  it("renders the settings navigation without the old hero", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Dictation" })).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Settings sections" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Close window" })).toBeNull();
    expect(screen.getByRole("searchbox", { name: "Search settings" })).toBeTruthy();
    expect(screen.getAllByText("Right Alt").length).toBeGreaterThan(0);
    expect(screen.queryByText("Speak. Release.")).toBeNull();
    expect(screen.queryByText("Ready")).toBeNull();
    expect(screen.queryByText("Saved")).toBeNull();
    expect(screen.queryByText(/this Mac/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Audio/ }));
    expect(screen.getByRole("heading", { name: "Audio" })).toBeTruthy();
  });

  it("does not bypass onboarding when assets are already installed", async () => {
    const snapshot = await api.getSnapshot();
    vi.spyOn(api, "getSnapshot").mockResolvedValue({
      ...snapshot,
      onboardingComplete: false,
      settings: { ...snapshot.settings, onboardingVersion: 0 }
    });

    render(<App />);

    expect(await screen.findByRole("heading", { name: "Zui. Voice" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Dictation" })).toBeNull();
  });

  it.each([
    ["macos-aarch64", "macos", "Right Option", true],
    ["windows-x86_64", "windows", "Right Alt", false],
    ["linux-x86_64", "linux", "Right Alt", false],
    ["freebsd", "other", "Right Alt", false]
  ])("renders the compact %s platform layout", async (reportedPlatform, uiPlatform, hotkeyLabel, hasOverlayHeader) => {
    const snapshot = await api.getSnapshot();
    vi.spyOn(api, "getSnapshot").mockResolvedValue({ ...snapshot, platform: reportedPlatform });

    render(<App />);

    expect(await screen.findByRole("heading", { name: "Dictation", level: 1 })).toBeTruthy();
    const sectionHeader = screen.queryByTestId("section-header");
    if (hasOverlayHeader) {
      expect(sectionHeader?.dataset.platform).toBe(uiPlatform);
      expect(within(sectionHeader!).queryAllByRole("button")).toHaveLength(0);
    } else {
      expect(sectionHeader).toBeNull();
    }
    expect(screen.getAllByText(hotkeyLabel).length).toBeGreaterThan(0);
  });

  it("keeps the macOS overlay header draggable below native chrome", async () => {
    const snapshot = await api.getSnapshot();
    vi.spyOn(api, "getSnapshot").mockResolvedValue({ ...snapshot, platform: "macos-aarch64" });

    render(<App />);
    const sectionHeader = await screen.findByTestId("section-header");
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });

    fireEvent.mouseDown(sectionHeader, { button: 0, buttons: 1, detail: 1 });
    expect(windowApi.startDragging).toHaveBeenCalledOnce();

    fireEvent.mouseDown(sectionHeader, { button: 0, buttons: 1, detail: 2 });
    expect(windowApi.toggleMaximize).toHaveBeenCalledOnce();
  });

  it("uses the platform hotkey label in the recording overlay", async () => {
    const snapshot = await api.getSnapshot();
    vi.spyOn(api, "getSnapshot").mockResolvedValue({ ...snapshot, platform: "macos-aarch64" });
    window.history.replaceState({}, "", "/?view=overlay");

    render(<App />);

    expect(await screen.findByText("Hold Right Option")).toBeTruthy();
  });

  it("supports light, dark, and live system appearance", async () => {
    let systemIsDark = false;
    let themeListener: (() => void) | undefined;
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({
        get matches() { return systemIsDark; },
        addEventListener: (_event: string, listener: () => void) => { themeListener = listener; },
        removeEventListener: vi.fn()
      }))
    });

    render(<App />);
    expect(await screen.findByRole("heading", { name: "Dictation" })).toBeTruthy();
    expect(document.documentElement.dataset.theme).toBe("light");
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });

    fireEvent.click(screen.getByRole("button", { name: /Appearance/ }));
    fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(windowApi.setTheme).toHaveBeenLastCalledWith("dark");

    fireEvent.click(screen.getByRole("radio", { name: "System" }));
    expect(document.documentElement.dataset.theme).toBe("light");
    systemIsDark = true;
    themeListener?.();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(windowApi.setTheme).toHaveBeenLastCalledWith("dark");
  });

  it("updates the model status after unloading", async () => {
    const snapshot = await api.getSnapshot();
    const readySnapshot = {
      ...snapshot,
      state: { phase: "idle" as const, backendStatus: "ready" as const }
    };
    const stoppedSnapshot = {
      ...snapshot,
      state: { phase: "idle" as const, backendStatus: "stopped" as const }
    };
    vi.spyOn(api, "getSnapshot").mockResolvedValue(readySnapshot);
    const unload = vi.spyOn(api, "unloadModel").mockResolvedValue(stoppedSnapshot);

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /Local Engine/ }));
    expect(screen.getByText("Ready")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Unload" }));

    expect(await screen.findByText("Stopped")).toBeTruthy();
    expect(unload).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Unload" }).hasAttribute("disabled")).toBe(true);
  });

  it("shows a recoverable startup error instead of hanging on the splash screen", async () => {
    vi.spyOn(api, "getSnapshot").mockRejectedValueOnce({ message: "Backend state is unavailable" });

    render(<App />);

    expect((await screen.findByRole("alert")).textContent).toContain("Backend state is unavailable");
    fireEvent.click(screen.getByRole("button", { name: "Try Again" }));
    expect(await screen.findByRole("heading", { name: "Dictation" })).toBeTruthy();
  });
});
