import { describe, expect, it } from "vitest";
import { editNote, type NoteEdit } from "./noteEdit";

const draft: NoteEdit = { text: "A revised note", phase: "editing", error: null };

describe("note editing", () => {
  it("keeps the draft editable after a failed save", () => {
    const saving = editNote(draft, { type: "save", original: "Original" });
    expect(saving.phase).toBe("saving");
    const failed = editNote(saving, { type: "failed", message: "Disk full. Try again." });
    expect(failed).toEqual({ text: "A revised note", phase: "editing", error: "Disk full. Try again." });
    expect(editNote(failed, { type: "save", original: "Original" }).phase).toBe("saving");
  });
  it("does not turn an empty edit into deletion", () => {
    const next = editNote({ ...draft, text: "  \n " }, { type: "save", original: "Original" });
    expect(next.phase).toBe("editing");
    expect(next.error).toBeTruthy();
    expect(next.text).toBe("  \n ");
  });
  it("protects the submitted text while saving", () => {
    const saving = editNote(draft, { type: "save", original: "Original" });
    expect(editNote(saving, { type: "change", text: "New text" })).toEqual(saving);
    expect(editNote(saving, { type: "save", original: "Original" })).toEqual(saving);
  });
  it("does not write an unchanged note", () => {
    expect(editNote(draft, { type: "save", original: "A revised note" }).phase).toBe("saved");
  });
});
