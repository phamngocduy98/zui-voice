import type { AppSettings } from "../../types";
import { SettingRow, Toggle } from "../SettingsControls";

export function DictationSettings({ settings, hotkeyLabel, onEnabledChange, onConsumeChange, onClipboardRestoreChange, onLaunchAtLoginChange }: {
  settings: AppSettings;
  hotkeyLabel: string;
  onEnabledChange: (value: boolean) => void;
  onConsumeChange: (value: boolean) => void;
  onClipboardRestoreChange: (value: boolean) => void;
  onLaunchAtLoginChange: (value: boolean) => void;
}) {
  return (
    <>
      <section className="settings-section">
        <h2>Push to Talk</h2>
        <div className="settings-card">
          <SettingRow title="Dictation" detail="Listen for the global hold key">
            <Toggle checked={settings.enabled} onChange={onEnabledChange} label="Dictation enabled" />
          </SettingRow>
          <SettingRow title="Hold key" detail="Press and hold to record">
            <kbd>{hotkeyLabel}</kbd>
          </SettingRow>
          <SettingRow title="Consume hold key" detail={`Keep ${hotkeyLabel} from reaching the focused app`}>
            <Toggle checked={settings.hotkey.consume} onChange={onConsumeChange} label="Consume hold key" />
          </SettingRow>
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
