import type { ReactNode } from "react";

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (value: boolean) => void; label: string }) {
  return (
    <button type="button" className={"toggle " + (checked ? "on" : "")} onClick={() => onChange(!checked)} role="switch" aria-checked={checked} aria-label={label}>
      <span />
    </button>
  );
}

export function SettingRow({ title, detail, children }: { title: string; detail?: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div className="setting-copy">
        <strong>{title}</strong>
        {detail && <small>{detail}</small>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}
