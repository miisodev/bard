/*
  The shelf's pure logic: what a card is, which cards a view shows, how they
  page, and what a keypress means. Every rule lives here rather than in the
  components so it can be read and tested without a DOM.
*/

import { colorValue, linkParts } from "./content";
import type { Item } from "./types";

/** A card's kind — its tint strip, its glyph and its label all follow from it. */
export type ShelfKind = "text" | "image" | "link" | "color" | "dictation" | "draw";

export const KIND_LABEL: Record<ShelfKind, string> = {
  text: "Text",
  image: "Image",
  link: "Link",
  color: "Color",
  dictation: "Dictation",
  draw: "Drawing",
};

/** The All zone's type chips, in the rail's order. */
export type TypeFilter = "all" | "text" | "image" | "link" | "color";

export const TYPE_FILTERS: { id: TypeFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "text", label: "Text" },
  { id: "image", label: "Image" },
  { id: "link", label: "Links" },
  { id: "color", label: "Colors" },
];

/**
 * What a card is. A drawing and a captured image are both `kind: "image"` —
 * the drawing is the one the person made (manual origin), so it takes the
 * pen's tint and glyph rather than the clipboard's.
 */
export function itemKind(item: Item): ShelfKind {
  if (item.kind === "image") return item.origin === "manual" ? "draw" : "image";
  if (item.origin === "dictation") return "dictation";
  if (colorValue(item.content)) return "color";
  if (linkParts(item.content)) return "link";
  return "text";
}

/**
 * The chips are buckets: Text owns dictated notes, Image owns drawings, and
 * a link or a colour is never also plain text, so every item answers exactly
 * one chip.
 */
export function matchesFilter(kind: ShelfKind, filter: TypeFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "text":
      return kind === "text" || kind === "dictation";
    case "image":
      return kind === "image" || kind === "draw";
    case "link":
      return kind === "link";
    case "color":
      return kind === "color";
  }
}

/**
 * The cards a view shows: the query, the active chip and the favourite view
 * applied together, in the order the backend sent (pins first, then its sort).
 */
export function filterItems(
  items: Item[],
  query: string,
  filter: TypeFilter,
  favoritesOnly: boolean,
): Item[] {
  const q = query.trim().toLowerCase();
  return items.filter((item) => {
    if (favoritesOnly && item.favorite === 0) return false;
    if (!matchesFilter(itemKind(item), filter)) return false;
    if (!q) return true;
    return item.content.toLowerCase().includes(q);
  });
}

/** What one pass over the list produces: the page's cards and every chip's count. */
export interface ShelfView {
  visible: Item[];
  counts: Record<TypeFilter, number>;
}

/**
 * [`filterItems`] and every chip's count, in one pass over the list.
 *
 * The shelf ran `filterItems` six times per change — once for the visible list
 * and once per chip — and each pass re-classified every item (`itemKind` runs
 * two regular expressions) and re-lowercased its content when a query was
 * present. That is six classifications and six lowercases per item per
 * keystroke, and the content can be 256 kB: the work grew with the library
 * while the screen showed eight cards.
 *
 * One pass classifies and lowercases once per item, and both answers fall out
 * of it: an item is counted under every chip it matches and collected only
 * under the active one.
 */
