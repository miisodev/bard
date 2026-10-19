import { listen } from "@tauri-apps/api/event";
import { Component, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "./store";
import { playCue } from "./audio";
import type { AppState, ModelProgress, ShelfEdge } from "./types";
import Panel, { type ToastDetail } from "./components/Panel";
import Onboarding from "./components/Onboarding";
import { useTheme } from "./useTheme";

export interface DictationState {
  listening: boolean;
  transcribing: boolean;
  level: number;
}

/*
  The `refresh` event arrives in bursts — a dictation take inserts an item per
  beat, a bulk action fires one per write — and each one used to cost a full
  state read. A short trailing debounce coalesces the burst into the last
  read, which is the only one whose answer is still current.
*/
const REFRESH_DEBOUNCE_MS = 50;

/*
  How long the slide-out may take before Rust is told to proceed anyway. The
  animation is --t-instant (90ms); the margin is the floor under a missed
  `animationend`, so a slide that never reports can never hang the hide.
*/
const SHELF_OUT_ACK_MS = 140;

function reducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function isShelfEdge(value: unknown): value is ShelfEdge {
  return value === "bottom" || value === "top" || value === "left" || value === "right";
}

/**
 * `mic-level` is the hottest path in the shell, and Rust emits it at 5 Hz and
 * not at all when the quantised level has not moved — so a silent take produces
 * one message rather than 24,000. Three guards remain here: the level is
 * quantised to 5% steps — finer than that is not visible in anything — the
 * comparison is on whole steps, so float noise cannot defeat it, and a value
 * is published at most every 120 ms. The setter also returns the *same* object
 * when nothing changed, which is the only way React can skip the re-render.
 */
const LEVEL_STEP = 0.05;
const LEVEL_MIN_INTERVAL_MS = 120;

function errorText(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || fallback;
}

/**
 * The panel shows what it is told. A bare string is a confirmation and clears
 * itself; an object carrying a tone is a failure and stays up until it has been
 * read, so every `listen` below has to decide which kind of thing it is.
 */
function toast(detail: string | ToastDetail) {
  window.dispatchEvent(new CustomEvent("bard-toast", { detail }));
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
          <p role="alert">{errorText(failure, "The panel stopped unexpectedly.")}</p>
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
  const [dictation, setDictation] = useState<DictationState>({
    listening: false,
    transcribing: false,
    level: 0,
  });
  const [activeItemId, setActiveItemId] = useState<number | null>(suppliedActiveItemId ?? null);
  // Which edge the window is docked to. Rust owns the geometry; this drives
  // the layout, the entry slide and which way it slides.
  const [edge, setEdge] = useState<ShelfEdge>(() => "bottom");
  // The live input level, and when it was last handed to React. Keeping the
  // rate down here is what stops a take from re-rendering every card 20 times a
  // second — see LEVEL_MIN_INTERVAL_MS.
  const publishedStep = useRef(0);
  const publishedAt = useRef(0);
  // The slide-out handshake: Rust waits for `shelf_hidden`, so it is sent
  // exactly once per hide, from whichever signal arrives first.
  const rootRef = useRef<HTMLDivElement | null>(null);
  const ackTimer = useRef<number | undefined>(undefined);
  const ackSent = useRef(false);
  const refreshTimer = useRef<number | undefined>(undefined);
  // The storage notice is read once per distinct message. It is state, not an
  // event, because the failure happens before any window exists to toast into.
  const storageNoticeShown = useRef<string | null>(null);

  useEffect(() => {
    setActiveItemId(suppliedActiveItemId ?? null);
  }, [suppliedActiveItemId]);

  useEffect(() => {
    if (state?.shelfEdge) setEdge(state.shelfEdge);
  }, [state?.shelfEdge]);

  /** Rust may proceed: restore the foreground, hide and park the window. */
  const acknowledgeHide = useCallback(() => {
    if (ackSent.current) return;
    ackSent.current = true;
    if (ackTimer.current !== undefined) {
      window.clearTimeout(ackTimer.current);
      ackTimer.current = undefined;
    }
    if (rootRef.current) delete rootRef.current.dataset.anim;
    void api.shelfHidden();
  }, []);

  /** Slide the band in from its edge. Reduced motion gets the resting state. */
  const animateIn = useCallback(() => {
    if (reducedMotion()) return;
    const el = rootRef.current;
    if (!el) return;
    // Clear, commit, re-add: a re-shown window restarts the keyframes rather
    // than sitting at the end of an animation CSS still considers finished.
    delete el.dataset.anim;
    void el.offsetWidth;
    el.dataset.anim = "in";
  }, []);

  /** Slide out, then answer. `shelf-hiding` is Rust asking permission to hide. */
  const animateOut = useCallback(() => {
    ackSent.current = false;
    const el = rootRef.current;
    if (reducedMotion() || !el) {
      acknowledgeHide();
      return;
    }
    delete el.dataset.anim;
    void el.offsetWidth;
    el.dataset.anim = "out";
    if (ackTimer.current !== undefined) window.clearTimeout(ackTimer.current);
    ackTimer.current = window.setTimeout(acknowledgeHide, SHELF_OUT_ACK_MS);
  }, [acknowledgeHide]);

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
          // A damaged history file is handled before a window can exist, so
          // the notice travels in the state every surface already reads. It is
          // shown once per distinct message; a relaunch re-reports it because
          // the file is still there.
          if (next.storageNotice && next.storageNotice !== storageNoticeShown.current) {
            storageNoticeShown.current = next.storageNotice;
            toast({ text: next.storageNotice, tone: "error" });
          }
        })
        .catch((error: unknown) => {
          if (alive) setLoadError(errorText(error, "Bard could not load its local state."));
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
    /** Coalesce a burst of `refresh` events into one trailing read. */
    const scheduleRefresh = () => {
      if (refreshTimer.current !== undefined) window.clearTimeout(refreshTimer.current);
      refreshTimer.current = window.setTimeout(() => {
        refreshTimer.current = undefined;
        refresh();
      }, REFRESH_DEBOUNCE_MS);
    };
    const clearDictation = () =>
      setDictation((current) => ({ ...current, listening: false, transcribing: false }));

    refresh();
    void api.refreshLicense().then((license) => {
      if (!alive) return;
      setState((current) => (current ? { ...current, license } : current));
    }).catch(() => undefined);
    subscribe(listen("refresh", scheduleRefresh));
    subscribe(
      listen("captured", (event) => {
        const id = Number(event.payload);
        if (Number.isFinite(id)) setActiveItemId(id);
        window.dispatchEvent(new CustomEvent("bard-captured", { detail: id }));
      }),
    );
    subscribe(
      listen("panel-shown", () => {
        animateIn();
        // Paint first: the window is already on screen with the state it last
        // rendered, and the read starts on the next frame so it cannot delay
        // that first paint.
        requestAnimationFrame(() => refresh());
      }),
    );
    subscribe(listen("shelf-hiding", animateOut));
    subscribe(
      listen("shelf-edge", (event) => {
        if (isShelfEdge(event.payload)) setEdge(event.payload);
      }),
    );
    subscribe(
      listen("dictation-panel", (event) => {
        toast("Dictated — saved to clipboard");
        void (event.payload as string);
      }),
    );
    subscribe(
      listen("dictation-error", (event) => {
        clearDictation();
        playCue("error");
        toast({ text: event.payload as string, tone: "error" });
      }),
    );
    subscribe(listen("dictation-cancelled", clearDictation));
    subscribe(
      listen("license-restricted", (event) => {
        const message = typeof event.payload === "string" ? event.payload : "Bard needs an active licence.";
        toast({ text: message, tone: "error" });
        refresh();
      }),
    );
    // The capture gesture does its work — sentinel on the clipboard, a synthetic
    // copy, fourteen polls of the result — and when nothing came back it emitted
    // `capture-failed` into a window with no listener, so a first-run user with
    // nothing selected was told nothing at all. The clipboard is left as it was
    // found, and the event carries no payload, so the message has to hold for
    // both reasons it fires: an empty clipboard, and a clipboard holding
    // something that is neither text nor an image. Both are answered the same
    // way, so both get the same sentence.
    subscribe(
      listen("capture-failed", () => {
        playCue("error");
        toast({
          text: "Nothing to capture. Select text or an image, then tap Left Shift twice.",
          tone: "error",
        });
      }),
    );
    // A write that could not happen. `insert_item_with_origin` and friends used
    // to unwrap on `execute`, so a full disk took the whole process down under
    // `panic = "abort"` — no window, no message. They now return a sentinel and
    // emit this instead, and without a listener the sentinel would just be
    // silent data loss: a capture that vanishes with nothing said anywhere.
    subscribe(
      listen<string>("db-write-failed", (event) => {
        playCue("error");
        toast({
          text: event.payload || "Bard could not save this. Check that the drive has free space.",
          tone: "error",
        });
      }),
    );
    subscribe(
      listen("dictation-cloud-warning", (event) => {
        toast({ text: `Cloud cleanup skipped — ${event.payload as string}`, tone: "error" });
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
        const raw = Number((event.payload as [number, number])[0]);
        if (!Number.isFinite(raw)) return;
        const step = Math.round(raw / LEVEL_STEP);
        if (step === publishedStep.current) return;
        const now = performance.now();
        if (now - publishedAt.current < LEVEL_MIN_INTERVAL_MS) return;
        publishedStep.current = step;
        publishedAt.current = now;
        const level = step * LEVEL_STEP;
        // Returning `current` unchanged is what lets React bail out entirely.
        setDictation((current) => (current.level === level ? current : { ...current, level }));
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
      if (refreshTimer.current !== undefined) window.clearTimeout(refreshTimer.current);
      for (const pending of pendingUnlisteners) {
        void pending.then((unlisten) => {
          if (disposed) unlisten();
        });
      }
    };
  }, [animateIn, animateOut]);

  // One boundary around every screen the shell can show. The panel is the
  // screen that matters and the boot and onboarding states are what it looks
  // like a second earlier, so all three fail the same visible way.
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
    <div className="app" data-edge={edge}>
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
    <div
      ref={rootRef}
      className="app"
      data-edge={edge}
      onAnimationEnd={(event) => {
        // A card's own arrival animation bubbles through here too; only the
        // band's slide is the hide handshake's business.
        if (event.animationName.startsWith("shelf-out")) {
          acknowledgeHide();
        } else if (event.animationName.startsWith("shelf-in")) {
          delete (event.currentTarget as HTMLElement).dataset.anim;
        }
      }}
    >
      <Panel
        state={state}
        dictation={dictation}
        onState={setState}
        activeItemId={activeItemId}
        onActiveItemChange={setActiveItemId}
        edge={edge}
      />
    </div>
  );

  return <ErrorBoundary>{screen}</ErrorBoundary>;
}
