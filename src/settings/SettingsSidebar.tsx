import { Gauge, Headphones, Info, Search, Settings2, SunMoon, type LucideIcon } from "lucide-react";
import type { SettingsSection } from "./types";

const sections: Array<{ id: SettingsSection; label: string; search: string; icon: LucideIcon }> = [
  { id: "dictation", label: "Dictation", search: "dictation push to talk", icon: Settings2 },
  { id: "audio", label: "Audio", search: "audio microphone", icon: Headphones },
  { id: "engine", label: "Local Engine", search: "local engine model", icon: Gauge },
  { id: "appearance", label: "Appearance", search: "appearance theme light dark system", icon: SunMoon },
  { id: "legal", label: "Legal & Privacy", search: "legal privacy license notices model nemotron parakeet runtime", icon: Info }
];

export function SettingsSidebar({ activeSection, onSectionChange, searchQuery, onSearchChange }: {
  activeSection: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
}) {
  const query = searchQuery.trim().toLowerCase();
  const visibleSections = sections.filter((section) => section.search.includes(query));

  return (
    <aside className="settings-sidebar">
      <div className="sidebar-search">
        <Search />
        <input type="search" aria-label="Search settings" placeholder="Search" value={searchQuery} onChange={(event) => onSearchChange(event.target.value)} />
      </div>

      <p className="sidebar-label">Zui Voice</p>
      <nav className="settings-nav" aria-label="Settings sections">
        {visibleSections.map(({ id, label, icon: Icon }) => (
          <button type="button" aria-label={label} className={activeSection === id ? "active" : ""} onClick={() => onSectionChange(id)} key={id}>
            <span className={`nav-icon ${id}`}><Icon /></span>
            <strong>{label}</strong>
          </button>
        ))}
        {visibleSections.length === 0 && <p className="no-results">No settings found</p>}
      </nav>
    </aside>
  );
}
