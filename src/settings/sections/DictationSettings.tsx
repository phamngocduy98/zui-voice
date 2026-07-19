import { useState } from "react";
import type { AppSettings, HoldKey } from "../../types";
import type { UiPlatform } from "../../ui";
import { KeyRecorder, type KeyRecorderState } from "../../components/KeyRecorder";
import { SettingRow, Toggle } from "../SettingsControls";

export function DictationSettings({ settings, hotkeyLabel, platform, onEnabledChange, onHotkeyChange, onConsumeChange, onRecordingLimitChange, onClipboardRestoreChange, onLaunchAtLoginChange }: {
  settings: AppSettings;
  hotkeyLabel: string;
  platform: UiPlatform;
  onEnabledChange: (value: boolean) => void;
  onHotkeyChange: (value: HoldKey) => void;
  onConsumeChange: (value: boolean) => void;
  onRecordingLimitChange: (value: number) => void;
  onClipboardRestoreChange: (value: boolean) => void;
  onLaunchAtLoginChange: (value: boolean) => void;
}) {
  const [keyRecorderState, setKeyRecorderState] = useState<KeyRecorderState>("idle");

  return (
    <>
      <section className="settings-section">
        <h2>Push to Talk</h2>
        <div className="settings-card">
          <SettingRow title="Dictation" detail="Listen for the global hold key">
            <Toggle checked={settings.enabled} onChange={onEnabledChange} label="Dictation enabled" />
          </SettingRow>
          <SettingRow
            title="Hold key"
            detail={keyRecorderState !== "idle"
              ? keyRecorderState === "invalid" ? "Use Right Alt, Right Control, F8, or F9" : "Press a key now; Escape cancels"
              : "Press and hold to record"}
          >
            <KeyRecorder
              value={settings.hotkey.key}
              platform={platform}
              onChange={onHotkeyChange}
              onStateChange={setKeyRecorderState}
            />
          </SettingRow>
          <SettingRow title="Consume hold key" detail={`Keep ${hotkeyLabel} from reaching the focused app`}>
            <Toggle checked={settings.hotkey.consume} onChange={onConsumeChange} label="Consume hold key" />
          </SettingRow>
        </div>
      </section>

      <section className="settings-section">
        <h2>Recording</h2>
        <div className="settings-card">
          <div className="range-setting">
            <div><strong>Recording limit</strong><small>Automatically stop a long dictation</small></div>
            <div className="range-control">
              <input aria-label="Recording limit" type="range" min="30" max="300" step="30" value={settings.maxRecordingSeconds} onChange={(event) => onRecordingLimitChange(Number(event.target.value))} />
              <output>{Math.round(settings.maxRecordingSeconds / 60)} min</output>
            </div>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <h2>Behavior</h2>
        <div className="settings-card">
          <SettingRow title="Restore clipboard" detail="Preserve common formats after insertion">
            <Toggle checked={settings.clipboardRestore} onChange={onClipboardRestoreChange} label="Restore clipboard" />
          </SettingRow>
          <SettingRow title="Open at Login" detail="Keep dictation one key away">
            <Toggle checked={settings.launchAtLogin} onChange={onLaunchAtLoginChange} label="Open at Login" />
          </SettingRow>
        </div>
      </section>
    </>
  );
}
