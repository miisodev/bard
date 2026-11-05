import { useEffect, useReducer, useRef } from "react";
import { editNote, type NoteEdit } from "../noteEdit";
import { api } from "../store";
import type { Item } from "../types";

/** Navigation cannot unmount a draft while a person is deciding what to keep. */
export default function NoteEditor({ item, onClose, onSaved }: {
  item: Item;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [edit, dispatch] = useReducer(editNote, { text: item.content, phase: "editing", error: null } satisfies NoteEdit);
  const dialog = useRef<HTMLDialogElement>(null);
  const editor = useRef<HTMLTextAreaElement>(null);
  const saving = useRef(false);

  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    const frame = requestAnimationFrame(() => editor.current?.focus());
    return () => { cancelAnimationFrame(frame); element?.close(); };
  }, []);

  const save = async () => {
    if (saving.current) return;
    const next = editNote(edit, { type: "save", original: item.content });
    dispatch({ type: "save", original: item.content });
    if (next.phase === "saved") { onClose(); return; }
    if (next.phase !== "saving") { editor.current?.focus(); return; }
    saving.current = true;
    try {
      await api.updateItem(item.id, next.text.trim());
      dispatch({ type: "saved" });
      onSaved();
      onClose();
    } catch (reason) {
      dispatch({ type: "failed", message: reason instanceof Error ? reason.message : String(reason || "Couldn’t save. Your changes are still here. Try again.") });
      requestAnimationFrame(() => editor.current?.focus());
    } finally { saving.current = false; }
  };

  return (
    <dialog ref={dialog} className="item-preview note-edit-dialog" aria-labelledby="note-edit-title"
      onCancel={(event) => { event.preventDefault(); if (!saving.current) onClose(); }}>
      <header className="item-preview-head"><h2 id="note-edit-title">Edit note</h2></header>
      <textarea ref={editor} className="inline-edit" aria-label="Edit note" value={edit.text}
        readOnly={edit.phase === "saving"} aria-invalid={!!edit.error}
        aria-describedby={`note-edit-help${edit.error ? " note-edit-error" : ""}`}
        onChange={(event) => dispatch({ type: "change", text: event.target.value })}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void save(); }
        }} />
      {edit.error ? <p id="note-edit-error" className="note-edit-error" role="alert">{edit.error}</p> : null}
      <footer className="item-preview-foot">
        <span id="note-edit-help" className="card-meta">Shift+Enter for a new line</span>
        <div className="note-edit-buttons">
          <button type="button" className="btn" disabled={edit.phase === "saving"} onClick={onClose}>Cancel</button>
          <button type="button" className="btn primary" disabled={edit.phase === "saving"} aria-busy={edit.phase === "saving"} onClick={() => void save()}>{edit.phase === "saving" ? "Saving…" : "Save"}</button>
        </div>
      </footer>
    </dialog>
  );
}
