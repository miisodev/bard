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
 * A lowercased copy of an item's content, kept beside the item itself.
 *
 * The list is searched on every keystroke and the history can hold five
 * thousand rows of long text; lowercasing all of it each time is a megabyte
 * of allocation per character typed. The copy is cached against the item, so
 * it is made once per row and thrown away when the row is.
 */
const FOLDED = new WeakMap<Item, string>();

function folded(item: Item): string {
  const cached = FOLDED.get(item);
  if (cached !== undefined) return cached;
  const next = item.content.toLowerCase();
  FOLDED.set(item, next);
  return next;
}

/**
 * A subsequence match with a score, the way a command palette ranks.
 *
 * A clipboard history is full of things whose spelling the person only
 * half-remembers: a registry name, a command, a file path. A substring
 * search misses those entirely — "ghcr" finds nothing in "GitHub Container
 * Registry" — and a subsequence match finds them, with a score that puts the
 * tightest one first.
 *
 * A direct substring always outscores a scattered subsequence, so the results
 * a person already expects keep their places and the fuzzy ones arrive after
 * them.
 *
 * Returns -1 when the query is not a subsequence of the text at all.
 */
export function fuzzyScore(query: string, text: string): number {
  if (!query) return 0;
  const needle = query.toLowerCase();
  const lower = text.toLowerCase();

  // A substring is worth more than any scattered subsequence: it is what the
  // person meant, and the earlier it starts the better it is.
  const direct = lower.indexOf(needle);
  if (direct === 0) return 10_000;
  if (direct > 0) return 8_000 - Math.min(direct, 500);

  let score = 0;
  let cursor = 0;
  let previous = -2;
  for (let index = 0; index < needle.length; index++) {
    const wanted = needle[index];
    let found = -1;
    while (cursor < lower.length) {
      if (lower[cursor] === wanted) {
        found = cursor;
        break;
      }
      cursor++;
    }
    if (found < 0) return -1;
    if (found === 0) score += 24;
    else if (isBoundary(text[found - 1])) score += 14;
    if (found === previous + 1) score += 12;
    // The gap between two matched characters costs, and a long one costs more.
    score -= Math.min(found - previous - 1, 16);
    score += 16;
    previous = found;
    cursor = found + 1;
  }
  // A match that reaches deep into the text scores lower than one that stays
  // near the front of it.
  score -= Math.min(previous, 40);
  return score;
}

/** True where a word starts: after a space, a separator or a punctuation mark. */
function isBoundary(character: string): boolean {
  if (!character) return true;
  if (character === " " || character === "/" || character === "\\") return true;
  if (character === "-" || character === "_" || character === ".") return true;
  if (character === ":" || character === "," || character === ";") return true;
  if (character === "(" || character === "[" || character === "{") return true;
  return false;
}

/**
 * The visible items and every filter's count, in one pass: each item is
 * classified and lowercased once, however many filters there are.
 *
 * A substring match is what the person typed, and it wins outright. The
 * subsequence match is the *fallback*, used only when nothing matches as
 * typed — so "ghcr" reaches "GitHub Container Registry" without "venue"
 * also reaching "Ship the V2 release notes". Precision first, recall second.
 */
export function filterShelf(items: Item[], query: string, filter: TypeFilter): ShelfView {
  const needle = query.trim().toLowerCase();
  const counts: Record<TypeFilter, number> = { all: 0, text: 0, link: 0, image: 0, file: 0 };
  const direct: Item[] = [];
  const fuzzy: { item: Item; score: number }[] = [];

  for (const item of items) {
    if (!needle) {
      direct.push(item);
      continue;
    }
    if (folded(item).includes(needle)) {
      direct.push(item);
      continue;
    }
    const score = fuzzyScore(needle, item.content);
    if (score >= 0) fuzzy.push({ item, score });
  }

  let matched = direct;
  if (needle && direct.length === 0 && fuzzy.length > 0) {
    fuzzy.sort((a, b) => b.score - a.score || b.item.id - a.item.id);
    matched = fuzzy.map((entry) => entry.item);
  }

  const visible: Item[] = [];
  for (const item of matched) {
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

/**
 * The same moment in words: "just now", "34 minutes ago", "5 weeks ago".
 *
 * The compact form on a row is deliberately terse, and "34m" is ambiguous to
 * anyone who reads "m" as months. The tooltip and the screen-reader text
 * carry the truth; the row itself stays quiet.
 */
export function agoLong(timestamp: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - timestamp) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  if (days < 14) return `${days} day${days === 1 ? "" : "s"} ago`;
  const weeks = Math.round(days / 7);
  return `${weeks} week${weeks === 1 ? "" : "s"} ago`;
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
  | { type: "menu" }
  | { type: "edit" }
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
  // F2 opens the row for editing, the way it does in a file list and in a
  // spreadsheet. It is a navigation key, so it works from the search field
  // too — the person types, then presses F2 to fix what they found.
  if (event.key === "F2") return { type: "edit" };
  // The context-menu key and its keyboard equivalent reach the row's menu.
  // It is how a person who cannot use a pointer reaches every action on a
  // row: the pointer buttons are decoration, the menu is the real one.
  if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
    return { type: "menu" };
  }
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
