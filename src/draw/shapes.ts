/*
  The drawing model.

  Deliberately free of React and of Tauri: it is data plus pure functions over
  that data, so the canvas can be driven imperatively at pointer speed without
  a re-render per sample, and so the whole thing can be reasoned about (and
  tested) without a window.

  Coordinates are canvas-space CSS pixels. Device-pixel scaling happens once,
  in `render`, via the context transform — nothing in this file knows about DPI.
*/

import { inkFor } from "../appearance";

export type Tool =
  | "select"
  | "pen"
  | "line"
  | "arrow"
  | "rect"
  | "ellipse"
  | "diamond"
  | "text"
  | "eraser";

/** The tools that drag between two points; the ones `constrain` understands. */
export type ConstrainKind = "line" | "arrow" | "rect" | "ellipse" | "diamond";

/**
 * The two inks: dark on a light page, light on a dark one. A shape drawn in
 * either is painted in whichever reads on the page it is rendered onto, so a
 * drawing never loses its strokes when the page changes.
 */
export const INKS = ["#1d1d1f", "#f5f5f7"];

function paintFor(color: string, ink: string | undefined): string {
  return ink && INKS.includes(color) ? ink : color;
}

export interface Point {
  x: number;
  y: number;
}

export type PenShape = {
  kind: "pen";
  id: number;
  color: string;
  width: number;
  points: Point[];
  /**
   * One ink width per point, recorded as the stroke was drawn (velocity taper,
   * or a pen's pressure). Rendered, hit-tested and exported from this array so
   * the three always agree; absent on a stroke drawn before widths existed,
   * and on the dev preview's fixtures — then `width` applies to the whole
   * stroke.
   */
  widths?: number[];
};

export type ImageShape = {
  kind: "image";
  id: number;
  /** An image data URL. Kept on the shape so state survives a re-render. */
  src: string;
  a: Point;
  b: Point;
  /** The decoded picture's own pixels; a resize never passes 1× of them. */
  natW: number;
  natH: number;
};

export type Shape =
  | PenShape
  | ImageShape
  | { kind: "line"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "arrow"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "rect"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "ellipse"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "diamond"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "text"; id: number; color: string; size: number; at: Point; text: string };

export interface Bounds {
  x: number;
  y: number;
  w: number;
  h: number;
}

let nextId = 1;
export function newId(): number {
  return nextId++;
}

const mid = (a: Point, b: Point): Point => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });

/* ── geometry ───────────────────────────────────────────────────────────── */

export function bounds(s: Shape): Bounds {
  switch (s.kind) {
    case "pen": {
      if (s.points.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
      const xs = s.points.map((p) => p.x);
      const ys = s.points.map((p) => p.y);
      const x = Math.min(...xs);
      const y = Math.min(...ys);
      return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
    }
    case "text":
      // Without measuring the font this is an estimate, and it only has to be
      // good enough to hit-test and to frame a selection.
      return {
        x: s.at.x,
        y: s.at.y - s.size,
        w: s.text.length * s.size * 0.55,
        h: s.size * 1.3,
      };
    default: {
      const x = Math.min(s.a.x, s.b.x);
      const y = Math.min(s.a.y, s.b.y);
      return { x, y, w: Math.abs(s.b.x - s.a.x), h: Math.abs(s.b.y - s.a.y) };
    }
  }
}

function nearSegment(p: Point, a: Point, b: Point, slack: number): boolean {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  // Degenerate segment: fall back to distance from the point itself.
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2));
  const cx = a.x + t * dx;
  const cy = a.y + t * dy;
  return Math.hypot(p.x - cx, p.y - cy) <= slack;
}

/** The width of a pen stroke at point `i`, whatever the stroke remembers. */
function penWidthAt(s: PenShape, i: number): number {
  return s.widths?.[i] ?? s.width;
}

/** The four midpoints of a box, in drawing order: the diamond's corners. */
function diamondCorners(b: Bounds): [Point, Point, Point, Point] {
  return [
    { x: b.x + b.w / 2, y: b.y },
    { x: b.x + b.w, y: b.y + b.h / 2 },
    { x: b.x + b.w / 2, y: b.y + b.h },
    { x: b.x, y: b.y + b.h / 2 },
  ];
}

/**
 * Whether `p` is on `s`'s outline — the one test selection and the eraser
 * share. Strokes hit near their line rather than inside their bounding box, so
 * a click inside an empty rectangle passes through to whatever is behind it.
 * An image and a text box are solid: there is nothing behind what they cover.
 */
