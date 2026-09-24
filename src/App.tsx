import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "./store";
import { playCue } from "./audio";
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

export interface AppProps {
  /** Optional host-provided current clipboard item; captured events also update it. */
  activeItemId?: number | null;
}

export default function App({ activeItemId: suppliedActiveItemId }: AppProps = {}) {
  useTheme();

  const [state, setState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [modelProgress, setModelProgress] = useState<ModelProgress | null>(null);
  const [dictation, setDictation] = useState<DictationState>({
    listening: false,
    transcribing: false,
    level: 0,
  });
  const [activeItemId, setActiveItemId] = useState<number | null>(suppliedActiveItemId ?? null);

  useEffect(() => {
    setActiveItemId(suppliedActiveItemId ?? null);
  }, [suppliedActiveItemId]);

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
          if (next.activeItemId !== undefined) setActiveItemId(next.activeItemId);
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
    const clearDictation = () =>
      setDictation((current) => ({ ...current, listening: false, transcribing: false }));

    refresh();
    void api.refreshLicense().then((license) => {
      if (!alive) return;
      setState((current) => (current ? { ...current, license } : current));
    }).catch(() => undefined);
    subscribe(listen("refresh", refresh));
    subscribe(
      listen("captured", (event) => {
        const id = Number(event.payload);
        if (Number.isFinite(id)) setActiveItemId(id);
        window.dispatchEvent(new CustomEvent("bard-captured", { detail: id }));
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
        clearDictation();
        playCue("error");
        const message = event.payload as string;
        window.dispatchEvent(new CustomEvent("bard-toast", { detail: message }));
      }),
    );
    subscribe(listen("dictation-cancelled", clearDictation));
    subscribe(
      listen("license-restricted", (event) => {
        const message = typeof event.payload === "string" ? event.payload : "Bard needs an active licence.";
        window.dispatchEvent(new CustomEvent("bard-toast", { detail: message }));
        refresh();
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
        setDictation((current) => {
          const listening = status === "listening";
          if (listening && !current.listening) playCue("dictate-start");
          if (!listening && current.listening && status !== "transcribing") playCue("dictate-stop");
          return {
            ...current,
            listening,
            transcribing: status === "transcribing" || status === "finishing",
          };
        });
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
      <Panel
        state={state}
        dictation={dictation}
        onState={setState}
        activeItemId={activeItemId}
        onActiveItemChange={setActiveItemId}
      />
    </div>
  );
}

/**
 * Resolve and apply the theme. "system" follows the OS and keeps following it;
 * `useTheme` re-runs this when the OS appearance changes.
 */
