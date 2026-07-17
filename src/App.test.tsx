// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("Zui. Voice settings shell", () => {
  it("renders the hold-to-talk instruction outside Tauri", async () => {
    render(<App />);
    expect((await screen.findAllByText(/Hold/)).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Right Alt").length).toBeGreaterThan(0);
  });
});