function nearShape(s: Shape, p: Point): boolean {
  const slack = ("width" in s ? s.width : 2) + 4;
  switch (s.kind) {
    case "pen": {
      for (let j = 1; j < s.points.length; j++) {
        const width = (penWidthAt(s, j - 1) + penWidthAt(s, j)) / 2;
        if (nearSegment(p, s.points[j - 1], s.points[j], width + 4)) return true;
      }
      return false;
    }
    case "line":
    case "arrow":
      return nearSegment(p, s.a, s.b, slack);
    case "rect": {
      const b = bounds(s);
      const tl = { x: b.x, y: b.y };
      const tr = { x: b.x + b.w, y: b.y };
      const br = { x: b.x + b.w, y: b.y + b.h };
      const bl = { x: b.x, y: b.y + b.h };
      return (
        nearSegment(p, tl, tr, slack) ||
        nearSegment(p, tr, br, slack) ||
        nearSegment(p, br, bl, slack) ||
        nearSegment(p, bl, tl, slack)
      );
    }
    case "diamond": {
      const [top, right, bottom, left] = diamondCorners(bounds(s));
      return (
        nearSegment(p, top, right, slack) ||
        nearSegment(p, right, bottom, slack) ||
        nearSegment(p, bottom, left, slack) ||
        nearSegment(p, left, top, slack)
      );
    }
    case "ellipse": {
      const b = bounds(s);
      const rx = b.w / 2;
      const ry = b.h / 2;
      if (rx < 1 || ry < 1) return false;
      const nx = (p.x - (b.x + rx)) / rx;
      const ny = (p.y - (b.y + ry)) / ry;
      const d = Math.hypot(nx, ny);
      return Math.abs(d - 1) * Math.min(rx, ry) <= slack;
    }
    case "image":
    case "text": {
      const b = bounds(s);
      return p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h;
    }
  }
}

/**
 * Topmost shape under `p`, or null. Strokes hit near their outline rather than
 * their bounding box, so a click inside an empty rectangle passes through to
 * whatever is behind it — which is what you expect of an outline.
 */
export function hitTest(shapes: Shape[], p: Point): Shape | null {
  for (let i = shapes.length - 1; i >= 0; i--) {
    if (nearShape(shapes[i], p)) return shapes[i];
  }
  return null;
}

/** How far apart the eraser's samples along its sweep are, in px. */
export const ERASE_SAMPLE_STEP = 3;

/**
 * Every shape the eraser's sweep crosses, topmost first — the whole gesture is
 * one delete, so the result is the set of ids that go together. A click is a
 * one-point path and takes what is under the pointer; the path is sampled so a
 * fast flick cannot step over a thin stroke between two pointer samples; and
 * the test per sample is the same outline test selection uses, so the eraser
 * takes exactly what it highlights.
 */
export function eraseTargets(shapes: Shape[], path: Point[]): number[] {
  if (path.length === 0) return [];
  const samples = samplePath(path, ERASE_SAMPLE_STEP);
  const ids: number[] = [];
  for (let i = shapes.length - 1; i >= 0; i--) {
    if (samples.some((p) => nearShape(shapes[i], p))) ids.push(shapes[i].id);
  }
  return ids;
}

/**
 * The shapes one straight segment of a sweep crosses.
 *
 * A sweep's ghosts only ever grow, and the segment between two pointer samples
 * is the only part of the path the last move has not already tested — so
 * appending this to the ids already found is the same answer as re-testing the
 * whole path, at the cost of one segment per move instead of the entire
 * gesture. That is what makes a long sweep with the eraser flat: the old code
 * re-sampled every point since the pointer went down and tested all of them
 * against every shape, on every move.
 */
export function eraseCrossed(shapes: Shape[], from: Point, to: Point): number[] {
  const samples = samplePath([from, to], ERASE_SAMPLE_STEP);
  const ids: number[] = [];
  for (let i = shapes.length - 1; i >= 0; i--) {
    if (ids.includes(shapes[i].id)) continue;
    if (samples.some((p) => nearShape(shapes[i], p))) ids.push(shapes[i].id);
  }
  return ids;
}

/** Points every `step` px along a path, ends included. */
function samplePath(path: Point[], step: number): Point[] {
  const samples: Point[] = [{ ...path[0] }];
  for (let i = 1; i < path.length; i++) {
    const a = path[i - 1];
    const b = path[i];
    const distance = Math.hypot(b.x - a.x, b.y - a.y);
    const pieces = Math.max(1, Math.ceil(distance / step));
    for (let k = 1; k <= pieces; k++) {
      samples.push({
        x: a.x + ((b.x - a.x) * k) / pieces,
        y: a.y + ((b.y - a.y) * k) / pieces,
      });
    }
  }
  return samples;
}

export function move(s: Shape, dx: number, dy: number): void {
  switch (s.kind) {
    case "pen":
      for (const p of s.points) {
        p.x += dx;
        p.y += dy;
      }
      break;
    case "text":
      s.at.x += dx;
      s.at.y += dy;
      break;
    default:
      s.a.x += dx;
      s.a.y += dy;
      s.b.x += dx;
      s.b.y += dy;
  }
}

/** The 15° step Shift snaps a line or an arrow to, in radians. */
const ANGLE_STEP = Math.PI / 12;

/**
 * Shift while drawing. A line or an arrow snaps to 15° steps and keeps the
 * length the pointer asked for; a box — rect, ellipse, diamond — becomes a
 * square (a circle for the ellipse), taking the larger of the two drag
 * extents so the shape never falls behind the pointer.
 */
