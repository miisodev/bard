export interface NoteEdit {
  text: string;
  phase: "editing" | "saving" | "saved";
  error: string | null;
}

type EditEvent =
  | { type: "change" | "reset"; text: string }
  | { type: "save"; original: string }
  | { type: "failed"; message: string }
  | { type: "saved" };

/** A draft remains owned by the editor until its write succeeds. */
export function editNote(state: NoteEdit, event: EditEvent): NoteEdit {
  switch (event.type) {
    case "reset": return { text: event.text, phase: "editing", error: null };
    case "change": return state.phase === "saving" ? state : { text: event.text, phase: "editing", error: null };
    case "save":
      if (state.phase === "saving") return state;
      if (!state.text.trim()) return { ...state, error: "Add some text, or cancel to keep the original note." };
      return { ...state, phase: state.text.trim() === event.original ? "saved" : "saving", error: null };
    case "failed": return { ...state, phase: "editing", error: event.message };
    case "saved": return { ...state, phase: "saved", error: null };
  }
}
