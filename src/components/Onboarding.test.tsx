// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { DownloadProgress, SetupStatus } from "../types";
import { Onboarding } from "./Onboarding";

const completeSetup: SetupStatus = {
  backendId: "nemotron-3.5-asr-streaming-0.6b",
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
  it("configures inputs and verifies the microphone and engine before completion", async () => {
    const snapshot = await api.getSnapshot();
    vi.spyOn(api, "listInputDevices").mockResolvedValue(["Studio Microphone"]);
    const update = vi.spyOn(api, "updateSettings");
    const microphone = vi.spyOn(api, "testMicrophone").mockResolvedValue();
    const engine = vi.spyOn(api, "retryBackend").mockResolvedValue(snapshot);
    const complete = vi.spyOn(api, "completeOnboarding").mockResolvedValue(snapshot);
    const onReady = vi.fn();

    render(
      <Onboarding
        setup={completeSetup}
        snapshot={snapshot}
        platform="windows"
        inputDeviceName={null}
        onChange={vi.fn()}
        onReady={onReady}
      />
    );

    expect(screen.getByRole("heading", { name: "Zui. Voice" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("heading", { name: "Set up dictation" })).toBeTruthy();
    fireEvent.change(await screen.findByRole("combobox", { name: "Microphone" }), {
      target: { value: "Studio Microphone" }
    });

    const keyRecorder = screen.getByRole("button", { name: /Hold key: Right Alt/ });
    fireEvent.click(keyRecorder);
    fireEvent.keyDown(keyRecorder, { key: "F9", code: "F9" });
    await vi.waitFor(() => expect(update).toHaveBeenCalledWith(expect.objectContaining({
      hotkey: expect.objectContaining({ key: "F9" })
    })));

    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Continue" }).hasAttribute("disabled")).toBe(false));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Speak. Release. Done." })).toBeTruthy();
    expect(microphone).toHaveBeenCalledWith("Studio Microphone");
    expect(engine).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Start using Zui" }));
    await vi.waitFor(() => expect(onReady).toHaveBeenCalledOnce());
    expect(complete).toHaveBeenCalledWith("Studio Microphone");
  });

  it("lets users return to their model choice and moves focus with the stage", async () => {
    const snapshot = await api.getSnapshot();
    vi.spyOn(api, "listInputDevices").mockResolvedValue([]);

    render(
      <Onboarding
        setup={completeSetup}
        snapshot={snapshot}
        platform="windows"
        inputDeviceName={null}
        onChange={vi.fn()}
        onReady={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const permissionsHeading = screen.getByRole("heading", { name: "Set up dictation" });
    expect(document.activeElement).toBe(permissionsHeading);

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    const installHeading = screen.getByRole("heading", { name: "Zui. Voice" });
    expect(document.activeElement).toBe(installHeading);
    expect(screen.getByRole("progressbar", { name: "Setup progress" }).getAttribute("aria-valuenow")).toBe("1");
  });

  it("shows semantic progress for download and verification phases", async () => {
    const snapshot = await api.getSnapshot();
    let publish: ((progress: DownloadProgress) => void) | undefined;
    vi.spyOn(api, "onDownload").mockImplementation(async (handler) => {
      publish = handler;
      return () => undefined;
    });
    vi.spyOn(api, "startAssetDownload").mockReturnValue(new Promise(() => undefined));

    render(
      <Onboarding
        setup={{ ...completeSetup, complete: false, modelFound: false }}
        snapshot={snapshot}
        platform="windows"
        inputDeviceName={null}
        onChange={vi.fn()}
        onReady={vi.fn()}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Download securely" }));
    await act(async () => {
      publish?.({
        phase: "verifying",
        asset: "nemotron-model",
        received: 917_504_000,
        total: 917_504_000,
        percent: 100
      });
    });

    const progress = screen.getByRole("progressbar", { name: "Verifying checksum Speech model" });
    expect(progress.getAttribute("aria-valuenow")).toBe("100");
    expect(screen.getByRole("button", { name: "Cancel download" })).toBeTruthy();
  });

  it("downloads from the release when setup is incomplete", async () => {
    const snapshot = await api.getSnapshot();
    const download = vi.spyOn(api, "startAssetDownload").mockResolvedValue(completeSetup);
    render(
      <Onboarding
        setup={{ ...completeSetup, complete: false, modelFound: false, manifestConfigured: false }}
        snapshot={snapshot}
        platform="linux"
        inputDeviceName={null}
        onChange={vi.fn()}
        onReady={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "Download securely" }));
    await vi.waitFor(() => expect(download).toHaveBeenCalledOnce());
  });
});