export function constrain(kind: ConstrainKind, a: Point, b: Point): Point {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  if (kind === "line" || kind === "arrow") {
    const length = Math.hypot(dx, dy);
    const angle = Math.round(Math.atan2(dy, dx) / ANGLE_STEP) * ANGLE_STEP;
    return { x: a.x + length * Math.cos(angle), y: a.y + length * Math.sin(angle) };
  }
  const side = Math.max(Math.abs(dx), Math.abs(dy));
  return { x: a.x + (dx < 0 ? -side : side), y: a.y + (dy < 0 ? -side : side) };
}

/* ── history ────────────────────────────────────────────────────────────── */

/**
 * A copy of the page, deep enough that later drawing cannot reach back through
 * a shared array. History is a list of whole states, not a stack of edits, so
 * undoing an erase brings the shapes back in their own order instead of
 * appending them at the end.
 */
export function snapshot(shapes: Shape[]): Shape[] {
  return shapes.map(cloneShape);
}

/** The same copy on the way back: a state lives in history until it is restored. */
export function restore(state: Shape[]): Shape[] {
  return state.map(cloneShape);
}

function cloneShape(s: Shape): Shape {
  switch (s.kind) {
    case "pen":
      return { ...s, points: s.points.map((p) => ({ ...p })), ...(s.widths ? { widths: [...s.widths] } : {}) };
    case "text":
      return { ...s, at: { ...s.at } };
    default:
      return { ...s, a: { ...s.a }, b: { ...s.b } };
  }
}

/** How many states Draw remembers; older ones fall off the end. */
export const HISTORY_CAP = 50;

export interface History {
  past: Shape[][];
  future: Shape[][];
}

export const emptyHistory = (): History => ({ past: [], future: [] });

/**
 * Record the state before a committed change — pass the snapshot taken before
 * the pointer went down, not the live page. A new change drops any redo, the
 * way every editor promises.
 */
export function record(h: History, before: Shape[]): History {
  return { past: [...h.past, before].slice(-HISTORY_CAP), future: [] };
}

/** Step back one state, if there is one. */
export function undo(h: History, current: Shape[]): { history: History; shapes: Shape[] } | null {
  if (h.past.length === 0) return null;
  return {
    history: { past: h.past.slice(0, -1), future: [...h.future, snapshot(current)] },
    shapes: restore(h.past[h.past.length - 1]),
  };
}

/** Step forward one state, if there is one. */
export function redo(h: History, current: Shape[]): { history: History; shapes: Shape[] } | null {
  if (h.future.length === 0) return null;
  return {
    history: {
      past: [...h.past, snapshot(current)].slice(-HISTORY_CAP),
      future: h.future.slice(0, -1),
    },
    shapes: restore(h.future[h.future.length - 1]),
  };
}

/* ── selection: handles, resize, duplicate ──────────────────────────────── */

export type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "a" | "b";

export interface Handle {
  id: HandleId;
  at: Point;
}

/** Which way each handle points from the shape's centre. */
const HANDLE_SIGN: Record<HandleId, Point> = {
  nw: { x: -1, y: -1 },
  n: { x: 0, y: -1 },
  ne: { x: 1, y: -1 },
  e: { x: 1, y: 0 },
  se: { x: 1, y: 1 },
  s: { x: 0, y: 1 },
  sw: { x: -1, y: 1 },
  w: { x: -1, y: 0 },
  a: { x: 0, y: 0 },
  b: { x: 0, y: 0 },
};

/**
 * The grips a selected shape offers. A box — rect, ellipse, diamond — gets the
 * eight of a box; a line or an arrow its two ends; a pen, a text and a picture
 * four corners, because all three scale as one piece.
 */
export function handles(s: Shape): Handle[] {
  const b = bounds(s);
  switch (s.kind) {
    case "line":
    case "arrow":
      return [
        { id: "a", at: { ...s.a } },
        { id: "b", at: { ...s.b } },
      ];
    case "pen":
    case "text":
    case "image":
      return [
        { id: "nw", at: { x: b.x, y: b.y } },
        { id: "ne", at: { x: b.x + b.w, y: b.y } },
        { id: "se", at: { x: b.x + b.w, y: b.y + b.h } },
        { id: "sw", at: { x: b.x, y: b.y + b.h } },
      ];
    default:
      return [
        { id: "nw", at: { x: b.x, y: b.y } },
        { id: "n", at: { x: b.x + b.w / 2, y: b.y } },
        { id: "ne", at: { x: b.x + b.w, y: b.y } },
        { id: "e", at: { x: b.x + b.w, y: b.y + b.h / 2 } },
        { id: "se", at: { x: b.x + b.w, y: b.y + b.h } },
        { id: "s", at: { x: b.x + b.w / 2, y: b.y + b.h } },
        { id: "sw", at: { x: b.x, y: b.y + b.h } },
        { id: "w", at: { x: b.x, y: b.y + b.h / 2 } },
      ];
  }
}

/**
 * The handle under a pointer, if any. The grip is drawn 8px, but its target is
 * the WCAG 2.5.8 24×24 — a resize you cannot reliably grab is not a control.
 */
export function handleAt(s: Shape, p: Point, radius = 12): HandleId | null {
  for (const h of handles(s)) {
    if (Math.abs(p.x - h.at.x) <= radius && Math.abs(p.y - h.at.y) <= radius) return h.id;
  }
  return null;
}

