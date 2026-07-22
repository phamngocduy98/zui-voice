// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SubtitleOverlay } from "./SubtitleOverlay";

vi.mock("../hooks/useSubtitles", () => ({
  useSubtitles: () => ({
    state: { phase: "listening" },
    settings: null,
    text: { sessionId: 1, revision: 1, utteranceId: 1, stableText: "你好", unstableText: "世界", isFinal: false }
  })
}));

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ startDragging: vi.fn() }) }));

describe("SubtitleOverlay", () => {
  it("keeps model-provided stable and unstable text contiguous and exposes the line limit", () => {
    const { container } = render(<SubtitleOverlay settings={{ overlayLocked: false, position: null, maxLines: 2 }} />);

    expect(screen.getByText("你好").nextSibling?.textContent).toBe("世界");
    expect(container.querySelector(".subtitle-shell")?.getAttribute("style")).toContain("--subtitle-lines: 2");
  });
});
