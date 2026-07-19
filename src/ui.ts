export type UiPlatform = "macos" | "windows" | "linux" | "other";

export const EMPTY_SPECTRUM_BINS = Array.from(
  { length: 24 },
  (_, index) => 0.16 + Math.sin(index * 1.7) * 0.03
);

export function errorMessage(error: unknown) {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return "Something went wrong. Please try again.";
}

export function normalizeUiPlatform(platform: string): UiPlatform {
  const os = platform.toLowerCase().split("-")[0];
  if (os === "macos" || os === "windows" || os === "linux") return os;
  return "other";
}

export function formatHotkeyLabel(key: string, platform: UiPlatform) {
  if (key === "RightAlt") return platform === "macos" ? "Right Option" : "Right Alt";
  return key;
}
