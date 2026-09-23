import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTheme } from "../useTheme";
import "../styles.css";

function Overlay() {
  useTheme();
  const [state, setState] = useState<"listening" | "transcribing" | "finishing" | "cancelling" | "error" | "idle" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [levels, setLevels] = useState<number[]>(new Array(12).fill(0.08));

  useEffect(() => {
    let alive = true;
    const pending: Promise<() => void>[] = [];
    const subscribe = (subscription: Promise<() => void>) => {
      pending.push(subscription.catch(() => () => undefined));
    };
    subscribe(
      listen("dictation-overlay", (e) => {
        const next = e.payload as "listening" | "transcribing" | "finishing" | "cancelling" | "idle";
        if (alive) {
          setError(null);
          setState(next === "idle" ? null : next);
        }
      }),
    );
    subscribe(listen("dictation-result", () => {
      if (!alive) return;
      setError(null);
      setState(null);
    }));
    subscribe(listen("dictation-error", (event) => {
      if (!alive) return;
      setError(typeof event.payload === "string" ? event.payload : "Dictation failed");
      setState("error");
      window.setTimeout(() => {
        if (alive) setState(null);
      }, 5000);
    }));
    subscribe(
      listen("mic-level", (e) => {
        const [level] = e.payload as [number, number];
        if (alive) setLevels((levels) => [...levels.slice(1), Math.max(0.06, Math.min(1, level))]);
      }),
    );
    return () => {
      alive = false;
      for (const unlisten of pending) void unlisten.then((fn) => fn());
    };
  }, []);

  if (!state) return null;
  const label =
    state === "listening"
      ? "Listening"
      : state === "finishing"
        ? "Finishing"
        : state === "cancelling"
          ? "Cancelling"
          : state === "error"
            ? error || "Dictation failed"
            : "Transcribing";
  return (
    <div className="overlay-stage">
      <div className="overlay-pill" role="status" aria-live="polite" aria-label={label}>
        <span className="dot" data-state={state} />
        <span>{label}</span>
        <span className="bars" aria-hidden="true">
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