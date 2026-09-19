import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "./store";
import type { AppState, ModelProgress } from "./types";
import Panel from "./components/Panel";
import Onboarding from "./components/Onboarding";
import { useTheme } from "./useTheme";

export interface DictationState {
  listening: boolean;
  transcribing: boolean;
  level: number;
}

export default function App() {
  useTheme();

  const [state, setState] = useState<AppState | null>(null);
  const [modelProgress, setModelProgress] = useState<ModelProgress | null>(null);
  const [dictation, setDictation] = useState<DictationState>({
    listening: false,
    transcribing: false,
    level: 0,
  });
  const stateRef = { current: state };

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];
    let alive = true;

    const refresh = () => {
      if (alive) void api.getState().then(setState);
    };

    void api.getState().then(setState);
    unlisteners.push(listen("refresh", refresh));
    unlisteners.push(
      listen("captured", () => {
        window.dispatchEvent(new CustomEvent("bard-captured"));
      })
    );
    unlisteners.push(
      listen("panel-shown", () => {
        refresh();
      })
    );
    unlisteners.push(
      listen("dictation-panel", (e) => {
        window.dispatchEvent(
          new CustomEvent("bard-toast", { detail: "Dictated — saved to clipboard" })
        );
        void (e.payload as string);
      })
    );
    unlisteners.push(
      listen("dictation-error", (e) => {
        const msg = e.payload as string;
        window.dispatchEvent(new CustomEvent("bard-toast", { detail: msg }));
      })
    );
    unlisteners.push(
      listen("dictation-overlay", (e) => {
        const s = e.payload as string;
        setDictation((d) => ({
          ...d,
          listening: s === "listening",
          transcribing: s === "transcribing",
        }));
      })
    );
    unlisteners.push(
      listen("mic-level", (e) => {
        const [level] = e.payload as [number, number];
        setDictation((d) => ({ ...d, level }));
      })
    );
    unlisteners.push(
      listen("model-progress", (e) => {
        setModelProgress(e.payload as ModelProgress);
      })
    );
    unlisteners.push(
      listen("model-ready", () => {
        setModelProgress(null);
        refresh();
      })
    );

    return () => {
      alive = false;
      for (const u of unlisteners) void u.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    stateRef.current = state;
  }, [state?.theme]);

  if (!state) {
    return <div className="app" />;
  }

  if (!state.onboardingCompleted) {
    return (
      <div className="app">
        <Onboarding
          state={state}
          modelProgress={modelProgress}
          onDone={() => {
            void api.setOnboardingCompleted(true);
            void api.getState().then(setState);
          }}
        />
      </div>
    );
  }

  return (
    <div className="app">
      <Panel state={state} dictation={dictation} onState={setState} />
    </div>
  );
}

/**
 * Resolve and apply the theme. "system" follows the OS and keeps following it;
 * `useTheme` re-runs this when the OS appearance changes.
 */
export function applyTheme(theme: string) {
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const resolved =
    theme === "light" || theme === "dark" ? theme : systemDark ? "dark" : "light";
  document.documentElement.dataset.theme = resolved;
}
