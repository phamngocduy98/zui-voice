import { useEffect, useRef } from "react";
import { Mic2 } from "lucide-react";
import { debugStart, debugStop } from "../api";

export function MicrophoneTestButton() {
  const active = useRef(false);

  const start = () => {
    if (active.current) return;
    active.current = true;
    void debugStart().catch(() => {
      active.current = false;
    });
  };
  const stop = () => {
    if (!active.current) return;
    active.current = false;
    void debugStop();
  };

  useEffect(() => {
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("blur", stop);
      stop();
    };
  }, []);

  return (
    <button
      type="button"
      className="secondary-button"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        start();
      }}
      onPointerUp={stop}
      onPointerCancel={stop}
      onLostPointerCapture={stop}
      onKeyDown={(event) => {
        if (!event.repeat && (event.key === " " || event.key === "Enter")) start();
      }}
      onKeyUp={(event) => {
        if (event.key === " " || event.key === "Enter") stop();
      }}
    >
      <Mic2 />Hold to Test
    </button>
  );
}
