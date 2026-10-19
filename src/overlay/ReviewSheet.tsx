import { AlertCircle, X } from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import { Button } from "../settings/ui";
import { api } from "../store";
import {
  REVIEW_BINDINGS,
  reviewCommand,
  type ReviewCommand,
  type ReviewEvent,
  type ReviewState,
} from "./review";

/** Rust's refusals are written for a person; an invoke failure is not. */
function failureText(reason: unknown, fallback: string): string {
  if (reason instanceof Error && reason.message) return humanise(reason.message);
  const text = String(reason ?? "").replace(/^Error:\s*/i, "").trim();
  return text ? humanise(text) : fallback;
}

/*
  One refusal arrives written for the log rather than for a person: the take
  this sheet is about has already ended — confirmed, cancelled or filed — which
  the sheet cannot fix and the person cannot act on. Everything else Rust says
  for these commands is a sentence someone can read, and is passed through.
*/
function humanise(text: string): string {
  return /no dictation waiting in the review sheet/i.test(text)
    ? "This take was already finished."
    : text;
}

const FALLBACK: Record<ReviewCommand, string> = {
  copy: "Bard couldn't copy the text.",
  confirm: "Bard couldn't reach the app it started in.",
  dismiss: "Bard couldn't close the review.",
};

/**
 * The body's focus: how many tries it gets and how far apart. The native window
 * is brought forward with its own bounded retry (overlay.rs); this is the
 * webview's half of the same moment.
 */
const FOCUS_ATTEMPTS = 3;
const FOCUS_RETRY_MS = 40;

/*
  A finished take, opened for review: the pill grown into a sheet. Every
  command carries the body as it stands, because the edits travelled with it,
  and the window may never take focus — so every control is operable with a
  mouse as well as from the keyboard.
*/
export default function ReviewSheet({
  review,
  onEvent,
}: {
  review: ReviewState;
  onEvent: (event: ReviewEvent) => void;
}) {
  const body = useRef<HTMLTextAreaElement>(null);
  // True while a command is in flight, so a held Enter or a double click
  // cannot fire two. The reducer refuses the second submit as well.
  const inFlight = useRef(false);

  useEffect(() => {
    // WebView2 can report the webview unfocused for a tick after the native
    // window takes focus, and the sheet is the one dialog whose whole point is
    // the caret: a single attempt that landed early left the body unfocused
    // until the person clicked it. Three tries at a short interval, and the
    // first one that takes ends them.
    let attempt = 0;
    let timer = 0;
    const focus = () => {
      const target = body.current;
      if (!target) return;
      target.focus();
      attempt += 1;
      if (attempt < FOCUS_ATTEMPTS && document.activeElement !== target) {
        timer = window.setTimeout(focus, FOCUS_RETRY_MS);
      }
    };
    focus();
    return () => window.clearTimeout(timer);
  }, []);

  const run = useCallback(
    (command: ReviewCommand) => {
      if (inFlight.current) return;
      const text = review.text;
      inFlight.current = true;
      onEvent({ type: "submit" });
      const call =
        command === "copy"
          ? api.dictationCopy(text)
          : command === "confirm"
            ? api.dictationConfirm(text)
            : api.dictationDismiss(text);
      call
        .then(() => {
          onEvent({ type: "settled" });
          // A copy leaves the sheet up: put the caret back so the next
          // keystroke belongs to the words, not to the button that just took
          // focus.
          if (command === "copy") body.current?.focus();
        })
        .catch((reason: unknown) =>
          onEvent({ type: "failed", message: failureText(reason, FALLBACK[command]) }),
        )
        .finally(() => {
          inFlight.current = false;
        });
    },
    [review.text, onEvent],
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const onButton =
        event.target instanceof HTMLElement && event.target.closest("button") !== null;
      // A focused button answers Enter itself; the sheet only claims what
      // nothing else will. Ctrl+C is the sheet's wherever focus sits.
      if (onButton && event.key === "Enter") return;
      const command = reviewCommand(event);
      if (!command) return;
      // Ctrl+C would otherwise copy the selection; the sheet's copy is the
      // whole body.
      event.preventDefault();
      run(command);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [run]);

  const busy = review.status === "working";

  return (
    <div
      className="overlay-sheet"
      role="dialog"
      aria-modal="true"
      aria-labelledby="overlay-sheet-title"
    >
      <header className="overlay-sheet-head">
        <div
          className="overlay-pager"
          role="group"
          aria-label={`Revisions, ${review.page + 1} of ${review.pages.length}`}
        >
          <button type="button" disabled aria-label="Previous revision">‹</button>
          <span className="overlay-pager-count" aria-hidden="true">
            {review.page + 1} / {review.pages.length}
          </span>
          <button type="button" disabled aria-label="Next revision">›</button>
        </div>
        <h2 id="overlay-sheet-title" className="overlay-sheet-title">Review</h2>
        <span className="overlay-sheet-kind">Dictation</span>
        <button
          type="button"
          className="icon-btn sheet-head-close"
          aria-label="Dismiss review"
          disabled={busy}
          onClick={() => run("dismiss")}
        >
          <X aria-hidden="true" />
        </button>
      </header>

      <p className="overlay-sheet-context">Esc keeps it on the clipboard · Confirm pastes</p>

      <textarea
        ref={body}
        className="overlay-sheet-body"
        aria-label="Transcript"
        value={review.text}
        onChange={(event) => onEvent({ type: "edit", text: event.target.value })}
      />

      {review.error ? (
        <p className="overlay-sheet-error" role="alert">
          <AlertCircle size={13} strokeWidth={2} aria-hidden="true" />
          {review.error}
        </p>
      ) : null}

      <footer className="overlay-sheet-foot">
        {REVIEW_BINDINGS.map((binding) => (
          <Button
            key={binding.action}
            variant={binding.action === "confirm" ? "primary" : "default"}
            disabled={busy}
            onClick={() => run(binding.action)}
          >
            {binding.label}
            <kbd aria-hidden="true">{binding.keycap}</kbd>
          </Button>
        ))}
      </footer>
    </div>
  );
}