export function filterShelf(
  items: Item[],
  query: string,
  filter: TypeFilter,
  favoritesOnly: boolean,
): ShelfView {
  const q = query.trim().toLowerCase();
  const counts: Record<TypeFilter, number> = { all: 0, text: 0, image: 0, link: 0, color: 0 };
  const visible: Item[] = [];
  for (const item of items) {
    if (favoritesOnly && item.favorite === 0) continue;
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
 * The shelf's list with one just-captured item in it, exactly where a backend
 * read would have put it.
 *
 * Only correct for the newest-first order, which is what the caller checks
 * before using it: in the other three the item's place comes from `sort_items`
 * in Rust, and a guess there would put a card somewhere the next read moves it.
 * Pins lead every order, so a capture — never pinned — goes after the last of
 * them, and the rest of the list is already newest-first.
 */
export function insertCaptured(items: Item[], item: Item): Item[] {
  const rest = items.filter((existing) => existing.id !== item.id);
  const firstUnpinned = rest.findIndex((existing) => existing.pinnedAt === null);
  const at = firstUnpinned < 0 ? rest.length : firstUnpinned;
  return [...rest.slice(0, at), item, ...rest.slice(at)];
}

/** Long enough to identify an item, short enough to be read out loud. */
const PREVIEW_CHARS = 48;

/**
 * What an item reads as on one line: its text, or what it is when it has none.
 * Nine rows all reading "Copy text" is nine identical controls; the preview is
 * what a control acts on, so it belongs in the name.
 */
export function itemPreview(item: Item, max = PREVIEW_CHARS): string {
  const text = item.content.replace(/\s+/g, " ").trim();
  if (!text) return item.kind === "image" ? "image" : "empty note";
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

/** The one sentence a refused pack paste gets, said the same way everywhere. */
export const PACK_NO_TEXT = "This pack has no text to paste.";

/**
 * What pasting a pack writes: the text-bearing members in pack order, one per
 * line — notes, dictations, links and colours. Images and drawings carry no
 * text to paste, so they are skipped, and so is a note that has edited down to
 * nothing. An empty string means there is no text in the pack; that is what
 * disables the paste and carries {@link PACK_NO_TEXT} as the reason.
 */
export function packText(items: Item[]): string {
  return items
    .filter((item) => item.kind !== "image")
    .map((item) => item.content.trim())
    .filter((text) => text !== "")
    .join("\n");
}

/** What the Packs zone paints, when its list is not (yet) a list. */
export type PacksZoneView = "cards" | "error" | "none" | "empty";

/**
 * The zone's state, read off the last read: cards whenever there are any, a
 * failure over an empty list (a shelf with no packs and a shelf whose packs
 * could not be read are different screens), and nothing at all until the
 * first read lands rather than claiming there are none.
 */
export function packsZoneView(
  cardCount: number,
  loaded: boolean,
  error: string | null,
): PacksZoneView {
  if (cardCount > 0) return "cards";
  if (error !== null) return "error";
  return loaded ? "empty" : "none";
}

/**
 * The one pack that holds the zone's tab stop: the cursor's pack while it is
 * on this page, the first card otherwise. A list whose tab stop is missing
 * cannot be tabbed into at all.
 */
export function rovingPack(cursor: number | null, pageIds: number[]): number | null {
  if (cursor !== null && pageIds.includes(cursor)) return cursor;
  return pageIds[0] ?? null;
}

/** How many cards fit a zone, given one card's size and the gap after it. */
export function pageSize(fit: number, card: number, gap: number): number {
  if (!(card > 0) || !Number.isFinite(fit)) return 1;
  return Math.max(1, Math.floor((fit + gap) / (card + gap)));
}

export function pageCount(total: number, perPage: number): number {
  return Math.max(1, Math.ceil(total / Math.max(1, perPage)));
}

export function clampPage(page: number, total: number, perPage: number): number {
  return Math.min(Math.max(0, page), pageCount(total, perPage) - 1);
}

/** One page of cards. Only the current page ever renders. */
export function pageItems<T>(items: T[], page: number, perPage: number): T[] {
  const start = Math.max(0, page) * perPage;
  return items.slice(start, start + perPage);
}

export type PageNumber = number | "gap";

/**
 * Page numbers with the middle elided: `1 2 3 … 9`. The first and last page
 * are always shown — they are how you reach either end — and the current page
 * keeps its neighbours so it reads as a position rather than a lone number.
 */
export function pageNumbers(current: number, total: number): PageNumber[] {
  if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1);
  const last = total;
  const from = Math.max(2, current - 1);
  const to = Math.min(last - 1, current + 2);
  const out: PageNumber[] = [1];
  if (from > 2) out.push("gap");
  for (let n = from; n <= to; n += 1) out.push(n);
  if (to < last - 1) out.push("gap");
  out.push(last);
  return out;
}

/**
 * Where the cursor lands after one move through the whole filtered order.
 * The page follows the cursor, so an edge of one page is never a dead end.
 */
export function nextIndex(current: number, delta: -1 | 1, total: number): number {
  if (total <= 0) return -1;
  if (current < 0) return delta > 0 ? 0 : total - 1;
  return Math.min(total - 1, Math.max(0, current + delta));
}

export type EscapeOutcome = "search" | "composer" | "selection" | "retract";

/**
 * The fixed Escape order: the search first (whatever has focus — a query you
 * cannot clear is the defect this encodes), then a draft in the focused
 * composer, then the selection, and only then the shelf itself.
 */
export function escapeOutcome(state: {
  query: string;
  composer: string;
  composerFocused: boolean;
  selectionSize: number;
}): EscapeOutcome {
  if (state.query !== "") return "search";
  if (state.composerFocused && state.composer !== "") return "composer";
  if (state.selectionSize > 0) return "selection";
  return "retract";
}

export type ShelfCommand =
  | { type: "move"; delta: -1 | 1; extend: boolean }
  | { type: "page"; delta: -1 | 1 }
  | { type: "paste" }
  | { type: "preview" }
  | { type: "pasteNth"; index: number }
  | { type: "copy" }
  | { type: "copyMarkdown" }
  | { type: "pin" }
  | { type: "delete" }
  | { type: "toggleDone" }
  | { type: "filters" }
  | { type: "shortcuts" }
  | { type: "escape" };

export interface KeyShape {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/**
 * A keypress as a shelf command, or null when the key is not the shelf's.
 * Enter, Space and the like are read as the shelf's own commands only after
 * the caller has ruled out a control having claimed them.
 */
export function shelfCommand(event: KeyShape): ShelfCommand | null {
  if (event.ctrlKey || event.metaKey) {
    const key = event.key.toLowerCase();
    if (key === "k") return { type: "filters" };
    if (key === "/") return { type: "shortcuts" };
    if (key === "c") return { type: "copy" };
    if (/^[1-9]$/.test(key) && !event.shiftKey && !event.altKey) {
      return { type: "pasteNth", index: Number(key) - 1 };
    }
    if (key === "p" && !event.shiftKey && !event.altKey) return { type: "pin" };
    if (key === "m" && event.shiftKey) return { type: "copyMarkdown" };
    return null;
  }
  switch (event.key) {
    case "ArrowLeft":
    case "ArrowUp":
      return { type: "move", delta: -1, extend: event.shiftKey };
    case "ArrowRight":
    case "ArrowDown":
      return { type: "move", delta: 1, extend: event.shiftKey };
    case "PageUp":
      return { type: "page", delta: -1 };
    case "PageDown":
      return { type: "page", delta: 1 };
    case "Enter":
      return { type: event.altKey ? "preview" : "paste" };
    case "Delete":
    case "Backspace":
      return { type: "delete" };
    case " ":
      return { type: "toggleDone" };
    case "Escape":
      return { type: "escape" };
    default:
      return null;
  }
}

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const RGB = /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/i;
const HSL = /^hsla?\(\s*([\d.]+)(?:deg)?[,\s]+([\d.]+)%[,\s]+([\d.]+)%/i;

function hslToRgb(h: number, s: number, l: number): Rgb {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let base: [number, number, number];
  if (h < 60) base = [c, x, 0];
  else if (h < 120) base = [x, c, 0];
  else if (h < 180) base = [0, c, x];
  else if (h < 240) base = [0, x, c];
  else if (h < 300) base = [x, 0, c];
  else base = [c, 0, x];
  return {
    r: Math.round((base[0] + m) * 255),
    g: Math.round((base[1] + m) * 255),
    b: Math.round((base[2] + m) * 255),
  };
}

/**
 * The channels behind a captured CSS colour — a hex, `rgb()` or `hsl()` — or
 * null for anything else (a bare `oklch()` value is still a colour item, it
 * just has no channels here to name or convert).
 */
export function colorChannels(value: string): Rgb | null {
  const text = value.trim();
  const hex = HEX.exec(text);
  if (hex) {
    const digits = hex[1];
    const full =
      digits.length === 3 || digits.length === 4
        ? digits
            .slice(0, 3)
            .split("")
            .map((d) => d + d)
            .join("")
        : digits.slice(0, 6);
    const n = parseInt(full, 16);
    return { r: (n >> 16) & 0xff, g: (n >> 8) & 0xff, b: n & 0xff };
  }
  const rgb = RGB.exec(text);
  if (rgb) {
    const clamp = (part: string) => Math.min(255, Math.max(0, Math.round(Number(part))));
    const channels = [clamp(rgb[1]), clamp(rgb[2]), clamp(rgb[3])];
    if (channels.some((channel) => !Number.isFinite(channel))) return null;
    return { r: channels[0], g: channels[1], b: channels[2] };
  }
  const hsl = HSL.exec(text);
  if (hsl) {
    const h = ((Number(hsl[1]) % 360) + 360) % 360;
    const s = Math.min(100, Math.max(0, Number(hsl[2]))) / 100;
    const l = Math.min(100, Math.max(0, Number(hsl[3]))) / 100;
    if (![h, s, l].every(Number.isFinite)) return null;
    return hslToRgb(h, s, l);
  }
  return null;
}

export function toHex({ r, g, b }: Rgb): string {
  return `#${[r, g, b].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

export function toRgbText({ r, g, b }: Rgb): string {
  return `rgb(${r}, ${g}, ${b})`;
}

export function toHsl({ r, g, b }: Rgb): { h: number; s: number; l: number } {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 0, s: 0, l: Math.round(l * 100) };
  const s = d / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === rn) h = 60 * (((gn - bn) / d) % 6);
  else if (max === gn) h = 60 * ((bn - rn) / d + 2);
  else h = 60 * ((rn - gn) / d + 4);
  if (h < 0) h += 360;
  return { h: Math.round(h), s: Math.round(s * 100), l: Math.round(l * 100) };
}

export function toHslText(rgb: Rgb): string {
  const { h, s, l } = toHsl(rgb);
  return `hsl(${h}, ${s}%, ${l}%)`;
}
