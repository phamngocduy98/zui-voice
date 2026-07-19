// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { DownloadProgress, SetupStatus } from "../types";
import { Onboarding } from "./Onboarding";

const completeSetup: SetupStatus = {
  complete: true,
  serverFound: true,
  modelFound: true,
  serverPath: null,
  modelPath: null,
  manifestConfigured: true
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Onboarding", () => {
  it("requires microphone and engine verification before completion", async () => {
    const snapshot = await api.getSnapshot();
    let confirmShortcut: (() => void) | undefined;
    vi.spyOn(api, "listInputDevices").mockResolvedValue(["Studio Microphone"]);
    vi.spyOn(api, "onHotkeyTest").mockImplementation(async (handler) => {
      confirmShortcut = handler;
      return () => undefined;
    });
    vi.spyOn(api, "getHotkeyTestStatus").mockResolvedValue(false);
    const shortcut = vi.spyOn(api, "beginHotkeyTest").mockResolvedValue();
    const microphone = vi.spyOn(api, "testMicrophone").mockResolvedValue();
    const engine = vi.spyOn(api, "retryBackend").mockResolvedValue(snapshot);
    const complete = vi.spyOn(api, "completeOnboarding").mockResolvedValue(snapshot);
    const onReady = vi.fn();

    render(
      <Onboarding
        setup={completeSetup}
        platform="windows"
        hotkeyLabel="Right Alt"
        inputDeviceName={null}
        onReady={onReady}
      />
    );

    expect(screen.getByRole("heading", { name: "Zui. Voice" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Check your microphone" })).toBeTruthy();
    fireEvent.change(await screen.findByRole("combobox", { name: "Microphone" }), {
      target: { value: "Studio Microphone" }
    });
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    expect((await screen.findByRole("status")).textContent).toContain("Microphone test passed");
    expect(microphone).toHaveBeenCalledWith("Studio Microphone");

    fireEvent.click(screen.getByRole("button", { name: "Test shortcut" }));
    await act(async () => confirmShortcut?.());
    expect(shortcut).toHaveBeenCalledOnce();
    expect(screen.getByText("Right Alt is available system-wide")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Speak. Release. Done." })).toBeTruthy();
    expect(engine).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Start using Zui" }));
    await vi.waitFor(() => expect(onReady).toHaveBeenCalledOnce());
    expect(complete).toHaveBeenCalledWith("Studio Microphone");
  });

  it("shows semantic progress for download and verification phases", async () => {
    let publish: ((progress: DownloadProgress) => void) | undefined;
    vi.spyOn(api, "onDownload").mockImplementation(async (handler) => {
      publish = handler;
      return () => undefined;
    });
    vi.spyOn(api, "startAssetDownload").mockReturnValue(new Promise(() => undefined));

    render(
      <Onboarding
        setup={{ ...completeSetup, complete: false, modelFound: false }}
        platform="windows"
        hotkeyLabel="Right Alt"
        inputDeviceName={null}
        onReady={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Download securely" }));
    await act(async () => {
      publish?.({
        phase: "verifying",
        asset: "vietnamese-model",
        received: 917_504_000,
        total: 917_504_000,
        percent: 100
      });
    });

    const progress = screen.getByRole("progressbar", { name: "Verifying checksum Vietnamese model" });
    expect(progress.getAttribute("aria-valuenow")).toBe("100");
    expect(screen.getByRole("button", { name: "Cancel download" })).toBeTruthy();
  });

  it("downloads from the release when setup is incomplete", async () => {
    const download = vi.spyOn(api, "startAssetDownload").mockResolvedValue(completeSetup);
    render(
      <Onboarding
        setup={{ ...completeSetup, complete: false, modelFound: false, manifestConfigured: false }}
        platform="linux"
        hotkeyLabel="Right Alt"
        inputDeviceName={null}
        onReady={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "Download securely" }));
    await vi.waitFor(() => expect(download).toHaveBeenCalledOnce());
  });
});
