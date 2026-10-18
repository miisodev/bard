import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { StrictMode, useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { boot } from "../dev/boot";
import type { DictationReviewPayload } from "../types";
import { useTheme } from "../useTheme";
import ReviewSheet from "./ReviewSheet";
import { reviewReduce, type ReviewEvent, type ReviewState } from "./review";
import "../styles.css";

type Phase = "listening" | "transcribing" | "finishing" | "cancelling";

/** How many meter bars the pill draws. */
const LEVEL_BARS = 12;
/** The resting height of a bar, used before any audio has arrived. */
const IDLE_BAR_HEIGHT = 0.08;

/*
  The pill shows a live take or a short report of why one ended. Rust decides
  when it is on screen; this only draws what it is told. While a take is live
  a click elsewhere is held back and the × flashes red — it is the way out.

  A take that opens for review grows the same window into the sheet; Rust
  resizes and re-anchors it, and owns Escape, which is why nothing here binds
  it.
*/
function Overlay() {
  useTheme();
  const [phase, setPhase] = useState<Phase | null>(null);
  const [report, setReport] = useState<string | null>(null);
  const [nudge, setNudge] = useState(0);
  const [review, setReview] = useState<ReviewState | null>(null);
  const [levels, setLevels] = useState<number[]>(() =>
    new Array(LEVEL_BARS).fill(IDLE_BAR_HEIGHT),
  );

  const dispatchReview = useCallback(
    (event: ReviewEvent) => setReview((state) => reviewReduce(state, event)),
    [],
  );

  useEffect(() => {
    let alive = true;
    const pending: Promise<() => void>[] = [];
    const subscribe = (subscription: Promise<() => void>) => {
      pending.push(subscription.catch(() => () => undefined));
    };
    subscribe(
      listen("dictation-overlay", (e) => {
        if (!alive) return;
        const next = e.payload as Phase | "idle";
        setReport(null);
        // A new take, or none: the × starts calm again.
        if (next === "idle" || next === "listening") setNudge(0);
        setPhase(next === "idle" ? null : next);
      }),
    );
    subscribe(
      listen("dictation-overlay-report", (e) => {
        if (!alive) return;
        setPhase(null);
        setNudge(0);
        setReport(typeof e.payload === "string" ? e.payload : "Dictation failed");
      }),
    );
    subscribe(
      listen("dictation-overlay-nudge", () => {
        if (alive) setNudge((n) => n + 1);
      }),
    );
    subscribe(
      listen("dictation-review", (e) => {
        if (!alive) return;
        const payload = e.payload as DictationReviewPayload | string | null;
        const text = typeof payload === "string" ? payload : payload?.text ?? "";
        setNudge(0);
        dispatchReview({ type: "open", text });
      }),
    );
    subscribe(
      listen("dictation-review-error", (e) => {
        if (!alive) return;
        const message =
          typeof e.payload === "string" && e.payload.trim() ? e.payload : "Review failed.";
        dispatchReview({ type: "failed", message });
      }),
    );
    // Rust's dismiss path, Escape included, closes the sheet; the command
    // itself is Rust's, so this only lets go of the state.
    subscribe(
      listen("dictation-cancelled", () => {
        if (alive) dispatchReview({ type: "close" });
      }),
    );
    subscribe(
      listen("mic-level", (e) => {
        const [level] = e.payload as [number, number];
        if (!alive) return;
        // Mutated in place rather than rebuilt. A twelve-element array is cheap,
        // but this fires for every meter tick and the overlay is the one window
        // that must never drop a frame — it sits over whatever the person is
        // doing, and a stutter there reads as the whole app stuttering.
        const next = Math.max(0.06, Math.min(1, level));
        setLevels((levels) => {
          levels.push(next);
          if (levels.length > LEVEL_BARS) levels.splice(0, levels.length - LEVEL_BARS);
          return levels;
        });
      }),
    );
    return () => {
      alive = false;
      for (const unlisten of pending) void unlisten.then((fn) => fn());
    };
  }, [dispatchReview]);

  if (review) {
    return (
      <div className="overlay-stage">
        <ReviewSheet review={review} onEvent={dispatchReview} />
      </div>
    );
  }
  if (!phase && !report) return null;
  const label = report
    ? report
    : phase === "listening"
      ? "Listening"
      : phase === "finishing"
        ? "Finishing"
        : phase === "cancelling"
          ? "Cancelling"
          : "Transcribing";
  const live = phase !== null && phase !== "cancelling";
  return (
    <div className="overlay-stage">
      <div className="overlay-pill" role="status" aria-live="polite" aria-label={label}>
        <span className="dot" data-state={report ? "report" : phase} />
        <span className="overlay-label">{label}</span>
        {report ? null : (
          <span className="bars" aria-hidden="true">
            {levels.map((l, i) => (
              <i key={i} style={{ transform: `scaleY(${Math.max(0.3, Math.min(1, l))})` }} />
            ))}
          </span>
        )}
        <button
          // Re-keyed on every nudge so the flash restarts on each held click.
          key={live ? `x${nudge}` : "x"}
          className={`overlay-x${live && nudge > 0 ? " nudge" : ""}`}
          title={live ? "Cancel" : "Dismiss"}
          aria-label={live ? "Cancel dictation" : "Dismiss"}
          onClick={() => void invoke("dictate_cancel")}
        >
          <X aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}

void boot("overlay").then(() =>
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <Overlay />
    </StrictMode>,
  ),
);
