import { describe, expect, it } from "vitest";
import {
  bounds,
  constrain,
  duplicate,
  emptyHistory,
  eraseCrossed,
  eraseTargets,
  exportSvg,
  handleAt,
  handles,
  HISTORY_CAP,
  hitTest,
  move,
  newId,
  record,
  redo,
  resize,
  restore,
  snapshot,
  stabilize,
  taperWidth,
  undo,
  type ImageShape,
  type PenShape,
  type Shape,
} from "./shapes";

const rect = (x: number, y: number, w: number, h: number, width = 2): Shape => ({
  kind: "rect",
  id: newId(),
  color: "#1d1d1f",
  width,
  a: { x, y },
  b: { x: x + w, y: y + h },
});

const line = (x1: number, y1: number, x2: number, y2: number, width = 2): Shape => ({
  kind: "line",
  id: newId(),
  color: "#1d1d1f",
  width,
  a: { x: x1, y: y1 },
  b: { x: x2, y: y2 },
});

const pen = (points: [number, number][], width = 2, widths?: number[]): PenShape => ({
  kind: "pen",
  id: newId(),
  color: "#1d1d1f",
  width,
  points: points.map(([x, y]) => ({ x, y })),
  widths,
});

const image = (x: number, y: number, w: number, h: number, natW: number, natH: number): ImageShape => ({
  kind: "image",
  id: newId(),
  src: "data:image/png;base64,AAAA",
  a: { x, y },
  b: { x: x + w, y: y + h },
  natW,
  natH,
});

describe("bounds", () => {
  it("frames every kind, including the diamond and the image", () => {
    expect(bounds(rect(10, 20, 30, 40))).toEqual({ x: 10, y: 20, w: 30, h: 40 });
    expect(
      bounds({ kind: "diamond", id: newId(), color: "#000", width: 2, a: { x: 5, y: 6 }, b: { x: 45, y: 26 } }),
    ).toEqual({ x: 5, y: 6, w: 40, h: 20 });
    expect(bounds(image(5, 6, 20, 10, 200, 100))).toEqual({ x: 5, y: 6, w: 20, h: 10 });
    expect(bounds({ kind: "text", id: newId(), color: "#000", size: 20, at: { x: 100, y: 200 }, text: "abcd" })).toEqual({
      x: 100,
      y: 180,
      w: 44,
      h: 26,
    });
    expect(bounds(pen([[4, 4], [10, 20], [2, 6]]))).toEqual({ x: 2, y: 4, w: 8, h: 16 });
  });
});

describe("hitTest", () => {
  it("hits the outline, not the bounding box", () => {
    const r = rect(0, 0, 100, 100);
    expect(hitTest([r], { x: 50, y: 50 })).toBeNull();
    expect(hitTest([r], { x: 0, y: 50 })).toBe(r);
  });

  it("reads a few px either side of a stroke as the stroke", () => {
    const r = rect(0, 0, 100, 100); // width 2 → 6px of slack
    expect(hitTest([r], { x: 50, y: -5 })).toBe(r);
    expect(hitTest([r], { x: 50, y: -7 })).toBeNull();
  });

  it("returns the topmost shape", () => {
    const under = rect(0, 0, 100, 100);
    const over = rect(0, 0, 100, 100);
    expect(hitTest([under, over], { x: 0, y: 50 })).toBe(over);
  });

  it("passes through the inside of an empty rectangle to what is behind", () => {
    const behind = line(40, 100, 160, 100);
    const inner = rect(80, 80, 40, 40);
    const outer = rect(0, 0, 200, 200);
    expect(hitTest([behind, outer, inner], { x: 100, y: 100 })).toBe(behind);
  });

  it("treats an image and a text box as solid", () => {
    const img = image(0, 0, 100, 60, 100, 60);
    const text: Shape = { kind: "text", id: newId(), color: "#000", size: 20, at: { x: 10, y: 40 }, text: "hi" };
    expect(hitTest([img], { x: 50, y: 30 })).toBe(img);
    expect(hitTest([text], { x: 20, y: 30 })).toBe(text);
  });

  it("hits only the four diamond segments", () => {
    const d: Shape = { kind: "diamond", id: newId(), color: "#000", width: 2, a: { x: 0, y: 0 }, b: { x: 100, y: 100 } };
    expect(hitTest([d], { x: 50, y: 0 })).toBe(d);
    expect(hitTest([d], { x: 50, y: 20 })).toBeNull();
  });
});

