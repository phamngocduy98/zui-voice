import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function MacOverlayHeader({ title }: { title: string }) {
  const handleMouseDown = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.buttons !== 1 || !("__TAURI_INTERNALS__" in window)) return;
    const appWindow = getCurrentWindow();
    if (event.detail === 2) void appWindow.toggleMaximize();
    else void appWindow.startDragging();
  };

  return (
    <div
      className="section-header"
      data-platform="macos"
      data-tauri-drag-region
      data-testid="section-header"
      onMouseDown={handleMouseDown}
    >
      <div className="section-header-pane"><h1>{title}</h1></div>
    </div>
  );
}
