import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTheme } from "../useTheme";
import "../styles.css";

function Overlay() {
  useTheme();
  const [state, setState] = useState<"listening" | "transcribing" | "finishing" | "cancelling" | "idle" | null>(null);
  const [levels, setLevels] = useState<number[]>(new Array(12).fill(0.08));

  useEffect(() => {
    void listen("dictation-overlay", (e) => {
      const next = e.payload as "listening" | "transcribing" | "finishing" | "cancelling" | "idle";
      setState(next === "idle" ? null : next);
    });
    void listen("dictation-result", () => setState(null));
    void listen("dictation-error", () => setState(null));
    void listen("mic-level", (e) => {
      const [level] = e.payload as [number, number];
      setLevels((levels) => [...levels.slice(1), Math.max(0.06, Math.min(1, level))]);
    });
  }, []);

  if (!state) return null;
  return (
    <div className="overlay-stage">
      <div className="overlay-pill">
        <span className="dot" />
        <span>
          {state === "listening"
            ? "Listening"
            : state === "finishing"
              ? "Finishing"
              : state === "cancelling"
                ? "Cancelling"
                : "Transcribing"}
        </span>
        <span className="bars">
          {levels.map((l, i) => (
            <i key={i} style={{ transform: `scaleY(${Math.max(0.3, Math.min(1, l))})` }} />
          ))}
        </span>
        <button
          className="overlay-x"
          title="Cancel"
          aria-label="Cancel dictation"
          onClick={() => void invoke("dictate_cancel")}
        >
          <X aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Overlay />
  </StrictMode>
);