describe("move and duplicate", () => {
  it("moves every kind in place", () => {
    const r = rect(0, 0, 10, 10);
    move(r, 5, -3);
    expect(bounds(r)).toEqual({ x: 5, y: -3, w: 10, h: 10 });
    const t: Shape = { kind: "text", id: newId(), color: "#000", size: 20, at: { x: 0, y: 20 }, text: "a" };
    move(t, 4, 4);
    expect(t.kind === "text" && t.at).toEqual({ x: 4, y: 24 });
    const p = pen([[0, 0], [10, 10]]);
    move(p, 2, 2);
    expect(p.points.map((point) => [point.x, point.y])).toEqual([
      [2, 2],
      [12, 12],
    ]);
  });

  it("duplicates with a new id and its own arrays", () => {
    const p = pen([[0, 0], [10, 10]], 3, [1, 2]);
    const copy = duplicate(p);
    expect(copy.id).not.toBe(p.id);
    expect(copy).toEqual({ ...p, id: copy.id });
    expect(copy).not.toBe(p);
    const copyPen = copy as PenShape;
    expect(copyPen.points).not.toBe(p.points);
    expect(copyPen.widths).not.toBe(p.widths);
    copyPen.points[0].x = 99;
    copyPen.widths![0] = 99;
    expect(p.points[0].x).toBe(0);
    expect(p.widths![0]).toBe(1);

    const img = image(0, 0, 10, 10, 10, 10);
    const copyImg = duplicate(img) as ImageShape;
    expect(copyImg.a).not.toBe(img.a);
    copyImg.a.x = 50;
    expect(img.a.x).toBe(0);
  });
});

