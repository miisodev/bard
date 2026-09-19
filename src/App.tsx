import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { api } from "./store";
import type { AppState, ModelProgress } from "./types";
import Panel from "./components/Panel";
import Onboarding from "./components/Onboarding";

export interface DictationState {
  listening: boolean;
  transcribing: boolean;
  level: number;
}

export default function App() {
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

    const onTheme = () => {
      if (alive && stateRef.current) applyTheme(stateRef.current.theme);
    };
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", onTheme);
    return () => {
      alive = false;
      for (const u of unlisteners) void u.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    stateRef.current = state;
    if (state) applyTheme(state.theme);
  }, [state?.theme]);

  useEffect(() => {
    if (!state?.onboardingCompleted) return;
    let timer: number | undefined;
    const onBlur = () => {
      if (timer) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        void getCurrentWindow().isMinimized().then((m) => {
          if (!m) void api.hidePanel();
        });
      }, 150);
    };
    const onFocus = () => {
      if (timer) {
        window.clearTimeout(timer);
        timer = undefined;
      }
    };
    window.addEventListener("blur", onBlur);
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("focus", onFocus);
      if (timer) window.clearTimeout(timer);
    };
  }, [state?.onboardingCompleted]);

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
      <Panel state={state} modelProgress={modelProgress} dictation={dictation} onState={setState} />
    </div>
  );
}

/*
  Four choices, two axes. Glass is presented as a theme because that is how a
  user thinks about it, but it is really "translucent" layered over whichever of
  light/dark the OS is in — so picking it still follows the system appearance.
*/
export function applyTheme(theme: string) {
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const glass = theme === "glass";
  const resolved =
    theme === "light" || theme === "dark" ? theme : systemDark ? "dark" : "light";

  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.dataset.glass = glass ? "1" : "0";
}