export interface ResizeMods {
  /** Keep the shape's proportions. */
  shift?: boolean;
  /** Grow and shrink about the centre. */
  alt?: boolean;
}

/** The smallest side a resize may leave; anything less inverts a shape. */
const MIN_SIDE = 2;

function setBox(s: { a: Point; b: Point }, x: number, y: number, w: number, h: number): void {
  s.a = { x, y };
  s.b = { x: x + w, y: y + h };
}

/**
 * Move a handle to `to`, editing the shape in place — the same contract as
 * `move`. Each kind answers in its own geometry: boxes resize their sides, a
 * line moves an end, a pen, a text and a picture scale as one piece about the
 * corner opposite the handle, and none of them can be pulled through itself —
 * every side bottoms out at 2px.
 */
export function resize(s: Shape, handle: HandleId, to: Point, mods: ResizeMods = {}): void {
  if (handle === "a" || handle === "b") {
    if (s.kind !== "line" && s.kind !== "arrow") return;
    const fixed = handle === "a" ? s.b : s.a;
    let at = { ...to };
    if (mods.shift) {
      const angle = Math.round(Math.atan2(to.y - fixed.y, to.x - fixed.x) / ANGLE_STEP) * ANGLE_STEP;
      const length = Math.hypot(to.x - fixed.x, to.y - fixed.y);
      at = { x: fixed.x + length * Math.cos(angle), y: fixed.y + length * Math.sin(angle) };
    }
    if (handle === "a") s.a = at;
    else s.b = at;
    return;
  }
  switch (s.kind) {
    case "pen":
      resizePen(s, handle, to);
      return;
    case "text":
      resizeText(s, handle, to);
      return;
    case "image":
      resizeImage(s, handle, to);
      return;
    case "line":
    case "arrow":
      return;
    default:
      resizeBox(s, handle, to, mods);
  }
}

/** Rect, ellipse and diamond resize as a box: sides move, Shift keeps the ratio. */
function resizeBox(
  s: Shape & { kind: "rect" | "ellipse" | "diamond" },
  handle: HandleId,
  to: Point,
  mods: ResizeMods,
): void {
  const b = bounds(s);
  const sign = HANDLE_SIGN[handle];
  if (mods.alt) {
    // Every side mirrors about the centre, so the centre is the one fixed thing.
    const cx = b.x + b.w / 2;
    const cy = b.y + b.h / 2;
    let w = sign.x === 0 ? b.w : Math.max(MIN_SIDE, 2 * Math.abs(to.x - cx));
    let h = sign.y === 0 ? b.h : Math.max(MIN_SIDE, 2 * Math.abs(to.y - cy));
    if (mods.shift && b.w > 0 && b.h > 0) {
      const scale = Math.max(w / b.w, h / b.h);
      w = b.w * scale;
      h = b.h * scale;
    }
    setBox(s, cx - w / 2, cy - h / 2, w, h);
    return;
  }
  if (sign.x !== 0 && sign.y !== 0) {
    // A corner: the corner opposite stays where it is. The extents are signed
    // by the handle's own direction, so pulling past the anchor bottoms out at
    // the minimum instead of growing back.
    const ax = sign.x < 0 ? b.x + b.w : b.x;
    const ay = sign.y < 0 ? b.y + b.h : b.y;
    let w = Math.max(MIN_SIDE, (to.x - ax) * sign.x);
    let h = Math.max(MIN_SIDE, (to.y - ay) * sign.y);
    if (mods.shift && b.w > 0 && b.h > 0) {
      const scale = Math.max(w / b.w, h / b.h);
      w = b.w * scale;
      h = b.h * scale;
    }
    setBox(s, sign.x < 0 ? ax - w : ax, sign.y < 0 ? ay - h : ay, w, h);
    return;
  }
  // An edge: one axis moves and the other stays; Shift carries the ratio
  // across, centred on the axis that did not move.
  if (sign.x !== 0) {
    const ax = sign.x < 0 ? b.x + b.w : b.x;
    const w = Math.max(MIN_SIDE, (to.x - ax) * sign.x);
    let h = b.h;
    let y = b.y;
    if (mods.shift && b.w > 0) {
      h = Math.max(MIN_SIDE, b.h * (w / b.w));
      y = b.y + b.h / 2 - h / 2;
    }
    setBox(s, sign.x < 0 ? ax - w : ax, y, w, h);
  } else {
    const ay = sign.y < 0 ? b.y + b.h : b.y;
    const h = Math.max(MIN_SIDE, (to.y - ay) * sign.y);
    let w = b.w;
    let x = b.x;
    if (mods.shift && b.h > 0) {
      w = Math.max(MIN_SIDE, b.w * (h / b.h));
      x = b.x + b.w / 2 - w / 2;
    }
    setBox(s, x, sign.y < 0 ? ay - h : ay, w, h);
  }
}

