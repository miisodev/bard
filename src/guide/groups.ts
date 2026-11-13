import type { ShortcutState } from "../types";

/** One block of the guide: a named group of key rows, with an optional caveat. */
export interface GuideGroup {
  name: string;
  rows: { keys: string[]; what: string }[];
  /** A sentence the key rows cannot carry. */
  note?: string;
}

/**
 * Everything Bard answers to, in the words the rest of the app uses. The
 * global rows show the keys as they are bound now — the person's own choice,
 * or what their desktop assigned — never the defaults from memory.
 */
export function guideGroups(state: ShortcutState | null): GuideGroup[] {
  const rows = state?.rows ?? [];
  const portal = state?.backend === "portal";
  // On Wayland a chord exists only once the desktop has granted it. Until
  // then the guide must not list keys that do nothing.
  const unbound = portal && rows.length > 0 && rows.every((r) => r.status === "pending" || r.status === "off");
  const keys = (id: string) => {
    const row = rows.find((r) => r.id === id);
    if (!row || !row.display) return ["Off"];
    if (portal && row.status === "pending") return ["Not set"];
    return row.display.split("+");
  };
  const row = (id: string, what: string) => ({ keys: keys(id), what });
  return [
    {
      name: "Anywhere",
      rows: [
        row("main", "Show or hide Bard"),
        row("dictate", "Dictate: hold, or tap twice"),
        row("slot-1", "Copy slot 1 (2 and 3 likewise)"),
        row("cycle", "Cycle through the slots"),
        row("preview", "Show the clipboard"),
        row("pause", "Pause or resume keeping copies"),
        row("draw", "Open Draw"),
        row("settings", "Open Settings"),
        row("guide", "Show or hide this guide"),
      ],
      note: unbound
        ? "Your desktop has not been asked for these yet. Connect them in Settings › Shortcuts."
        : portal
          ? "Your desktop owns these shortcuts. Change them in Settings › Shortcuts."
          : "Escape cancels a dictation while it is listening. Change any shortcut in Settings › Shortcuts.",
    },
    {
      name: "In the main window",
      rows: [
        { keys: ["↑", "↓"], what: "Move" },
        { keys: ["Enter"], what: "Copy and close" },
        { keys: ["Shift", "Enter"], what: "Copy and stay" },
        { keys: ["Ctrl", "P"], what: "Pin or unpin" },
        { keys: ["Ctrl", "S"], what: "Keep forever" },
        { keys: ["Delete"], what: "Delete" },
        { keys: ["Ctrl", "F"], what: "Search" },
        { keys: ["Esc"], what: "Clear search, then close" },
      ],
    },
  ];
}
