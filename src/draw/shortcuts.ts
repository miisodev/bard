import type { ShortcutGroup } from "../components/ShortcutSheet";

/**
 * Draw's keys, stated once and taught in both places that list them: Draw's
 * own `?` sheet and the panel shortcut sheet's Draw group.
 *
 * The note is the honest half. Draw is a focused window inside Bard, and the
 * app's global gestures keep listening: Ctrl ×2 starts a take and Left Shift ×2
 * captures the selection while the pointer is on the page. The sheet says so
 * rather than implying Draw has taken the keyboard for itself.
 */
export const DRAW_FRICTION_NOTE =
  "While Draw is focused, Ctrl ×2 still starts a take and Left Shift ×2 still captures the selection — those gestures are the app's, not Draw's.";

export const DRAW_SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    name: "Tools",
    rows: [
      { keys: ["V", "1"], what: "Select" },
      { keys: ["P", "2"], what: "Pen" },
      { keys: ["L", "3"], what: "Line" },
      { keys: ["A", "4"], what: "Arrow" },
      { keys: ["R", "5"], what: "Rectangle" },
      { keys: ["O", "6"], what: "Ellipse" },
      { keys: ["D", "7"], what: "Diamond" },
      { keys: ["T", "8"], what: "Text" },
      { keys: ["E", "9"], what: "Eraser" },
    ],
  },
  {
    name: "Edit",
    rows: [
      { keys: ["Ctrl", "Z"], what: "Undo" },
      { keys: ["Ctrl", "Shift", "Z"], what: "Redo" },
      { keys: ["Ctrl", "Y"], what: "Redo" },
      { keys: ["Ctrl", "D"], what: "Duplicate the selection" },
      { keys: ["Alt", "drag"], what: "Drag a duplicate, leaving the original" },
      { keys: ["Delete"], what: "Delete the selection (Backspace works too)" },
      { keys: ["Shift"], what: "While drawing: 15° steps, a square, a circle" },
      { keys: ["Shift"], what: "While resizing: keep the shape's proportions" },
      { keys: ["Alt"], what: "While resizing: grow from the centre" },
    ],
  },
  {
    name: "Save",
    rows: [
      { keys: ["Enter"], what: "Send — keep it and paste it" },
      { keys: ["Ctrl", "Enter"], what: "Send" },
      { keys: ["Ctrl", "S"], what: "Keep it in Bard" },
      { keys: ["Ctrl", "C"], what: "Copy it to the clipboard" },
    ],
  },
  {
    name: "Window",
    rows: [
      { keys: ["F11"], what: "Fullscreen" },
      { keys: ["?"], what: "These shortcuts" },
      {
        keys: ["Esc"],
        what: "One step back: the page menu, a prompt, the selection, the tool, fullscreen, then close",
      },
      { keys: ["Ctrl", "V"], what: "Paste a picture from the clipboard" },
    ],
    note: DRAW_FRICTION_NOTE,
  },
];
