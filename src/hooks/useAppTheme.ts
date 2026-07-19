import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ThemePreference } from "../types";

export function useAppTheme(themePreference: ThemePreference) {
  useEffect(() => {
    const media = typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null;
    const applyTheme = () => {
      const resolved = themePreference === "system" ? (media?.matches ? "dark" : "light") : themePreference;
      document.documentElement.dataset.theme = resolved;
      document.documentElement.style.colorScheme = resolved;
      if ("__TAURI_INTERNALS__" in window) {
        void getCurrentWindow().setTheme(resolved).catch(() => undefined);
      }
    };
    applyTheme();
    media?.addEventListener("change", applyTheme);
    return () => media?.removeEventListener("change", applyTheme);
  }, [themePreference]);
}