describe("resize", () => {
  it("maps a corner drag onto the opposite anchor", () => {
    const r = rect(0, 0, 100, 50);
    resize(r, "se", { x: 120, y: 80 });
    expect(bounds(r)).toEqual({ x: 0, y: 0, w: 120, h: 80 });
  });

  it("clamps at 2px and never inverts", () => {
    const r = rect(0, 0, 100, 50);
    resize(r, "se", { x: -30, y: -10 });
    expect(bounds(r)).toEqual({ x: 0, y: 0, w: 2, h: 2 });
    const back = rect(100, 50, 100, 50);
    resize(back, "nw", { x: 500, y: 500 });
    expect(bounds(back)).toEqual({ x: 198, y: 98, w: 2, h: 2 });
  });

  it("keeps the ratio under Shift", () => {
    const r = rect(0, 0, 100, 50);
    resize(r, "se", { x: 150, y: 60 }, { shift: true });
    expect(bounds(r)).toEqual({ x: 0, y: 0, w: 150, h: 75 });
  });

  it("grows from the centre under Alt", () => {
    const r = rect(0, 0, 100, 50);
    resize(r, "se", { x: 60, y: 30 }, { alt: true });
    expect(bounds(r)).toEqual({ x: 40, y: 20, w: 20, h: 10 });
  });

  it("moves a line's endpoint, with a 15° snap under Shift", () => {
    const free = line(0, 0, 100, 0);
    resize(free, "b", { x: 50, y: 80 });
    expect(free.kind === "line" && free.b).toEqual({ x: 50, y: 80 });
    expect(free.kind === "line" && free.a).toEqual({ x: 0, y: 0 });

    const snapped = line(0, 0, 100, 0);
    resize(snapped, "b", { x: 10, y: 90 }, { shift: true });
    // 90° is on the 15° grid; the length is kept.
    expect(snapped.kind === "line" && Math.round(snapped.b.x)).toBe(0);
    expect(snapped.kind === "line" && Math.round(snapped.b.y)).toBe(Math.round(Math.hypot(10, 90)));
  });

  it("scales a pen uniformly about the opposite corner", () => {
    const p = pen([[0, 0], [10, 10], [20, 4]]);
    resize(p, "se", { x: 60, y: 20 });
    expect(p.points.map((point) => [point.x, point.y])).toEqual([
      [0, 0],
      [30, 30],
      [60, 12],
    ]);
  });

  it("scales a text's size", () => {
    const t: Shape = { kind: "text", id: newId(), color: "#000", size: 20, at: { x: 0, y: 20 }, text: "abcd" };
    resize(t, "se", { x: 88, y: 26 });
    expect(t.kind === "text" && t.size).toBe(40);
    expect(bounds(t)).toEqual({ x: 0, y: 0, w: 88, h: 52 });
  });

  it("keeps a picture's proportions and never passes 1×", () => {
    const img = image(0, 0, 100, 50, 200, 100);
    resize(img, "se", { x: 400, y: 200 });
    expect(bounds(img)).toEqual({ x: 0, y: 0, w: 200, h: 100 });
    const small = image(0, 0, 100, 50, 200, 100);
    resize(small, "se", { x: -10, y: -10 });
    expect(bounds(small)).toEqual({ x: 0, y: 0, w: 4, h: 2 });
  });

  it("offers the right handles per kind", () => {
    const ids = (s: Shape) => handles(s).map((h) => h.id);
    expect(ids(rect(0, 0, 10, 10))).toEqual(["nw", "n", "ne", "e", "se", "s", "sw", "w"]);
    expect(ids(line(0, 0, 10, 10))).toEqual(["a", "b"]);
    expect(ids(pen([[0, 0], [10, 10]]))).toEqual(["nw", "ne", "se", "sw"]);
    expect(ids(image(0, 0, 10, 10, 10, 10))).toEqual(["nw", "ne", "se", "sw"]);
  });

  it("gives a handle a 24×24 target though it is drawn 8px", () => {
    const r = rect(0, 0, 100, 50);
    expect(handleAt(r, { x: 12, y: 12 })).toBe("nw");
    expect(handleAt(r, { x: 50, y: 12 })).toBe("n");
    expect(handleAt(r, { x: 11, y: 50 })).toBe("sw");
    expect(handleAt(r, { x: 14, y: 50 })).toBeNull();
    expect(handleAt(r, { x: 40, y: 30 })).toBeNull();
  });
});

describe("eraseTargets", () => {
  it("takes every shape a sweep crosses, topmost first", () => {
    const under = line(0, 100, 100, 100);
    const over = line(0, 200, 100, 200);
    const ids = eraseTargets([under, over], [{ x: 50, y: 0 }, { x: 50, y: 300 }]);
    expect(ids).toEqual([over.id, under.id]);
  });

  it("takes a shape under a click, and nothing from the inside of an empty rect", () => {
    const r = rect(0, 0, 100, 100);
    expect(eraseTargets([r], [{ x: 50, y: 0 }])).toEqual([r.id]);
    expect(eraseTargets([r], [{ x: 50, y: 50 }])).toEqual([]);
  });

  it("removes as one set, so undo puts every shape back in its own order", () => {
    const a = rect(0, 0, 10, 10);
    const b = rect(20, 20, 10, 10);
    const c = line(0, 100, 100, 100);
    const before = snapshot([a, b, c]);
    const ids = eraseTargets([a, b, c], [{ x: 25, y: 20 }, { x: 25, y: 33 }]);
    expect(ids).toEqual([b.id]);
    const after = [a, b, c].filter((s) => !ids.includes(s.id));
    const step = undo(record(emptyHistory(), before), after);
    expect(step?.shapes.map((s) => s.id)).toEqual([a.id, b.id, c.id]);
  });
});