/** A pen scales as one piece about the opposite corner; its ink scales with it. */
function resizePen(s: Shape & { kind: "pen" }, handle: HandleId, to: Point): void {
  const b = bounds(s);
  const sign = HANDLE_SIGN[handle];
  const fx = sign.x < 0 ? b.x + b.w : b.x;
  const fy = sign.y < 0 ? b.y + b.h : b.y;
  const sx = b.w > 0 ? Math.max(MIN_SIDE / b.w, ((to.x - fx) * sign.x) / b.w) : 1;
  const sy = b.h > 0 ? Math.max(MIN_SIDE / b.h, ((to.y - fy) * sign.y) / b.h) : 1;
  const scale = Math.max(sx, sy);
  for (const p of s.points) {
    p.x = fx + (p.x - fx) * scale;
    p.y = fy + (p.y - fy) * scale;
  }
  if (s.widths) {
    for (let i = 0; i < s.widths.length; i++) s.widths[i] = Math.max(0.5, s.widths[i] * scale);
  } else {
    s.width = Math.max(0.5, s.width * scale);
  }
}

/** Text scales its size about the opposite corner; its anchor follows the box. */
function resizeText(s: Shape & { kind: "text" }, handle: HandleId, to: Point): void {
  const b = bounds(s);
  const sign = HANDLE_SIGN[handle];
  const fx = sign.x < 0 ? b.x + b.w : b.x;
  const fy = sign.y < 0 ? b.y + b.h : b.y;
  const sx = b.w > 0 ? ((to.x - fx) * sign.x) / b.w : 1;
  const sy = b.h > 0 ? ((to.y - fy) * sign.y) / b.h : 1;
  const scale = Math.max(
    Math.max(sx, sy),
    MIN_SIDE / Math.max(b.w, 1),
    MIN_SIDE / Math.max(b.h, 1),
  );
  const x = fx + (b.x - fx) * scale;
  const y = fy + (b.y - fy) * scale;
  s.size *= scale;
  s.at = { x, y: y + s.size };
}

/** A picture keeps its proportions and never passes its own pixels. */
function resizeImage(s: Shape & { kind: "image" }, handle: HandleId, to: Point): void {
  const b = bounds(s);
  const sign = HANDLE_SIGN[handle];
  const fx = sign.x < 0 ? b.x + b.w : b.x;
  const fy = sign.y < 0 ? b.y + b.h : b.y;
  const sx = b.w > 0 ? ((to.x - fx) * sign.x) / b.w : 1;
  const sy = b.h > 0 ? ((to.y - fy) * sign.y) / b.h : 1;
  const floor = Math.max(MIN_SIDE / Math.max(b.w, 1), MIN_SIDE / Math.max(b.h, 1));
  const ceiling = Math.min(
    s.natW / Math.max(b.w, 0.5),
    s.natH / Math.max(b.h, 0.5),
  );
  const scale = Math.min(Math.max(Math.max(sx, sy), floor), Math.max(ceiling, floor));
  const w = b.w * scale;
  const h = b.h * scale;
  setBox(s, sign.x < 0 ? fx - w : fx, sign.y < 0 ? fy - h : fy, w, h);
}

/** A second copy of a shape: its own id, its own arrays. */
export function duplicate(s: Shape): Shape {
  const copy = cloneShape(s);
  copy.id = newId();
  return copy;
}

/* ── ink ────────────────────────────────────────────────────────────────── */

/** The taper never runs past ±40% of the chosen width. */
const TAPER_MAX = 1.4;
const TAPER_MIN = 0.6;
/** Speed, in px/ms, at which a stroke reads as fast and stays thin. */
const FAST = 2.5;

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

/**
 * One sample's width. A pen reports pressure, so it is trusted for it;
 * anything else is read from how fast the pointer moves — quick strokes thin
 * out, slow ones pool. The clamp is the contract: never past ±40% of the
 * chosen width, so a stroke is never thinner or fatter than the toolbar said.
 */
export function taperWidth(base: number, speed: number, pressure: number | null): number {
  const factor =
    pressure !== null
      ? TAPER_MIN + 0.8 * clamp01(pressure)
      : TAPER_MAX - 0.8 * clamp01(speed / FAST);
  return base * Math.min(TAPER_MAX, Math.max(TAPER_MIN, factor));
}

/**
 * Light stabilisation: each sample is answered a little behind the pointer, so
 * a shaky hand draws a straighter line without the stroke feeling towed. The
 * last point is added raw when the pointer lifts, so the stroke still ends
 * where the hand stopped.
 */
export function stabilize(prev: Point, next: Point, ease = 0.8): Point {
  return { x: prev.x + (next.x - prev.x) * ease, y: prev.y + (next.y - prev.y) * ease };
}

/* ── painting ───────────────────────────────────────────────────────────── */

const ARROW_HEAD = 12;

