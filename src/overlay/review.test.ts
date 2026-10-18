import { describe, expect, it } from "vitest";
import {
  REVIEW_BINDINGS,
  reviewCommand,
  reviewPages,
  reviewReduce,
  type ReviewAction,
  type ReviewKeyLike,
} from "./review";

/** The stroke each badge names, as the foot draws it. */
const STROKE: Record<ReviewAction, ReviewKeyLike> = {
  copy: { key: "c", ctrlKey: true, metaKey: false, shiftKey: false },
  confirm: { key: "Enter", ctrlKey: false, metaKey: false, shiftKey: false },
};

describe("reviewCommand", () => {
  it("answers the gesture its badge names", () => {
    for (const binding of REVIEW_BINDINGS) {
      expect(reviewCommand(STROKE[binding.action])).toBe(binding.action);
    }
  });

  it("leaves Shift+Enter to the textarea and Escape to Rust", () => {
    expect(reviewCommand({ ...STROKE.confirm, shiftKey: true })).toBeNull();
    expect(reviewCommand({ key: "Escape", ctrlKey: false, metaKey: false, shiftKey: false })).toBeNull();
    expect(reviewCommand({ key: "c", ctrlKey: false, metaKey: false, shiftKey: false })).toBeNull();
    expect(reviewCommand({ key: "a", ctrlKey: true, metaKey: false, shiftKey: false })).toBeNull();
  });

  it("never confirms over a composing IME", () => {
    expect(reviewCommand({ ...STROKE.confirm, isComposing: true })).toBeNull();
  });

  it("reads the Meta chord too", () => {
    expect(reviewCommand({ key: "C", ctrlKey: false, metaKey: true, shiftKey: false })).toBe("copy");
  });
});

describe("reviewPages", () => {
  it("opens an untouched take as one page of one", () => {
    expect(reviewPages("Ship it")).toEqual(["Ship it"]);
    const state = reviewReduce(null, { type: "open", text: "Ship it" });
    expect(state).toEqual({
      pages: ["Ship it"],
      page: 0,
      text: "Ship it",
      status: "ready",
      error: null,
    });
  });

  it("refuses a take with nothing to review", () => {
    expect(reviewReduce(null, { type: "open", text: "   " })).toBeNull();
  });
});

describe("reviewReduce", () => {
  const open = reviewReduce(null, { type: "open", text: "Ship it" })!;

  it("keeps edits in the body and on the page", () => {
    const edited = reviewReduce(open, { type: "edit", text: "Ship it Friday" })!;
    expect(edited.text).toBe("Ship it Friday");
    expect(edited.pages).toEqual(["Ship it Friday"]);
  });

  it("works a command once", () => {
    const working = reviewReduce(open, { type: "submit" })!;
    expect(working.status).toBe("working");
    // A second submit — a held Enter, a double click — is not a second
    // command; it is the same state, reference and all.
    expect(reviewReduce(working, { type: "submit" })).toBe(working);
  });

  it("shows a refusal under the body and stays open, retryable", () => {
    const failed = reviewReduce(reviewReduce(open, { type: "submit" }), {
      type: "failed",
      message: "Bard couldn't reach the app it started in.",
    })!;
    expect(failed.status).toBe("ready");
    expect(failed.error).toBe("Bard couldn't reach the app it started in.");
    expect(failed.text).toBe("Ship it");
    // The retry clears the message while the attempt is in flight.
    const retry = reviewReduce(failed, { type: "submit" })!;
    expect(retry.error).toBeNull();
    expect(retry.status).toBe("working");
  });

  it("settles back to ready", () => {
    const working = reviewReduce(open, { type: "submit" })!;
    expect(reviewReduce(working, { type: "settled" })!.status).toBe("ready");
  });

  it("closes to nothing from anywhere", () => {
    expect(reviewReduce(open, { type: "close" })).toBeNull();
    expect(reviewReduce(null, { type: "close" })).toBeNull();
    expect(reviewReduce(null, { type: "edit", text: "x" })).toBeNull();
    expect(reviewReduce(null, { type: "settled" })).toBeNull();
  });
});
