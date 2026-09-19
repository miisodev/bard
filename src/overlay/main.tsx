import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTheme } from "../useTheme";
import "../styles.css";

function Overlay() {
  useTheme();
  const [state, setState] = useState<"listening" | "transcribing" | null>(null);
  const [levels, setLevels] = useState<number[]>(new Array(12).fill(0.08));

  useEffect(() => {
    void listen("dictation-overlay", (e) => {
      setState(e.payload as "listening" | "transcribing");
    });
    void listen("dictation-result", () => setState(null));
    void listen("dictation-error", () => setState(null));
    void listen("mic-level", (e) => {
      const [level] = e.payload as [number, number];
      const next = [...levels.slice(1), Math.max(0.06, Math.min(1, level))];
      setLevels(next);
    });
  }, []);

  if (!state) return null;
  return (
    <div className="overlay-pill">
      <span className="dot" />
      <span>{state === "listening" ? "Listening…" : "Transcribing…"}</span>
      <span className="bars">
        {levels.map((l, i) => (
          <i key={i} style={{ height: `${Math.max(3, l * 12)}px` }} />
        ))}
      </span>
      <button className="overlay-x" title="Cancel" onClick={() => void invoke("dictate_cancel")}>
        ✕
      </button>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Overlay />
  </StrictMode>
);