function path(ctx: CanvasRenderingContext2D, s: Shape): void {
  ctx.beginPath();
  switch (s.kind) {
    case "pen": {
      if (s.points.length === 0) return;
      ctx.moveTo(s.points[0].x, s.points[0].y);
      // Quadratic through midpoints: smooths the polyline without needing to
      // fit real curves, and costs nothing at sample rate.
      for (let i = 1; i < s.points.length - 1; i++) {
        const m = mid(s.points[i], s.points[i + 1]);
        ctx.quadraticCurveTo(s.points[i].x, s.points[i].y, m.x, m.y);
      }
      const last = s.points[s.points.length - 1];
      ctx.lineTo(last.x, last.y);
      break;
    }
    case "line":
      ctx.moveTo(s.a.x, s.a.y);
      ctx.lineTo(s.b.x, s.b.y);
      break;
    case "arrow": {
      ctx.moveTo(s.a.x, s.a.y);
      ctx.lineTo(s.b.x, s.b.y);
      const ang = Math.atan2(s.b.y - s.a.y, s.b.x - s.a.x);
      const head = Math.max(ARROW_HEAD, s.width * 3);
      for (const spread of [Math.PI - 0.42, Math.PI + 0.42]) {
        ctx.moveTo(s.b.x, s.b.y);
        ctx.lineTo(s.b.x + head * Math.cos(ang + spread), s.b.y + head * Math.sin(ang + spread));
      }
      break;
    }
    case "rect": {
      const b = bounds(s);
      const r = Math.min(6, b.w / 2, b.h / 2);
      ctx.roundRect(b.x, b.y, b.w, b.h, r);
      break;
    }
    case "ellipse": {
      const b = bounds(s);
      ctx.ellipse(b.x + b.w / 2, b.y + b.h / 2, b.w / 2, b.h / 2, 0, 0, Math.PI * 2);
      break;
    }
    case "diamond": {
      const [top, right, bottom, left] = diamondCorners(bounds(s));
      ctx.moveTo(top.x, top.y);
      ctx.lineTo(right.x, right.y);
      ctx.lineTo(bottom.x, bottom.y);
      ctx.lineTo(left.x, left.y);
      ctx.closePath();
      break;
    }
    case "text":
    case "image":
      break;
  }
}

/**
 * A pen whose width varies point by point: each smoothed segment is stroked on
 * its own so the width can interpolate along the line. A single path is smooth
 * but holds one width; a taper needs the segments separate. The SVG exporter
 * walks the same segments, so page and export agree.
 */
function strokePen(ctx: CanvasRenderingContext2D, s: PenShape): void {
  const points = s.points;
  if (points.length < 2) return;
  if (points.length === 2) {
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    ctx.lineTo(points[1].x, points[1].y);
    ctx.lineWidth = (penWidthAt(s, 0) + penWidthAt(s, 1)) / 2;
    ctx.stroke();
    return;
  }
  let start = points[0];
  for (let i = 1; i < points.length - 1; i++) {
    const end = mid(points[i], points[i + 1]);
    ctx.beginPath();
    ctx.moveTo(start.x, start.y);
    ctx.quadraticCurveTo(points[i].x, points[i].y, end.x, end.y);
    ctx.lineWidth = penWidthAt(s, i);
    ctx.stroke();
    start = end;
  }
  const last = points[points.length - 1];
  ctx.beginPath();
  ctx.moveTo(start.x, start.y);
  ctx.lineTo(last.x, last.y);
  ctx.lineWidth = penWidthAt(s, points.length - 1);
  ctx.stroke();
}

export interface RenderOptions {
  dpr?: number;
  background?: string;
  ink?: string;
  selected?: number | null;
  /**
   * The selection's ink: the theme's `--accent`, read by the window and passed
   * in. There is no literal fallback — without it the ring and the grips are
   * not drawn, so the colour can only ever come from the token.
   */
  accent?: string;
  images?: ReadonlyMap<string, CanvasImageSource>;
  /** Shapes the eraser is about to take: painted dimmed until the pointer lifts. */
  ghost?: readonly number[] | null;
  /** The eraser's ring, at the pointer. */
  eraser?: { at: Point; r: number } | null;
}

function drawHandle(ctx: CanvasRenderingContext2D, at: Point, fill: string, accent: string): void {
  ctx.beginPath();
  ctx.roundRect(at.x - 4, at.y - 4, 8, 8, 2);
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.strokeStyle = accent;
  ctx.lineWidth = 1.25;
  ctx.stroke();
}

export function render(
  ctx: CanvasRenderingContext2D,
  shapes: Shape[],
  w: number,
  h: number,
  opts: RenderOptions = {}
): void {
  const dpr = opts.dpr ?? 1;
  const accent = opts.accent;
  const ghosts = opts.ghost && opts.ghost.length > 0 ? new Set(opts.ghost) : null;
  ctx.save();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  if (opts.background) {
    ctx.fillStyle = opts.background;
    ctx.fillRect(0, 0, w, h);
  }

  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  for (const s of shapes) {
    ctx.globalAlpha = ghosts?.has(s.id) ? 0.3 : 1;
    if (s.kind === "text") {
      ctx.fillStyle = paintFor(s.color, opts.ink);
      ctx.font = `${s.size}px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`;
      ctx.textBaseline = "alphabetic";
      ctx.fillText(s.text, s.at.x, s.at.y);
    } else if (s.kind === "image") {
      const b = bounds(s);
      const img = opts.images?.get(s.src);
      // An undecoded picture paints nothing rather than a placeholder: the
      // window blocks every export while one is still loading, so what is
      // visible is what would be saved.
      if (img) ctx.drawImage(img, b.x, b.y, b.w, b.h);
    } else {
      ctx.strokeStyle = paintFor(s.color, opts.ink);
      ctx.lineWidth = s.width;
      if (s.kind === "pen" && s.widths && s.widths.length === s.points.length) {
        strokePen(ctx, s);
      } else {
        path(ctx, s);
        ctx.stroke();
      }
    }
  }
  ctx.globalAlpha = 1;

  if (opts.selected != null && accent) {
    const s = shapes.find((x) => x.id === opts.selected);
    if (s) {
      const b = bounds(s);
      ctx.strokeStyle = accent;
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 3]);
      ctx.strokeRect(b.x - 5, b.y - 5, b.w + 10, b.h + 10);
      ctx.setLineDash([]);
      for (const grip of handles(s)) {
        drawHandle(ctx, grip.at, opts.background ?? "#ffffff", accent);
      }
    }
  }

  if (opts.eraser && accent) {
    ctx.beginPath();
    ctx.arc(opts.eraser.at.x, opts.eraser.at.y, opts.eraser.r, 0, Math.PI * 2);
    ctx.fillStyle = accent;
    ctx.globalAlpha = 0.14;
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1.5;
    ctx.stroke();
  }

  ctx.restore();
}