describe("eraseCrossed", () => {
  it("answers segment by segment what the whole sweep answers", () => {
    const shapes = [line(0, 100, 100, 100), rect(40, 95, 20, 10), rect(0, 0, 10, 10)];
    const path = [
      { x: 0, y: 0 },
      { x: 10, y: 40 },
      { x: 50, y: 99 },
      { x: 90, y: 100 },
      { x: 92, y: 101 },
    ];
    const whole = new Set(eraseTargets(shapes, path));
    const incremental = new Set<number>();
    for (let i = 1; i < path.length; i++) {
      for (const id of eraseCrossed(shapes, path[i - 1], path[i])) incremental.add(id);
    }
    expect([...incremental].sort()).toEqual([...whole].sort());
    // Topmost first, so the order a segment reports matches the whole sweep's.
    expect(eraseCrossed(shapes, { x: 50, y: 90 }, { x: 50, y: 110 })).toEqual(
      eraseTargets(shapes, [{ x: 50, y: 90 }, { x: 50, y: 110 }]),
    );
  });

  it("sampling is the sweep's, so a fast flick cannot step over a stroke", () => {
    const r = line(50, 100, 50, 100);
    expect(eraseCrossed([r], { x: 0, y: 100 }, { x: 100, y: 100 })).toEqual([r.id]);
  });
});

describe("history", () => {
  it("snapshots and restores without sharing arrays", () => {
    const p = pen([[0, 0], [10, 10]], 3, [1, 2]);
    const snap = snapshot([p]);
    p.points[0].x = 99;
    p.widths![0] = 9;
    expect(snap[0].kind === "pen" && snap[0].points[0].x).toBe(0);
    expect(snap[0].kind === "pen" && snap[0].widths![0]).toBe(1);
    const back = restore(snap);
    expect(back).toEqual(snap);
    expect(back).not.toBe(snap);
    expect(back[0]).not.toBe(snap[0]);
  });

  it("caps the past and drops the redo on a new change", () => {
    let h = emptyHistory();
    for (let i = 0; i < HISTORY_CAP + 10; i++) h = record(h, snapshot([rect(0, 0, 10, 10)]));
    expect(h.past.length).toBe(HISTORY_CAP);
    const stepped = undo(h, [rect(0, 0, 10, 10)]);
    expect(stepped?.history.future.length).toBe(1);
    h = record(stepped!.history, snapshot([]));
    expect(h.future).toEqual([]);
  });

  it("steps back to a whole state and forward again", () => {
    const a = rect(0, 0, 10, 10);
    const b = rect(20, 20, 10, 10);
    const withBoth = snapshot([a, b]);
    const back = undo(record(emptyHistory(), snapshot([a])), withBoth);
    expect(back?.shapes.length).toBe(1);
    const forward = redo(back!.history, back!.shapes);
    expect(forward?.shapes.length).toBe(2);
    expect(forward?.shapes[1].id).toBe(b.id);
  });
});

describe("constrain", () => {
  it("snaps a line to 15° steps and keeps its length", () => {
    const at = constrain("line", { x: 0, y: 0 }, { x: 100, y: 20 });
    const angle = (Math.atan2(at.y, at.x) * 180) / Math.PI;
    expect(angle).toBeCloseTo(15, 5);
    expect(Math.hypot(at.x, at.y)).toBeCloseTo(Math.hypot(100, 20), 5);
  });

  it("squares a box from the larger extent", () => {
    expect(constrain("rect", { x: 0, y: 0 }, { x: 100, y: 40 })).toEqual({ x: 100, y: 100 });
    expect(constrain("ellipse", { x: 100, y: 100 }, { x: 20, y: 40 })).toEqual({ x: 20, y: 20 });
  });
});

