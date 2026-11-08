/*
  The clipboard list's pure logic: what an item is, which items a view shows,
  how a row reads and what a keypress means. Every rule lives here rather than
  in the components so it can be read and tested without a DOM.
*/

import { colorValue, linkParts } from "./content";
import type { Item } from "./types";

/** An item's kind — its glyph, its tone and its label all follow from it. */
export type ShelfKind = "text" | "image" | "link" | "color" | "dictation" | "draw" | "file";

export const KIND_LABEL: Record<ShelfKind, string> = {
  text: "Text",
  image: "Image",
  link: "Link",
  color: "Colour",
  dictation: "Dictation",
  draw: "Drawing",
  file: "Files",
};

/** The kind filter, in the order it is shown. */
export type TypeFilter = "all" | "text" | "link" | "image" | "file";

export const TYPE_FILTERS: { id: TypeFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "text", label: "Text" },
  { id: "link", label: "Links" },
  { id: "image", label: "Images" },
  { id: "file", label: "Files" },
];

/**
 * What an item is. A drawing and a copied image are both `kind: "image"` —
 * the drawing is the one the person made (manual origin).
 */
export function itemKind(item: Item): ShelfKind {
  if (item.kind === "image") return item.origin === "manual" ? "draw" : "image";
  if (item.kind === "file") return "file";
  if (item.origin === "dictation") return "dictation";
  if (colorValue(item.content)) return "color";
  if (linkParts(item.content)) return "link";
  return "text";
}

/**
 * The filters are buckets: Text owns dictation and colours, Images owns
 * drawings, so every item answers exactly one filter besides All.
 */
export function matchesFilter(kind: ShelfKind, filter: TypeFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "text":
      return kind === "text" || kind === "dictation" || kind === "color";
    case "image":
      return kind === "image" || kind === "draw";
    case "link":
      return kind === "link";
    case "file":
      return kind === "file";
  }
}

export interface ShelfView {
  visible: Item[];
  counts: Record<TypeFilter, number>;
}

/**
 * The visible items and every filter's count, in one pass: each item is
 * classified and lowercased once, however many filters there are.
 */
export function filterShelf(items: Item[], query: string, filter: TypeFilter): ShelfView {
  const q = query.trim().toLowerCase();
  const counts: Record<TypeFilter, number> = { all: 0, text: 0, link: 0, image: 0, file: 0 };
  const visible: Item[] = [];
  for (const item of items) {
    if (q && !item.content.toLowerCase().includes(q)) continue;
    const kind = itemKind(item);
    for (const chip of TYPE_FILTERS) {
      if (matchesFilter(kind, chip.id)) counts[chip.id] += 1;
    }
    if (matchesFilter(kind, filter)) visible.push(item);
  }
  return { visible, counts };
}

/**
 * The list with one just-captured item in it, exactly where a backend read
 * would put it: after the pins, at the top of everything else.
 */
export function insertCaptured(items: Item[], item: Item): Item[] {
  const rest = items.filter((existing) => existing.id !== item.id);
  const firstUnpinned = rest.findIndex((existing) => existing.pinnedAt === null);
  const at = firstUnpinned < 0 ? rest.length : firstUnpinned;
  return [...rest.slice(0, at), item, ...rest.slice(at)];
}

const PREVIEW_CHARS = 48;

/** What an item reads as on one line, for names and announcements. */
export function itemPreview(item: Item, max = PREVIEW_CHARS): string {
  if (item.kind === "file") {
    const paths = filePaths(item);
    const first = fileName(paths[0] ?? "");
    return paths.length > 1 ? `${first} and ${paths.length - 1} more` : first || "files";
  }
  const text = item.content.replace(/\s+/g, " ").trim();
  if (!text) return item.kind === "image" ? "image" : "empty item";
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

export function filePaths(item: Item): string[] {
  return item.content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function fileFolder(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  parts.pop();
  return parts.pop() ?? "";
}

/** "now", "4m", "3h", "2d", "5w" — short enough for a row's edge. */
export function ago(timestamp: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - timestamp) / 1000));
  if (seconds < 45) return "now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.round(hours / 24);
  if (days < 14) return `${days}d`;
  return `${Math.round(days / 7)}w`;
}

/** The next index in a list of `total`, clamped (lists do not wrap). */
export function nextIndex(current: number, delta: -1 | 1, total: number): number {
  if (total <= 0) return -1;
  if (current < 0) return delta > 0 ? 0 : total - 1;
  return Math.min(total - 1, Math.max(0, current + delta));
}

/** What a key does in the list, independent of the DOM. */
export type ListCommand =
  | { type: "move"; delta: -1 | 1 }
  | { type: "edge"; to: "first" | "last" }
  | { type: "copy"; hide: boolean }
  | { type: "delete" }
  | { type: "pin" }
  | { type: "keep" }
  | { type: "search" }
  | { type: "guide" }
  | { type: "escape" };

export interface KeyShape {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  /** The search field has focus. */
  inSearch: boolean;
}

export function listCommand(event: KeyShape): ListCommand | null {
  const mod = event.ctrlKey || event.metaKey;
  if (event.key === "Escape") return { type: "escape" };
  if (event.key === "ArrowDown") return { type: "move", delta: 1 };
  if (event.key === "ArrowUp") return { type: "move", delta: -1 };
  if (event.key === "Enter") return { type: "copy", hide: !event.shiftKey };
  if (mod && event.key.toLowerCase() === "f") return { type: "search" };
  if (mod && event.key === "/") return { type: "guide" };
  if (event.inSearch) return null;
  if (event.key === "Home") return { type: "edge", to: "first" };
  if (event.key === "End") return { type: "edge", to: "last" };
  if (event.key === "Delete" || event.key === "Backspace") return { type: "delete" };
  if (mod && event.key.toLowerCase() === "p") return { type: "pin" };
  if (mod && event.key.toLowerCase() === "s") return { type: "keep" };
  if (mod && event.key.toLowerCase() === "c") return { type: "copy", hide: false };
  if (event.key === "?") return { type: "guide" };
  return null;
}