/**
 * Render to an offscreen canvas and return base64 PNG data (no data: prefix),
 * which is the shape `add_image` already accepts.
 *
 * The export is always drawn on an opaque background: a transparent PNG pasted
 * into a dark chat or a light document is unreadable in one of the two, and a
 * drawing is made to be pasted. `images` carries the decoded pictures the page
 * holds; a picture that is not in it cannot be drawn, so the window refuses to
 * export rather than exporting without it.
 */
export function exportPng(
  shapes: Shape[],
  w: number,
  h: number,
  scale = 2,
  background = "#ffffff",
  images?: ReadonlyMap<string, CanvasImageSource>
): string {
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(w * scale);
  canvas.height = Math.round(h * scale);
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  render(ctx, shapes, w, h, { dpr: scale, background, ink: inkFor(background), images });
  return canvas.toDataURL("image/png").split(",")[1] ?? "";
}

/* ── SVG export ─────────────────────────────────────────────────────────── */

/** Space between the drawing and the exported edge. */
const SVG_PAD = 16;

/** A number an SVG attribute can read: two decimals at most, no exponents. */
function num(v: number): string {
  return String(Math.round(v * 100) / 100);
}

function attrs(values: Record<string, string | number | null | undefined>): string {
  const out: string[] = [];
  for (const [key, value] of Object.entries(values)) {
    if (value === null || value === undefined) continue;
    out.push(` ${key}="${typeof value === "string" ? escapeText(value) : num(value)}"`);
  }
  return out.join("");
}

