import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
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

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Bard could not load its local state.";
}

export default function App() {
  useTheme();

  const [state, setState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [modelProgress, setModelProgress] = useState<ModelProgress | null>(null);
  const [dictation, setDictation] = useState<DictationState>({
    listening: false,
    transcribing: false,
    level: 0,
  });

  useEffect(() => {
    const pendingUnlisteners: Promise<() => void>[] = [];
    let alive = true;
    let disposed = false;

    const refresh = () => {
      if (!alive) return;
      void api
        .getState()
        .then((next) => {
          if (!alive) return;
          setState(next);
          setLoadError(null);
        })
        .catch((error: unknown) => {
          if (alive) setLoadError(errorText(error));
        });
    };

    const subscribe = (subscription: Promise<() => void>) => {
      pendingUnlisteners.push(
        subscription.catch(() => () => {
          // An unavailable optional event must not prevent the rest of the shell
          // from loading. Commands remain the source of truth.
        }),
      );
    };

    refresh();
    subscribe(listen("refresh", refresh));
    subscribe(
      listen("captured", () => {
        window.dispatchEvent(new CustomEvent("bard-captured"));
      }),
    );
    subscribe(
      listen("panel-shown", () => {
        refresh();
      }),
    );
    subscribe(
      listen("dictation-panel", (event) => {
        window.dispatchEvent(
          new CustomEvent("bard-toast", { detail: "Dictated — saved to clipboard" })
        );
        void (event.payload as string);
      }),
    );
    subscribe(
      listen("dictation-error", (event) => {
        const message = event.payload as string;
        window.dispatchEvent(new CustomEvent("bard-toast", { detail: message }));
      }),
    );
    subscribe(
      listen("dictation-cloud-warning", (event) => {
        const message = event.payload as string;
        window.dispatchEvent(
          new CustomEvent("bard-toast", { detail: `Cloud cleanup skipped — ${message}` }),
        );
      }),
    );
    subscribe(
      listen("dictation-overlay", (event) => {
        const status = event.payload as string;
        setDictation((current) => ({
          ...current,
          listening: status === "listening",
          transcribing: status === "transcribing" || status === "finishing",
        }));
      }),
    );
    subscribe(
      listen("mic-level", (event) => {
        const [level] = event.payload as [number, number];
        setDictation((current) => ({ ...current, level }));
      }),
    );
    subscribe(
      listen("model-progress", (event) => {
        setModelProgress(event.payload as ModelProgress);
      }),
    );
    subscribe(
      listen("model-download-failed", (event) => {
        const payload = event.payload as { id?: string; error?: string };
        setModelProgress({
          id: payload.id ?? "unknown",
          received: 0,
          total: 0,
          percent: 0,
          phase: "Download failed",
          error: payload.error ?? "The model download failed.",
        });
      }),
    );
    subscribe(
      listen("model-ready", () => {
        setModelProgress(null);
        refresh();
      }),
    );

    return () => {
      alive = false;
      disposed = true;
      for (const pending of pendingUnlisteners) {
        void pending.then((unlisten) => {
          if (disposed) unlisten();
        });
      }
    };
  }, []);

  if (!state) {
    return (
      <div className="app">
        <div className="onboarding">
          {loadError ? (
            <>
              <AlertCircle width={28} height={28} aria-hidden="true" />
              <h1>Bard could not start</h1>
              <p role="alert">{loadError}</p>
              <button className="btn primary" onClick={() => window.location.reload()}>
                <RotateCcw aria-hidden="true" />
                Try again
              </button>
            </>
          ) : (
            <>
              <LoaderCircle width={28} height={28} aria-hidden="true" />
              <h1>Starting Bard</h1>
              <p role="status">Loading your local workspace…</p>
            </>
          )}
        </div>
      </div>
    );
  }

  if (!state.onboardingCompleted) {
    return (
      <div className="app">
        <Onboarding
          state={state}
          modelProgress={modelProgress}
          onDone={async () => {
            await api.setOnboardingCompleted(true);
            const next = await api.getState();
            setState(next);
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
