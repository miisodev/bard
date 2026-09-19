/*
  The drawing model.

  Deliberately free of React and of Tauri: it is data plus pure functions over
  that data, so the canvas can be driven imperatively at pointer speed without
  a re-render per sample, and so the whole thing can be reasoned about (and one
  day tested) without a window.

  Coordinates are canvas-space CSS pixels. Device-pixel scaling happens once,
  in `render`, via the context transform — nothing in this file knows about DPI.
*/

export type Tool = "select" | "pen" | "line" | "arrow" | "rect" | "ellipse" | "text";

export interface Point {
  x: number;
  y: number;
}

export type Shape =
  | { kind: "pen"; id: number; color: string; width: number; points: Point[] }
  | { kind: "line"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "arrow"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "rect"; id: number; color: string; width: number; a: Point; b: Point }
  | { kind: "ellipse"; id: number; color: string; width: number; a: Point; b: Point }
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

/* ── geometry ───────────────────────────────────────────────────────────── */

export function bounds(s: Shape): Bounds {
  switch (s.kind) {
    case "pen": {
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

/**
 * Topmost shape under `p`, or null. Strokes hit near their outline rather than
 * their bounding box, so a click inside an empty rectangle passes through to
 * whatever is behind it — which is what you expect of an outline.
 */
export function hitTest(shapes: Shape[], p: Point): Shape | null {
  for (let i = shapes.length - 1; i >= 0; i--) {
    const s = shapes[i];
    const slack = ("width" in s ? s.width : 2) + 4;
    switch (s.kind) {
      case "pen":
        for (let j = 1; j < s.points.length; j++) {
          if (nearSegment(p, s.points[j - 1], s.points[j], slack)) return s;
        }
        break;
      case "line":
      case "arrow":
        if (nearSegment(p, s.a, s.b, slack)) return s;
        break;
      case "rect": {
        const b = bounds(s);
        const tl = { x: b.x, y: b.y };
        const tr = { x: b.x + b.w, y: b.y };
        const br = { x: b.x + b.w, y: b.y + b.h };
        const bl = { x: b.x, y: b.y + b.h };
        if (
          nearSegment(p, tl, tr, slack) ||
          nearSegment(p, tr, br, slack) ||
          nearSegment(p, br, bl, slack) ||
          nearSegment(p, bl, tl, slack)
        )
          return s;
        break;
      }
      case "ellipse": {
        const b = bounds(s);
        const rx = b.w / 2;
        const ry = b.h / 2;
        if (rx < 1 || ry < 1) break;
        const nx = (p.x - (b.x + rx)) / rx;
        const ny = (p.y - (b.y + ry)) / ry;
        const d = Math.hypot(nx, ny);
        if (Math.abs(d - 1) * Math.min(rx, ry) <= slack) return s;
        break;
      }
      case "text": {
        const b = bounds(s);
        if (p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h) return s;
        break;
      }
    }
  }
  return null;
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
        const mx = (s.points[i].x + s.points[i + 1].x) / 2;
        const my = (s.points[i].y + s.points[i + 1].y) / 2;
        ctx.quadraticCurveTo(s.points[i].x, s.points[i].y, mx, my);
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
    case "text":
      break;
  }
}

export function render(
  ctx: CanvasRenderingContext2D,
  shapes: Shape[],
  w: number,
  h: number,
  opts: { dpr?: number; background?: string; selected?: number | null } = {}
): void {
  const dpr = opts.dpr ?? 1;
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
    if (s.kind === "text") {
      ctx.fillStyle = s.color;
      ctx.font = `${s.size}px "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`;
      ctx.textBaseline = "alphabetic";
      ctx.fillText(s.text, s.at.x, s.at.y);
    } else {
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.width;
      path(ctx, s);
      ctx.stroke();
    }
  }

  if (opts.selected != null) {
    const s = shapes.find((x) => x.id === opts.selected);
    if (s) {
      const b = bounds(s);
      ctx.strokeStyle = "#2f6fed";
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 3]);
      ctx.strokeRect(b.x - 5, b.y - 5, b.w + 10, b.h + 10);
      ctx.setLineDash([]);
    }
  }

  ctx.restore();
}

/**
 * Render to an offscreen canvas and return base64 PNG data (no data: prefix),
 * which is the shape `add_image` already accepts.
 *
 * The export is always drawn on an opaque background: a transparent PNG pasted
 * into a dark chat or a light document is unreadable in one of the two, and a
 * drawing is made to be pasted.
 */
export function exportPng(
  shapes: Shape[],
  w: number,
  h: number,
  scale = 2,
  background = "#ffffff"
): string {
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(w * scale);
  canvas.height = Math.round(h * scale);
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  render(ctx, shapes, w, h, { dpr: scale, background });
  return canvas.toDataURL("image/png").split(",")[1] ?? "";
}