describe("ink", () => {
  it("clamps the taper to ±40% of the chosen width", () => {
    expect(taperWidth(4, 0, null)).toBeCloseTo(5.6, 5);
    expect(taperWidth(4, 100, null)).toBeCloseTo(2.4, 5);
    expect(taperWidth(4, 0, 1)).toBeCloseTo(5.6, 5);
    expect(taperWidth(4, 0, 0)).toBeCloseTo(2.4, 5);
    expect(taperWidth(4, 0, 0.5)).toBeCloseTo(4, 5);
  });

  it("stabilises toward the pointer without overshooting", () => {
    expect(stabilize({ x: 0, y: 0 }, { x: 100, y: 0 }, 1)).toEqual({ x: 100, y: 0 });
    expect(stabilize({ x: 0, y: 0 }, { x: 100, y: 0 }, 0)).toEqual({ x: 0, y: 0 });
    expect(stabilize({ x: 0, y: 0 }, { x: 100, y: 50 })).toEqual({ x: 80, y: 40 });
  });
});

describe("exportSvg", () => {
  const three: Shape[] = [
    { kind: "rect", id: 1, color: "#1d1d1f", width: 2, a: { x: 20, y: 30 }, b: { x: 120, y: 90 } },
    { kind: "line", id: 2, color: "#e5484d", width: 3, a: { x: 20, y: 110 }, b: { x: 120, y: 110 } },
    { kind: "text", id: 3, color: "#1d1d1f", size: 20, at: { x: 20, y: 160 }, text: "Bard" },
  ];

  it("serialises a fixed three-shape array, with no background when there is none", () => {
    expect(exportSvg(three, { background: null })).toBe(
      [
        '<svg xmlns="http://www.w3.org/2000/svg" width="132" height="168" viewBox="4 14 132 168">',
        '<rect fill="none" stroke-linecap="round" stroke-linejoin="round" x="20" y="30" width="100" height="60" rx="6" stroke="#1d1d1f" stroke-width="2"/>',
        '<line fill="none" stroke-linecap="round" stroke-linejoin="round" x1="20" y1="110" x2="120" y2="110" stroke="#e5484d" stroke-width="3"/>',
        '<text x="20" y="160" font-size="20" fill="#1d1d1f" font-family="\'Segoe UI Variable Text\', \'Segoe UI\', system-ui, sans-serif">Bard</text>',
        "</svg>",
      ].join("\n"),
    );
  });

  it("paints the page colour as one background rect when it is the page", () => {
    const svg = exportSvg(three, { background: "#ffffff" });
    expect(svg).toContain('<rect x="4" y="14" width="132" height="168" fill="#ffffff"/>');
    expect(exportSvg(three, { background: null })).not.toContain('fill="#ffffff"');
  });

  it("draws the diamond as its four midpoints and escapes text", () => {
    const d: Shape = { kind: "diamond", id: 4, color: "#000000", width: 2, a: { x: 0, y: 0 }, b: { x: 40, y: 40 } };
    const svg = exportSvg([d]);
    expect(svg).toContain('points="20,0 40,20 20,40 0,20"');
    const t: Shape = { kind: "text", id: 5, color: "#000000", size: 12, at: { x: 0, y: 12 }, text: "a<b&c" };
    expect(exportSvg([t])).toContain(">a&lt;b&amp;c</text>");
  });

  it("exports a pen's taper as segmented paths, not a filled outline", () => {
    const p = pen([[0, 0], [10, 0], [20, 0]], 4, [1, 2, 3]);
    const svg = exportSvg([p]);
    expect(svg).toContain("<path");
    // One segment per width in the taper, quantised; both are strokes.
    expect(svg).toContain('stroke-width="2"');
    expect(svg).toContain('stroke-width="3"');
    expect(svg).not.toContain('fill="#1d1d1f"');
  });
});
