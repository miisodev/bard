import { listen } from "@tauri-apps/api/event";
import { Component, useEffect, useRef, useState, type ReactNode } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "./store";
import { insertCaptured } from "./shelf";
import type { AppState, CapturedEvent, ModelProgress } from "./types";
import Clipboard from "./components/Clipboard";
import Onboarding from "./components/Onboarding";
import { useTheme } from "./useTheme";

/*
  The `refresh` event arrives in bursts (a bulk action fires one per write),
  and each one used to cost a full state read. A one-frame trailing debounce
  coalesces the burst into the last read.
*/
const REFRESH_DEBOUNCE_MS = 16;

function errorText(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || fallback;
}

/**
 * A render throw in this window is a blank rectangle. The panel is frameless,
 * untitled and `skipTaskbar`, so there is no title bar to read and no taskbar
 * entry to fall back on — nothing distinguishes a crash from an app that simply
 * did not open. This boundary is the only thing standing between that and a
 * user who concludes Bard is broken: it names the failure, keeps the detail in
 * the console for support, and offers the one action that can fix it.
 */
class ErrorBoundary extends Component<{ children: ReactNode }, { failure: Error | null }> {
  state: { failure: Error | null } = { failure: null };

  static getDerivedStateFromError(failure: Error) {
    return { failure };
  }

  componentDidCatch(failure: Error, info: { componentStack?: string | null }) {
    console.error("Bard could not render.", failure, info.componentStack ?? "");
  }

  render() {
    const { failure } = this.state;
    if (!failure) return this.props.children;
    return (
      <div className="app">
        <div className="onboarding">
          <AlertCircle width={28} height={28} aria-hidden="true" />
          <h1>Bard ran into a problem</h1>
          <p role="alert">{errorText(failure, "The shelf stopped unexpectedly.")}</p>
          <button className="btn primary" onClick={() => window.location.reload()}>
            <RotateCcw aria-hidden="true" />
            Reload Bard
          </button>
        </div>
      </div>
    );
  }
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
  const [activeItemId, setActiveItemId] = useState<number | null>(suppliedActiveItemId ?? null);
  const refreshTimer = useRef<number | undefined>(undefined);
  const storageNoticeShown = useRef<string | null>(null);

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
          if (next.storageNotice && next.storageNotice !== storageNoticeShown.current) {
            storageNoticeShown.current = next.storageNotice;
            void api.notifyError(next.storageNotice);
          }
        })
        .catch((error: unknown) => {
          if (alive) setLoadError(errorText(error, "Bard could not load its local state."));
        });
    };

    const subscribe = (subscription: Promise<() => void>) => {
      pendingUnlisteners.push(subscription.catch(() => () => undefined));
    };
    const scheduleRefresh = () => {
      if (refreshTimer.current !== undefined) window.clearTimeout(refreshTimer.current);
      refreshTimer.current = window.setTimeout(() => {
        refreshTimer.current = undefined;
        refresh();
      }, REFRESH_DEBOUNCE_MS);
    };

    refresh();
    subscribe(listen("refresh", scheduleRefresh));
    subscribe(listen("capture-paused", scheduleRefresh));
    subscribe(
      listen<CapturedEvent>("captured", (event) => {
        // The same guard the refresh path carries. An event that arrives
        // after this window has gone must not call a setter on an unmounted
        // component: React warns about it today and drops the update, but the
        // window is torn down on a hide, and the event that crossed the gap
        // belonged to a shelf that is no longer on screen.
        if (!alive) return;
        const payload = event.payload;
        const id = typeof payload === "number" ? payload : Number(payload?.id);
        if (Number.isFinite(id)) setActiveItemId(id);
        const item = typeof payload === "object" && payload !== null ? payload.item : undefined;
        if (item && !payload.removed) {
          setState((current) =>
            current ? { ...current, items: insertCaptured(current.items, item) } : current,
          );
        } else {
          scheduleRefresh();
        }
      }),
    );
    subscribe(listen("panel-shown", () => requestAnimationFrame(() => refresh())));
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
      if (refreshTimer.current !== undefined) window.clearTimeout(refreshTimer.current);
      for (const pending of pendingUnlisteners) {
        void pending.then((unlisten) => {
          if (disposed) unlisten();
        });
      }
    };
  }, []);

  const screen = !state ? (
    <div className="app">
      <div className="onboarding">
        {loadError ? (
          <>
            <AlertCircle width={28} height={28} aria-hidden="true" />
            <h1>Bard could not start</h1>
            <p role="alert">{errorText(loadError, "Bard could not load its local state.")}</p>
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
  ) : !state.onboardingCompleted ? (
    <div className="app setup">
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
  ) : (
    <div className="app" data-dock={state.dock}>
      <Clipboard state={state} onState={setState} activeItemId={activeItemId} />
    </div>
  );

  return <ErrorBoundary>{screen}</ErrorBoundary>;
}