function escapeText(v: string): string {
  return v
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** The one smoothed path a pen with a single width exports as. */
function penPathD(s: PenShape): string {
  const p = s.points;
  if (p.length === 0) return "";
  let d = `M ${num(p[0].x)} ${num(p[0].y)}`;
  for (let i = 1; i < p.length - 1; i++) {
    const m = mid(p[i], p[i + 1]);
    d += ` Q ${num(p[i].x)} ${num(p[i].y)} ${num(m.x)} ${num(m.y)}`;
  }
  const last = p[p.length - 1];
  d += ` L ${num(last.x)} ${num(last.y)}`;
  return d;
}

/** The same segments `strokePen` paints, as standalone path data with widths. */
function penSegments(s: PenShape): { d: string; width: number }[] {
  const points = s.points;
  if (points.length < 2) return [];
  if (points.length === 2) {
    return [
      {
        d: `M ${num(points[0].x)} ${num(points[0].y)} L ${num(points[1].x)} ${num(points[1].y)}`,
        width: (penWidthAt(s, 0) + penWidthAt(s, 1)) / 2,
      },
    ];
  }
  const segments: { d: string; width: number }[] = [];
  let start = points[0];
  for (let i = 1; i < points.length - 1; i++) {
    const end = mid(points[i], points[i + 1]);
    segments.push({
      d: `M ${num(start.x)} ${num(start.y)} Q ${num(points[i].x)} ${num(points[i].y)} ${num(end.x)} ${num(end.y)}`,
      width: penWidthAt(s, i),
    });
    start = end;
  }
  const last = points[points.length - 1];
  segments.push({
    d: `M ${num(start.x)} ${num(start.y)} L ${num(last.x)} ${num(last.y)}`,
    width: penWidthAt(s, points.length - 1),
  });
  return segments;
}

const STROKE_ATTRS = { fill: "none", "stroke-linecap": "round", "stroke-linejoin": "round" } as const;

function penElement(s: PenShape, ink: string | undefined): string {
  const paint = paintFor(s.color, ink);
  if (!s.widths || s.widths.length !== s.points.length) {
    return `<path${attrs({ d: penPathD(s), ...STROKE_ATTRS, stroke: paint, "stroke-width": s.width })}/>`;
  }
  // Segments share an element while their width rounds to the same quarter
  // pixel, so a tapered stroke exports as a handful of paths rather than one
  // per sample — and never as a filled outline.
  const runs: { d: string[]; width: number }[] = [];
  for (const segment of penSegments(s)) {
    const width = Math.round(segment.width * 4) / 4;
    const last = runs[runs.length - 1];
    if (last && last.width === width) last.d.push(segment.d);
    else runs.push({ d: [segment.d], width });
  }
  return runs
    .map((run) => `<path${attrs({ d: run.d.join(" "), ...STROKE_ATTRS, stroke: paint, "stroke-width": run.width })}/>`)
    .join("\n");
}

function svgShape(s: Shape, ink: string | undefined): string {
  switch (s.kind) {
    case "pen":
      return penElement(s, ink);
    case "line": {
      const paint = paintFor(s.color, ink);
      return `<line${attrs({ ...STROKE_ATTRS, x1: s.a.x, y1: s.a.y, x2: s.b.x, y2: s.b.y, stroke: paint, "stroke-width": s.width })}/>`;
    }
    case "arrow": {
      const paint = paintFor(s.color, ink);
      const line = `<line${attrs({ ...STROKE_ATTRS, x1: s.a.x, y1: s.a.y, x2: s.b.x, y2: s.b.y, stroke: paint, "stroke-width": s.width })}/>`;
      const head = Math.max(ARROW_HEAD, s.width * 3);
      const angle = Math.atan2(s.b.y - s.a.y, s.b.x - s.a.x);
      const spread = [Math.PI - 0.42, Math.PI + 0.42].map((turn) => ({
        x: s.b.x + head * Math.cos(angle + turn),
        y: s.b.y + head * Math.sin(angle + turn),
      }));
      const d = `M ${num(spread[0].x)} ${num(spread[0].y)} L ${num(s.b.x)} ${num(s.b.y)} L ${num(spread[1].x)} ${num(spread[1].y)}`;
      const barb = `<path${attrs({ ...STROKE_ATTRS, d, stroke: paint, "stroke-width": s.width })}/>`;
      return `${line}\n${barb}`;
    }
    case "rect": {
      const paint = paintFor(s.color, ink);
      const b = bounds(s);
      const r = Math.min(6, b.w / 2, b.h / 2);
      return `<rect${attrs({ ...STROKE_ATTRS, x: b.x, y: b.y, width: b.w, height: b.h, rx: r > 0 ? r : null, stroke: paint, "stroke-width": s.width })}/>`;
    }
    case "ellipse": {
      const paint = paintFor(s.color, ink);
      const b = bounds(s);
      return `<ellipse${attrs({ ...STROKE_ATTRS, cx: b.x + b.w / 2, cy: b.y + b.h / 2, rx: b.w / 2, ry: b.h / 2, stroke: paint, "stroke-width": s.width })}/>`;
    }
    case "diamond": {
      const paint = paintFor(s.color, ink);
      const [top, right, bottom, left] = diamondCorners(bounds(s));
      const points = `${num(top.x)},${num(top.y)} ${num(right.x)},${num(right.y)} ${num(bottom.x)},${num(bottom.y)} ${num(left.x)},${num(left.y)}`;
      return `<polygon${attrs({ ...STROKE_ATTRS, points, stroke: paint, "stroke-width": s.width })}/>`;
    }
    case "text": {
      const paint = paintFor(s.color, ink);
      return `<text${attrs({
        x: s.at.x,
        y: s.at.y,
        "font-size": s.size,
        fill: paint,
        "font-family": "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif",
      })}>${escapeText(s.text)}</text>`;
    }
    case "image": {
      const b = bounds(s);
      return `<image${attrs({
        x: b.x,
        y: b.y,
        width: b.w,
        height: b.h,
        href: s.src,
        preserveAspectRatio: "xMidYMid meet",
      })}/>`;
    }
  }
}

/** The whole drawing's bounds, which is what the export frames. */
function contentBounds(shapes: Shape[]): Bounds {
  if (shapes.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const s of shapes) {
    const b = bounds(s);
    minX = Math.min(minX, b.x);
    minY = Math.min(minY, b.y);
    maxX = Math.max(maxX, b.x + b.w);
    maxY = Math.max(maxY, b.y + b.h);
  }
  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
}

/**
 * The drawing as an SVG document, sized to what is on the page rather than to
 * the window: a page-wide viewBox would export a sea of empty margin around a
 * small sketch. `background` is the page colour and becomes the one full-size
 * rect, painted first so pictures and strokes land on it; null exports
 * transparent. This cut always passes the page — there is no "no page" mode.
 */
export function exportSvg(shapes: Shape[], opts: { background?: string | null } = {}): string {
  const box = contentBounds(shapes);
  const x = box.x - SVG_PAD;
  const y = box.y - SVG_PAD;
  const w = box.w + SVG_PAD * 2;
  const h = box.h + SVG_PAD * 2;
  const ink = opts.background ? inkFor(opts.background) : undefined;
  const body: string[] = [];
  if (opts.background) {
    body.push(`<rect${attrs({ x, y, width: w, height: h, fill: opts.background })}/>`);
  }
  for (const s of shapes) body.push(svgShape(s, ink));
  return [
    `<svg${attrs({ xmlns: "http://www.w3.org/2000/svg", width: w, height: h, viewBox: `${num(x)} ${num(y)} ${num(w)} ${num(h)}` })}>`,
    ...body,
    "</svg>",
  ].join("\n");
}
