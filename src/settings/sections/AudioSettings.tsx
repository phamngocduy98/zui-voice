import type { AppSettings } from "../../types";
import { MicrophoneTestButton } from "../MicrophoneTestButton";
import { SettingRow } from "../SettingsControls";

export function AudioSettings({ settings, devices, onInputDeviceChange }: {
  settings: AppSettings;
  devices: string[];
  onInputDeviceChange: (value: string | null) => void;
}) {
  return (
    <>
      <section className="settings-section">
        <h2>Input</h2>
        <div className="settings-card">
          <SettingRow title="Microphone" detail="Use the system default when unchanged">
            <select aria-label="Microphone" value={settings.inputDeviceName ?? ""} onChange={(event) => onInputDeviceChange(event.target.value || null)}>
              <option value="">System Default</option>
              {devices.map((device) => <option key={device}>{device}</option>)}
            </select>
          </SettingRow>
          <SettingRow title="Test microphone" detail="Hold the button and speak normally">
            <MicrophoneTestButton />
          </SettingRow>
        </div>
      </section>
    </>
  );
}
