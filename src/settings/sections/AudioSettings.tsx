import type { AppSettings } from "../../types";
import { MicrophoneTestButton } from "../MicrophoneTestButton";
import { SettingRow } from "../SettingsControls";

export function AudioSettings({ settings, devices, onInputDeviceChange, onRecordingLimitChange }: {
  settings: AppSettings;
  devices: string[];
  onInputDeviceChange: (value: string | null) => void;
  onRecordingLimitChange: (value: number) => void;
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

      <section className="settings-section">
        <h2>Recording</h2>
        <div className="settings-card">
          <div className="range-setting">
            <div><strong>Recording limit</strong><small>Automatically stop a long recording</small></div>
            <div className="range-control">
              <input aria-label="Recording limit" type="range" min="30" max="300" step="30" value={settings.maxRecordingSeconds} onChange={(event) => onRecordingLimitChange(Number(event.target.value))} />
              <output>{Math.round(settings.maxRecordingSeconds / 60)} min</output>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}
