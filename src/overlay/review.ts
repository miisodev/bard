/*
  The review sheet's pure logic: what a keypress means, how a take pages into
  revisions, and the states a command moves through. It lives here rather than
  in the component so all of it can be read and tested without a DOM.

  Rust owns the window and the Escape key: `dictation-review` opens the sheet,
  `dictation-cancelled` closes it, and every command carries the body as it
  stands — the edits travel with the action.
*/

/** What the foot's buttons, and the keys that mirror them, ask of a take. */
export type ReviewAction = "copy" | "confirm";

/** What the sheet's controls can ask, Rust's own dismiss included. */
export type ReviewCommand = ReviewAction | "dismiss";

export interface ReviewKeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  /** True while an IME is composing: a half-typed word must not confirm. */
  isComposing?: boolean;
}

/**
 * The foot's bindings. The badge and the handler read the same entry, so the
 * keycap a person sees is the key the sheet answers.
 */
export const REVIEW_BINDINGS: { action: ReviewAction; label: string; keycap: string }[] = [
  { action: "copy", label: "Copy", keycap: "⌃C" },
  { action: "confirm", label: "Confirm", keycap: "⏎" },
];

/**
 * Enter confirms and Ctrl+C copies the whole body; Shift+Enter stays the
 * textarea's own newline. Escape is deliberately absent — Rust catches it
 * globally and dismisses the take.
 */
export function reviewCommand(event: ReviewKeyLike): ReviewAction | null {
  if (event.isComposing) return null;
  if (event.key === "Enter") return event.shiftKey ? null : "confirm";
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "c") return "copy";
  return null;
}

/** The pages of a take under review. One today; revisions are future work. */
export function reviewPages(text: string): string[] {
  return [text];
}

export type ReviewStatus = "ready" | "working";

export interface ReviewState {
  /** Each page is one revision of the take, in order. */
  pages: string[];
  /** The page on screen. */
  page: number;
  /** That page's body, edits and all. */
  text: string;
  status: ReviewStatus;
  /** What the last command was refused with, shown under the body. */
  error: string | null;
}

export type ReviewEvent =
  | { type: "open"; text: string }
  | { type: "edit"; text: string }
  | { type: "submit" }
  | { type: "failed"; message: string }
  | { type: "settled" }
  | { type: "close" };

export function reviewReduce(state: ReviewState | null, event: ReviewEvent): ReviewState | null {
  switch (event.type) {
    case "open":
      // There is no review for an empty take; Rust does not open one, and a
      // sheet with nothing to read would be a blank window.
      if (!event.text.trim()) return state;
      return {
        pages: reviewPages(event.text),
        page: 0,
        text: event.text,
        status: "ready",
        error: null,
      };
    case "edit": {
      if (!state) return state;
      const pages = [...state.pages];
      pages[state.page] = event.text;
      return { ...state, pages, text: event.text };
    }
    case "submit":
      // A held Enter or a double click is not a second command: the in-flight
      // one owns the take until it settles.
      if (!state || state.status === "working") return state;
      return { ...state, status: "working", error: null };
    case "failed":
      if (!state) return state;
      return { ...state, status: "ready", error: event.message };
    case "settled":
      if (!state) return state;
      return { ...state, status: "ready", error: null };
    case "close":
      return null;
  }
}
