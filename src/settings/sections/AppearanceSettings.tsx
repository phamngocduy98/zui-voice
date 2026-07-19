import type { AppSettings, ThemePreference } from "../../types";
import { SettingRow } from "../SettingsControls";

const themes: ThemePreference[] = ["light", "system", "dark"];

export function AppearanceSettings({ settings, onThemeChange }: { settings: AppSettings; onThemeChange: (theme: ThemePreference) => void }) {
  return (
    <section className="settings-section">
      <h2>Appearance</h2>
      <div className="settings-card">
        <SettingRow title="Theme" detail="System follows your device appearance">
          <div className="theme-picker" role="radiogroup" aria-label="Theme">
            {themes.map((theme) => (
              <button
                type="button"
                role="radio"
                aria-checked={settings.theme === theme}
                className={settings.theme === theme ? "selected" : ""}
                key={theme}
                onClick={() => onThemeChange(theme)}
              >
                {theme[0].toUpperCase() + theme.slice(1)}
              </button>
            ))}
          </div>
        </SettingRow>
      </div>
    </section>
  );
}
