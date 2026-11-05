import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowRight,
  ChevronDown,
  Circle,
  CircleHelp,
  Diamond,
  Download,
  Eraser,
  Maximize2,
  Minimize2,
  Minus,
  MousePointer2,
  Pen,
  Redo2,
  Square,
  Type,
  Undo2,
  X,
} from "lucide-react";
import ResizeHandles from "../components/ResizeHandles";
import ShortcutSheet from "../components/ShortcutSheet";
import { api } from "../store";
import { useTheme } from "../useTheme";
import { DRAW_CANVAS, drawCanvasFor, inkFor, isHexColor, type BackgroundMode } from "../appearance";
import { DRAW_SHORTCUT_GROUPS } from "./shortcuts";
import {
  bounds,
  constrain,
  duplicate,
  emptyHistory,
  ERASE_SAMPLE_STEP,
  eraseCrossed,
  eraseTargets,
  exportPng,
  exportSvg,
  handleAt,
  hitTest,
  INKS,
  move,
  newId,
  record,
  redo,
  render,
  resize,
  snapshot,
  stabilize,
  taperWidth,
  undo,
  type HandleId,
  type History,
  type Point,
  type Shape,
  type Tool,
} from "./shapes";

const win = getCurrentWindow();

const WIDTHS = [2, 4, 8];

/** Ctrl+D and Alt+drag land the copy this far from the original, so it reads as one. */
const DUPLICATE_STEP = 12;

/*
  One icon family. The seven tools used to be hand-drawn inline SVG at a
  different stroke weight from the Lucide Undo and Redo beside them, in the
  same 28px button: two icon systems, one toolbar. The diamond and the eraser
  are Lucide like the rest.
*/
const TOOLS: { tool: Tool; label: string; key: string; Icon: typeof MousePointer2 }[] = [
  { tool: "select", label: "Select", key: "V", Icon: MousePointer2 },
  { tool: "pen", label: "Pen", key: "P", Icon: Pen },
  { tool: "line", label: "Line", key: "L", Icon: Minus },
  { tool: "arrow", label: "Arrow", key: "A", Icon: ArrowRight },
  { tool: "rect", label: "Rectangle", key: "R", Icon: Square },
  { tool: "ellipse", label: "Ellipse", key: "O", Icon: Circle },
  { tool: "diamond", label: "Diamond", key: "D", Icon: Diamond },
  { tool: "text", label: "Text", key: "T", Icon: Type },
  { tool: "eraser", label: "Eraser", key: "E", Icon: Eraser },
];

/** Every key that picks a tool, letter and digit alike. */
const TOOL_KEYS: Record<string, Tool> = {
  v: "select",
  "1": "select",
  p: "pen",
  "2": "pen",
  l: "line",
  "3": "line",
  a: "arrow",
  "4": "arrow",
  r: "rect",
  "5": "rect",
  o: "ellipse",
  "6": "ellipse",
  d: "diamond",
  "7": "diamond",
  t: "text",
  "8": "text",
  e: "eraser",
  "9": "eraser",
};

/** The three ways a finished drawing leaves the window. */
type SaveAction = "send" | "keep" | "copy";

const CURSORS: Record<HandleId, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  n: "ns-resize",
  s: "ns-resize",
  e: "ew-resize",
  w: "ew-resize",
  a: "pointer",
  b: "pointer",
};

/** Rust's refusals are written for a person; a rejected invoke is not. */
function reasonText(reason: unknown, fallback: string): string {
  const detail = reason instanceof Error ? reason.message : String(reason ?? "");
  return detail.trim() || fallback;
}

/** A File from the DOM as the data URL a shape stores. */
function readImageFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("That picture could not be opened."));
    reader.readAsDataURL(file);
  });
}

/**
 * A picture is decoded the moment it is dropped or seeded and kept in a cache,
 * so every later paint and export is synchronous. `decode()` is the stronger
 * promise that the pixels themselves are ready; where an engine has no decode,
 * the load event is the guarantee.
 */
async function decodeImage(src: string): Promise<HTMLImageElement> {
  const img = new Image();
  await new Promise<void>((resolve, reject) => {
    img.onload = () => resolve();
    img.onerror = () => reject(new Error("That picture could not be opened."));
    img.src = src;
  });
  if (typeof img.decode === "function") {
    await img.decode().catch(() => undefined);
  }
  return img;
}

/*
  The swatches are drawing inks, and the first of them is the page's own ink:
  dark on a light page, light on a dark one, so a stroke in ink stays readable
  when the page turns. The hues beside it are the theme's own accent, danger,
  success and warning read at render time — the toolbar used to hold the light
  theme's values as literals, which put a light blue swatch beside a dark
  theme's blue selection ring, two blues 2px apart.
*/
const HUE_FALLBACK = ["#2f6fed", "#e5484d", "#2c9e5b", "#e8a33d"] as const;

function tokenInk(property: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(property).trim();
  return value || fallback;
}

/** The theme's inks, re-read whenever the resolved theme can have changed. */
function useHues(themeKey: string): string[] {
  const [hues, setHues] = useState<string[]>([...HUE_FALLBACK]);
  useEffect(() => {
    // Runs after `useTheme`'s own effect has written the new theme, so this
    // reads the tokens the window is actually painted with.
    setHues([
      tokenInk("--accent", HUE_FALLBACK[0]),
      tokenInk("--danger", HUE_FALLBACK[1]),
      tokenInk("--success", HUE_FALLBACK[2]),
      tokenInk("--warning", HUE_FALLBACK[3]),
    ]);
  }, [themeKey]);
  return hues;
}

/*
  The canvas is driven imperatively.

  Shapes live in a ref and the canvas is repainted by hand, because a stroke
  produces a pointer sample every few milliseconds and routing each one through
  React state would re-render the whole window per sample for no benefit.
  React state holds only what the chrome shows: the tool, the colour, the
  width, and which shape is selected.

  History is a list of whole states, not a stack of shapes: the snapshot taken
  before each committed change is what undo restores, so an erased shape comes
  back in its own place and order and a redo is a state, not a replay.
*/
export default function DrawWindow() {
  // Draw's window wears the theme and the panel's background like every other
  // window; its page is the one surface with a colour of its own.
  const appearance = useTheme();
  const [canvasPreview, setCanvasPreview] = useState<string | null>(null);
  const page = canvasPreview ?? (appearance ? drawCanvasFor(appearance) : DRAW_CANVAS.light);
  const ink = inkFor(page);
  const hues = useHues(`${appearance?.theme ?? ""}:${appearance?.systemDark ?? ""}`);
  const colors = [ink, ...hues];
  const accent = hues[0];

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const shapesRef = useRef<Shape[]>([]);
  const draftRef = useRef<Shape | null>(null);
  const dragRef = useRef<{ id: number; last: Point; changed: boolean } | null>(null);
  const resizeRef = useRef<{ id: number; handle: HandleId; changed: boolean } | null>(null);
  /** The state before the gesture in progress; recorded when the pointer lifts. */
  const pendingRef = useRef<Shape[] | null>(null);
  const historyRef = useRef<History>(emptyHistory());
  const [historyCounts, setHistoryCounts] = useState({ undo: 0, redo: 0 });
  /** The eraser's sweep, its current targets, and the ring it paints at the pointer. */
  const erasePathRef = useRef<Point[] | null>(null);
  const ghostRef = useRef<number[] | null>(null);
  const eraserRef = useRef<{ at: Point; r: number } | null>(null);
  /** The last raw sample and its time, for the pen's taper and its tail. */
  const sampleRef = useRef<{ at: Point; t: number; width: number } | null>(null);
  /** Decoded pictures by data URL; a shape is only added once its image is here. */
  const imagesRef = useRef(new Map<string, HTMLImageElement>());
  /** Pictures still decoding; every export waits for them rather than missing them. */
  const loadingRef = useRef(0);

  const [tool, setTool] = useState<Tool>("pen");
  const [color, setColor] = useState(INKS[0]);
  const [customColor, setCustomColor] = useState("#8e5cf6");
  const [canvasOpen, setCanvasOpen] = useState(false);
  const canvasWell = useRef<HTMLInputElement>(null);
  const canvasChoice = useRef<HTMLDivElement>(null);
  const penCustom = !colors.includes(color);
  /** The colour a stroke would actually take: an ink follows the page. */
  const paintColor = INKS.includes(color) ? ink : color;

  // The ink follows the page: a stroke in ink stays readable when the page
  // turns from light to dark, while a chosen hue is left exactly as chosen.
  useEffect(() => {
    setColor((current) => (INKS.includes(current) ? ink : current));
  }, [ink]);
  const [width, setWidth] = useState(WIDTHS[1]);
  const [selected, setSelected] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [empty, setEmpty] = useState(true);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [discard, setDiscard] = useState(false);
  const [textField, setTextField] = useState<{ at: Point; left: number; top: number; value: string } | null>(null);
  const textSize = Math.max(14, width * 5);
  const [savedOnce, setSavedOnce] = useState(false);
  const savedIdRef = useRef<number | null>(null);
  /** A ref beside the state: a double Enter must not save twice in one frame. */
  const savingRef = useRef(false);
  const [help, setHelp] = useState(false);

  /**
   * A drawing that has not been saved. Save puts it in Bard and closes the
   * window, so this is "there is something on the page that Bard has not got"
   * — and the two ways out of a dirty window have to ask first.
   */
  const dirty = !empty && !savedOnce;
  const dirtyRef = useRef(dirty);

  useEffect(() => {
    dirtyRef.current = dirty;
  }, [dirty]);

  /**
   * Anything that changes the page un-saves it. The id remembered from the last
   * save no longer describes what is on the page, so a Send that reused it
   * would paste the drawing as it was, not as it is — and a close would stop
   * having to ask, throwing the change away. Every mutation calls this, and a
   * save only marks the page clean once Bard has answered with an id.
   */
  const markEdited = () => {
    savedIdRef.current = null;
    setSavedOnce(false);
  };

  const paint = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    const all = draftRef.current ? [...shapesRef.current, draftRef.current] : shapesRef.current;
    render(ctx, all, w, h, {
      dpr,
      background: page,
      ink,
      selected,
      accent,
      images: imagesRef.current,
      ghost: ghostRef.current,
      eraser: eraserRef.current,
    });
  }, [selected, page, ink, accent]);

  /*
    One paint per frame, however many pointer events land in it.
    A 120 Hz mouse or a drawing tablet produced an event per sample and every
    one of them re-rendered the whole page — every shape, every pen point, from
    scratch. The state the paint reads is already up to date when the frame
    runs, so the frame shows the same picture the last of those events would
    have drawn, and everything between the frames costs nothing.
  */
  const paintRef = useRef(paint);
  paintRef.current = paint;
  const frameRef = useRef<number | undefined>(undefined);
  const schedulePaint = useCallback(() => {
    if (frameRef.current !== undefined) return;
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = undefined;
      paintRef.current();
    });
  }, []);

  useEffect(
    () => () => {
      if (frameRef.current !== undefined) window.cancelAnimationFrame(frameRef.current);
    },
    [],
  );

  // Browser preview only: a seeded sketch, so screenshots show Draw in use.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const seed = (window as { __bardPreviewShapes?: Shape[] }).__bardPreviewShapes;
    if (seed && shapesRef.current.length === 0) {
      shapesRef.current = snapshot(seed);
      setEmpty(seed.length === 0);
      paint();
    }
  }, [paint]);

  useEffect(() => {
    paint();
    const onResize = () => paint();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [paint]);

  // The tool/colour/width the person last used are the ones Draw opens with;
  // they come from the appearance record and go back to it as they change.
  const prefsLoaded = useRef(false);
  useEffect(() => {
    if (!appearance || prefsLoaded.current) return;
    prefsLoaded.current = true;
    if (appearance.drawTool && TOOLS.some((t) => t.tool === appearance.drawTool)) {
      setTool(appearance.drawTool as Tool);
    }
    if (appearance.drawColor && isHexColor(appearance.drawColor)) {
      setColor(appearance.drawColor.toLowerCase());
    }
    if (appearance.drawWidth != null && WIDTHS.includes(appearance.drawWidth)) {
      setWidth(appearance.drawWidth);
    }
  }, [appearance]);

  useEffect(() => {
    if (!prefsLoaded.current) return;
    // Debounced: the custom colour well fires on every pixel of the picker.
    const id = window.setTimeout(() => {
      void api.setDrawPrefs(tool, color, width).catch(() => undefined);
    }, 400);
    return () => window.clearTimeout(id);
  }, [tool, color, width]);

  const syncHistory = () => {
    setHistoryCounts({ undo: historyRef.current.past.length, redo: historyRef.current.future.length });
  };

  /** Record a state, taken before the change it undoes. */
  const pushHistory = (before: Shape[]) => {
    // Every caller is a change to the page, so the saved copy is stale from
    // here until a save replaces it — see `markEdited`.
    markEdited();
    historyRef.current = record(historyRef.current, before);
    syncHistory();
  };

  const commit = (s: Shape | null) => {
    if (s) {
      pushHistory(snapshot(shapesRef.current));
      shapesRef.current = [...shapesRef.current, s];
      setEmpty(false);
    }
    draftRef.current = null;
    paint();
  };

  /** A gesture's snapshot, held until the pointer lifts and the change is real. */
  const beginGesture = () => {
    pendingRef.current = snapshot(shapesRef.current);
  };

  const endGesture = (changed: boolean) => {
    const before = pendingRef.current;
    pendingRef.current = null;
    if (before && changed) pushHistory(before);
  };

  const toggleFullscreen = useCallback(async () => {
    const next = !fullscreen;
    try {
      await win.setFullscreen(next);
      setFullscreen(next);
    } catch {
      // A window manager may reject a fullscreen transition while it is
      // animating; the next F11 or toolbar click can retry it.
    }
  }, [fullscreen]);

  useEffect(() => {
    let alive = true;
    void win.isFullscreen().then((value) => {
      if (alive) setFullscreen(value);
    }).catch(() => undefined);
    const pending = win.onResized(() => {
      void win.isFullscreen().then((value) => {
        if (alive) setFullscreen(value);
      }).catch(() => undefined);
    });
    return () => {
      alive = false;
      void pending.then((unlisten) => unlisten());
    };
  }, []);

  const at = (e: React.PointerEvent): Point => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  /**
   * Open the text tool's field where the click landed. The shape's own point is
   * canvas-relative; the field's is relative to the surface the field is a
   * child of, so the two are kept apart rather than converted at commit time.
   */
  const openTextField = (p: Point) => {
    const pageBox = canvasRef.current?.getBoundingClientRect();
    const surface = surfaceRef.current?.getBoundingClientRect();
    setTextField({
      at: p,
      left: p.x + (pageBox?.left ?? 0) - (surface?.left ?? 0),
      top: p.y + (pageBox?.top ?? 0) - (surface?.top ?? 0),
      value: "",
    });
  };

  /** Enter, or clicking away with something typed, keeps the text. */
  const commitText = () => {
    const draft = textField;
    setTextField(null);
    if (!draft) return;
    const text = draft.value.trim();
    if (!text) return;
    commit({ kind: "text", id: newId(), color, size: textSize, at: draft.at, text });
  };

  const cancelText = () => setTextField(null);

  const textInputRef = useRef<HTMLInputElement>(null);
  const textFieldOpen = textField !== null;
  useEffect(() => {
    if (!textFieldOpen) return;
    const id = window.setTimeout(() => textInputRef.current?.focus(), 0);
    return () => window.clearTimeout(id);
  }, [textFieldOpen, textField?.left, textField?.top]);

  const chooseTool = (next: Tool) => {
    setTool(next);
    if (next !== "select") setSelected(null);
    if (next !== "eraser") {
      ghostRef.current = null;
      eraserRef.current = null;
    }
    canvasRef.current?.style.removeProperty("cursor");
    paint();
  };

  /** The select tool's pointer: a grip, a shape to move, or the page. */
  const updateCursor = (p: Point) => {
    const canvas = canvasRef.current;
    if (!canvas || tool !== "select" || dragRef.current || resizeRef.current) return;
    const chosen = shapesRef.current.find((s) => s.id === selected);
    const grip = chosen ? handleAt(chosen, p) : null;
    canvas.style.cursor = grip ? CURSORS[grip] : hitTest(shapesRef.current, p) ? "move" : "default";
  };

  const onPointerDown = (e: React.PointerEvent) => {
    const p = at(e);

    // Before the capture: a field is a field, and the pointer belongs to it.
    // The click's own default action would move focus to the canvas a moment
    // after the field mounted, blurring it while still empty, and an empty
    // blur closes the field: the tool looked dead. Stop that default, and
    // focus the field once it exists.
    if (tool === "text") {
      e.preventDefault();
      if (textField) commitText();
      openTextField(p);
      return;
    }

    // A toolbar button that still holds focus would answer the next Enter or
    // letter itself; drawing takes the focus back the moment it starts.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();

    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);

    if (tool === "eraser") {
      // The snapshot is taken now; the delete is recorded when the pointer
      // lifts, so the whole sweep is one undo away.
      beginGesture();
      erasePathRef.current = [p];
      ghostRef.current = eraseTargets(shapesRef.current, erasePathRef.current);
      eraserRef.current = { at: p, r: width + 4 };
      paint();
      return;
    }

    if (tool === "select") {
      const chosen = shapesRef.current.find((s) => s.id === selected);
      const grip = chosen ? handleAt(chosen, p) : null;
      if (grip && chosen) {
        beginGesture();
        resizeRef.current = { id: chosen.id, handle: grip, changed: false };
        return;
      }
      const hit = hitTest(shapesRef.current, p);
      if (!hit) {
        setSelected(null);
        dragRef.current = null;
        return;
      }
      if (e.altKey) {
        // Alt+drag: the original stays, the copy follows the pointer.
        beginGesture();
        const copy = duplicate(hit);
        shapesRef.current = [...shapesRef.current, copy];
        setEmpty(false);
        setSelected(copy.id);
        dragRef.current = { id: copy.id, last: p, changed: true };
        paint();
        return;
      }
      setSelected(hit.id);
      beginGesture();
      dragRef.current = { id: hit.id, last: p, changed: false };
      return;
    }

    const id = newId();
    if (tool === "pen") {
      const start = taperWidth(width, 0, null);
      draftRef.current = { kind: "pen", id, color, width, points: [p], widths: [start] };
      sampleRef.current = { at: { ...p }, t: e.timeStamp, width: start };
    } else {
      draftRef.current = { kind: tool, id, color, width, a: p, b: p };
    }
    paint();
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const p = at(e);

    if (tool === "eraser") {
      // Once the pointer is down every crossed shape ghosts; nothing is
      // removed until it lifts, so the page never changes under the sweep
      // that is still choosing. Hovering ghosts the one shape under the
      // pointer — the shape a click would take.
      if (erasePathRef.current) {
        const path = erasePathRef.current;
        const last = path[path.length - 1] ?? p;
        // A move shorter than the sweep's own sampling step cannot change what
        // it crosses, so it does not grow the path. Then only the new segment
        // is tested, against what the gesture has not already taken: the ghosts
        // only ever grow, so their union is the answer the old full re-test
        // produced, at a cost that does not grow with the gesture.
        if (Math.hypot(p.x - last.x, p.y - last.y) >= ERASE_SAMPLE_STEP) {
          erasePathRef.current = [...path, p];
          const crossed = eraseCrossed(shapesRef.current, last, p);
          if (crossed.length > 0) {
            const ghost = new Set(ghostRef.current ?? []);
            for (const id of crossed) ghost.add(id);
            ghostRef.current = [...ghost];
          }
        }
      } else {
        const under = hitTest(shapesRef.current, p);
        ghostRef.current = under ? [under.id] : null;
      }
      eraserRef.current = { at: p, r: width + 4 };
      schedulePaint();
      return;
    }

    if (resizeRef.current) {
      const s = shapesRef.current.find((x) => x.id === resizeRef.current!.id);
      if (s) {
        resize(s, resizeRef.current.handle, p, { shift: e.shiftKey, alt: e.altKey });
        resizeRef.current.changed = true;
        schedulePaint();
      }
      return;
    }

    if (dragRef.current) {
      const s = shapesRef.current.find((x) => x.id === dragRef.current!.id);
      if (s) {
        move(s, p.x - dragRef.current.last.x, p.y - dragRef.current.last.y);
        dragRef.current.last = p;
        dragRef.current.changed = true;
        schedulePaint();
      }
      return;
    }

    if (tool === "select") {
      updateCursor(p);
      return;
    }

    const draft = draftRef.current;
    if (!draft) return;
    if (draft.kind === "pen") {
      const sample = sampleRef.current;
      if (!sample) return;
      const dt = Math.max(1, e.timeStamp - sample.t);
      const speed = Math.hypot(p.x - sample.at.x, p.y - sample.at.y) / dt;
      // Pressure is a pen's own signal; a mouse or a trackpad gets the taper.
      const pressure = e.pointerType === "pen" && e.pressure > 0 ? e.pressure : null;
      const settled = stabilize(draft.points[draft.points.length - 1], p);
      const nextWidth = taperWidth(width, speed, pressure);
      draft.points.push(settled);
      draft.widths!.push(nextWidth);
      sampleRef.current = { at: { ...p }, t: e.timeStamp, width: nextWidth };
    } else if (
      draft.kind === "line" ||
      draft.kind === "arrow" ||
      draft.kind === "rect" ||
      draft.kind === "ellipse" ||
      draft.kind === "diamond"
    ) {
      draft.b = e.shiftKey ? constrain(draft.kind, draft.a, p) : p;
    }
    schedulePaint();
  };

  const onPointerUp = () => {
    if (tool === "eraser") {
      const ids = ghostRef.current ?? [];
      erasePathRef.current = null;
      ghostRef.current = null;
      if (ids.length > 0) {
        endGesture(true);
        shapesRef.current = shapesRef.current.filter((s) => !ids.includes(s.id));
        setSelected(null);
        setEmpty(shapesRef.current.length === 0);
      } else {
        endGesture(false);
      }
      paint();
      return;
    }

    if (resizeRef.current || dragRef.current) {
      const changed = Boolean(resizeRef.current?.changed || dragRef.current?.changed);
      resizeRef.current = null;
      dragRef.current = null;
      endGesture(changed);
      return;
    }

    const draft = draftRef.current;
    if (!draft) return;
    if (draft.kind === "pen") {
      // End where the hand stopped: the raw sample the easing held back.
      const sample = sampleRef.current;
      const tail = draft.points[draft.points.length - 1];
      if (sample && Math.hypot(sample.at.x - tail.x, sample.at.y - tail.y) > 0.75) {
        draft.points.push({ ...sample.at });
        draft.widths!.push(sample.width);
      }
    }
    // Discard an accidental click: a zero-area shape is a mis-click, not a mark.
    const b = bounds(draft);
    const trivial = draft.kind === "pen" ? draft.points.length < 2 : b.w < 2 && b.h < 2;
    commit(trivial ? null : draft);
  };

  /** Let the pointer leave without leaving a ring or a ghost behind. */
  const onPointerLeave = () => {
    if (erasePathRef.current) return;
    if (eraserRef.current || ghostRef.current) {
      eraserRef.current = null;
      ghostRef.current = null;
      paint();
    }
  };

  const undoOnce = () => {
    const step = undo(historyRef.current, shapesRef.current);
    if (!step) return;
    markEdited();
    historyRef.current = step.history;
    shapesRef.current = step.shapes;
    setSelected(null);
    setEmpty(step.shapes.length === 0);
    syncHistory();
    paint();
  };

  const redoOnce = () => {
    const step = redo(historyRef.current, shapesRef.current);
    if (!step) return;
    markEdited();
    historyRef.current = step.history;
    shapesRef.current = step.shapes;
    setSelected(null);
    setEmpty(step.shapes.length === 0);
    syncHistory();
    paint();
  };

  const remove = () => {
    if (selected == null) return;
    pushHistory(snapshot(shapesRef.current));
    shapesRef.current = shapesRef.current.filter((s) => s.id !== selected);
    setSelected(null);
    setEmpty(shapesRef.current.length === 0);
    paint();
  };

  const duplicateSelection = () => {
    if (selected == null) return;
    const chosen = shapesRef.current.find((s) => s.id === selected);
    if (!chosen) return;
    const copy = duplicate(chosen);
    move(copy, DUPLICATE_STEP, DUPLICATE_STEP);
    pushHistory(snapshot(shapesRef.current));
    shapesRef.current = [...shapesRef.current, copy];
    setEmpty(false);
    setSelected(copy.id);
    paint();
  };

  /**
   * A dropped, pasted or seeded picture is fitted to the page with an 8%
   * margin and never upscaled: a screenshot fills the page without bleeding
   * off it, and a small picture keeps its own pixels. It is decoded before it
   * becomes a shape, so hit-testing, painting and every export see real pixels
   * — and every export waits while one is still decoding.
   */
  const addImageFromSource = useCallback(
    async (src: string) => {
      loadingRef.current += 1;
      try {
        const img = await decodeImage(src);
        imagesRef.current.set(src, img);
        const canvas = canvasRef.current;
        const pageW = canvas?.clientWidth ?? 0;
        const pageH = canvas?.clientHeight ?? 0;
        const natW = img.naturalWidth || 1;
        const natH = img.naturalHeight || 1;
        const scale = pageW > 0 && pageH > 0 ? Math.min(1, (pageW * 0.84) / natW, (pageH * 0.84) / natH) : 1;
        const w = natW * scale;
        const h = natH * scale;
        const a = { x: (pageW - w) / 2, y: (pageH - h) / 2 };
        pushHistory(snapshot(shapesRef.current));
        shapesRef.current = [
          ...shapesRef.current,
          { kind: "image", id: newId(), src, a, b: { x: a.x + w, y: a.y + h }, natW, natH },
        ];
        setEmpty(false);
        paint();
      } catch (reason) {
        setSaveError(reasonText(reason, "That picture could not be opened."));
      } finally {
        loadingRef.current -= 1;
      }
    },
    [paint],
  );

  /*
    A drop from anywhere on the window is the same as the panel's: image files
    become pictures on the page. Anything else is refused in a sentence rather
    than swallowed, because a drop that does nothing reads as a broken window.
  */
  useEffect(() => {
    const onDragOver = (e: DragEvent) => {
      if (e.dataTransfer?.types.includes("Files")) {
        e.preventDefault();
        if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      }
    };
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      const files = [...(e.dataTransfer?.files ?? [])];
      if (files.length === 0) return;
      const picture = files.find((file) => file.type.startsWith("image/"));
      if (!picture) {
        setSaveError("Only pictures can be dropped onto a drawing.");
        return;
      }
      void readImageFile(picture)
        .then(addImageFromSource)
        .catch((reason: unknown) => setSaveError(reasonText(reason, "That picture could not be opened.")));
    };
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragover", onDragOver);
      window.removeEventListener("drop", onDrop);
    };
  }, [addImageFromSource]);

  // Annotate: the panel asks Rust to open Draw over an image item, and Rust
  // announces it with `draw-seed`. The pixels come from the item, exactly as
  // the panel's cover does.
  useEffect(() => {
    let alive = true;
    const pending = listen<{ itemId?: number }>("draw-seed", (event) => {
      const itemId = event.payload?.itemId;
      if (typeof itemId !== "number") return;
      void api
        .itemImageData(itemId)
        .then((src) => {
          if (!alive) return;
          if (!src) {
            setSaveError("That picture could not be opened.");
            return;
          }
          return addImageFromSource(src);
        })
        .catch((reason: unknown) => {
          if (alive) setSaveError(reasonText(reason, "That picture could not be opened."));
        });
    });
    return () => {
      alive = false;
      void pending.then((off) => off()).catch(() => undefined);
    };
  }, [addImageFromSource]);

  /*
    The clipboard picture is the one read the v2.1 contract leaves open: none
    of its four Rust commands (send_drawing, save_svg, open_draw_with_image,
    set_draw_prefs) carries clipboard pixels. `clipboard_image_data` in the
    store is the seam — expected to answer an image data URL, or null when the
    clipboard holds no picture. Drop and Annotate already seed pictures through
    addImageFromSource, so only this read is waiting.
  */
  const pasteImage = useCallback(async () => {
    try {
      const src = await api.clipboardImage();
      if (!src) return;
      await addImageFromSource(src);
    } catch {
      setSaveError("Couldn't read a picture from the clipboard.");
    }
  }, [addImageFromSource]);

  /**
   * The three ways out. Send also pastes into the app behind Bard, Keep only
   * leaves the drawing in Bard, Copy only puts it on the clipboard. Every one
   * first saves, and none of them closes on a failure: the real reason stays
   * on screen with the drawing still on the page.
   */
  const save = async (action: SaveAction) => {
    const canvas = canvasRef.current;
    // Read the shapes from the ref, not from `empty`: the key handler closes
    // over whichever render registered it, and a stale `empty` there silently
    // swallowed Ctrl+Enter.
    if (!canvas || savingRef.current || shapesRef.current.length === 0) return;
    if (loadingRef.current > 0) {
      setSaveError("Still loading the picture…");
      return;
    }
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    try {
      let id = savedIdRef.current;
      if (id == null) {
        const b64 = exportPng(shapesRef.current, canvas.clientWidth, canvas.clientHeight, 2, page, imagesRef.current);
        if (!b64) throw new Error("The page could not be rendered.");
        const created = await api.addImage(b64, "png");
        // Only an id means the drawing is really in Bard. A save that came back
        // with nothing has put it nowhere, so the window keeps the page and
        // closing still has to ask — the remembered id is set below, never
        // before the answer.
        if (!created) throw new Error("The drawing could not be saved to Bard.");
        id = created;
        savedIdRef.current = created;
        // The drawing is in Bard from here: it is no longer unsaved work, so
        // closing stops having to ask before throwing it away.
        setSavedOnce(true);
      }
      if ((action === "copy" || action === "send") && id) {
        try {
          await api.copyItem(id);
        } catch {
          // In Bard, not on the clipboard. Worth saying, and not worth a
          // second copy of the same drawing in the timeline if Send is pressed
          // again — hence the id remembered above.
          setSaveError("Saved to Bard, but Bard could not copy it to the clipboard.");
          return;
        }
      }
      if (action === "send") {
        try {
          await api.sendDrawing();
        } catch (reason) {
          setSaveError(reasonText(reason, "Saved to Bard, but Bard couldn't paste it."));
          return;
        }
      }
      await win.close();
    } catch (reason) {
      // Rust's refusal says what actually happened — a licence that ran out, a
      // file that could not be written — and the window keeps the drawing.
      setSaveError(reasonText(reason, "Couldn't save this drawing. Try again."));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  /** The SVG goes out through the native Save-As dialog; the page stays open. */
  const saveSvgFile = async () => {
    if (savingRef.current || shapesRef.current.length === 0) return;
    if (loadingRef.current > 0) {
      setSaveError("Still loading the picture…");
      return;
    }
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    try {
      const svg = exportSvg(shapesRef.current, { background: page });
      await api.saveSvg(svg, "Bard drawing.svg");
    } catch (reason) {
      setSaveError(reasonText(reason, "Couldn't save the SVG."));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  /** Set once the user has answered the discard question, so it is asked once. */
  const discardAgreed = useRef(false);

  const closeNow = useCallback(() => {
    discardAgreed.current = true;
    void win.close();
  }, []);

  /**
   * Both ways out of a dirty window — the × and Esc, and the window's own close
   * request — come through here, so a drawing is never thrown away without
   * being asked about first.
   */
  const requestClose = useCallback(() => {
    if (dirtyRef.current && !discardAgreed.current) setDiscard(true);
    else closeNow();
  }, [closeNow]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      const key = e.key;
      const lower = key.length === 1 ? key.toLowerCase() : key;

      // F11 is Draw's own while Draw has focus — the universal fullscreen key,
      // never taken from whichever app you are actually using.
      if (key === "F11") {
        e.preventDefault();
        void toggleFullscreen();
        return;
      }
      // The sheet and the text field own the keyboard while they are open;
      // each stops its own events, and this is the belt for a wandered focus.
      if (help || textField) return;
      if (key === "Escape") {
        // The chain, one step per press: the page menu, the discard question,
        // the selection, the tool, fullscreen, and then the way out.
        e.preventDefault();
        if (canvasOpen) setCanvasOpen(false);
        else if (discard) setDiscard(false);
        else if (selected != null) setSelected(null);
        else if (tool !== "select") chooseTool("select");
        else if (fullscreen) void toggleFullscreen();
        else requestClose();
        return;
      }
      const target = e.target as HTMLElement | null;
      // A field owns every key while it has focus; a focused button answers
      // Enter and Space itself, so plain keys go to it and modified shortcuts
      // still come here.
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
      if (!mod && target?.closest("button")) return;

      if (mod && lower === "z") {
        e.preventDefault();
        if (e.shiftKey) redoOnce();
        else undoOnce();
      } else if (mod && lower === "y") {
        e.preventDefault();
        redoOnce();
      } else if (mod && lower === "d") {
        e.preventDefault();
        duplicateSelection();
      } else if (mod && lower === "s") {
        e.preventDefault();
        void save("keep");
      } else if (mod && lower === "c") {
        e.preventDefault();
        void save("copy");
      } else if (mod && lower === "v") {
        e.preventDefault();
        void pasteImage();
      } else if (key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void save("send");
      } else if (key === "Delete" || key === "Backspace") {
        if (selected != null) {
          e.preventDefault();
          remove();
        }
      } else if (key === "?") {
        e.preventDefault();
        setHelp(true);
      } else if (!mod && !e.altKey && TOOL_KEYS[lower]) {
        e.preventDefault();
        chooseTool(TOOL_KEYS[lower]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, saving, page, fullscreen, canvasOpen, discard, textField, help, requestClose, toggleFullscreen]);

  // The taskbar, Alt+F4 and any other close request arrive here rather than
  // through the × in the header.
  useEffect(() => {
    const pending = win.onCloseRequested((event) => {
      if (discardAgreed.current || !dirtyRef.current) return;
      event.preventDefault();
      setDiscard(true);
    });
    return () => {
      void pending.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, []);

  /** A discard that has been answered yes: the next close request is honoured. */
  const confirmDiscard = closeNow;

  // The canvas choice closes when you click anywhere else.
  useEffect(() => {
    if (!canvasOpen) return;
    const onPointer = (event: PointerEvent) => {
      if (!canvasChoice.current?.contains(event.target as Node)) setCanvasOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [canvasOpen]);

  // A custom page previews live while the picker moves and is saved once,
  // when the picker closes.
  useEffect(() => {
    const input = canvasWell.current;
    if (!input) return;
    const commit = () => {
      if (!isHexColor(input.value)) return;
      void api.setDrawBackground("custom", input.value).finally(() => setCanvasPreview(null));
    };
    input.addEventListener("change", commit);
    return () => input.removeEventListener("change", commit);
  }, [canvasOpen]);

  const chooseCanvas = (mode: BackgroundMode) => {
    setCanvasPreview(null);
    void api.setDrawBackground(mode);
  };
  const canvasMode = appearance?.drawBackground ?? "theme";

  return (
    <div className={`draw-root companion${fullscreen ? " fullscreen" : ""}`}>
      {fullscreen ? null : <ResizeHandles />}
      {/* The header is the handle: an ordinary window can be moved by it. */}
      <header className="draw-head" data-tauri-drag-region>
        <div className="draw-head-lead" data-tauri-drag-region>
          <p className="companion-title" data-tauri-drag-region>
            Draw
            {dirty ? <span className="draw-dirty" title="Unsaved drawing" aria-hidden="true" /> : null}
          </p>
          <div className="canvas-choice" ref={canvasChoice}>
            <button
              type="button"
              className="canvas-toggle"
              aria-expanded={canvasOpen}
              aria-haspopup="true"
              title="Page colour"
              onClick={() => setCanvasOpen((open) => !open)}
            >
              <span className="canvas-chip" style={{ background: page }} aria-hidden="true" />
              Page
              <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
            </button>
            {canvasOpen ? (
              <div className="canvas-pop" role="group" aria-label="Page colour">
                <div className="canvas-options">
                  {(["light", "dark"] as const).map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      className={`canvas-opt${canvasMode === mode ? " on" : ""}`}
                      style={{ background: DRAW_CANVAS[mode] }}
                      aria-label={mode === "light" ? "Light canvas" : "Dark canvas"}
                      aria-pressed={canvasMode === mode}
                      title={mode === "light" ? "Light" : "Dark"}
                      onClick={() => chooseCanvas(mode)}
                    />
                  ))}
                  <label
                    className={`canvas-opt custom${canvasMode === "custom" ? " on" : ""}`}
                    style={canvasMode === "custom" ? { background: page } : undefined}
                    title="Custom"
                  >
                    <input
                      ref={canvasWell}
                      type="color"
                      aria-label="Custom canvas colour"
                      defaultValue={isHexColor(appearance?.drawBackgroundColor ?? "") ? appearance?.drawBackgroundColor : page}
                      onChange={(event) => setCanvasPreview(event.target.value)}
                    />
                  </label>
                </div>
                <p className="canvas-note">
                  {canvasMode === "theme" ? "Follows the theme until you choose." : "Chosen by you."}
                </p>
                {canvasMode === "theme" ? null : (
                  <button type="button" className="canvas-match" onClick={() => chooseCanvas("theme")}>
                    Match theme
                  </button>
                )}
              </div>
            ) : null}
          </div>
        </div>
        <div className="draw-head-actions">
          <button
            type="button"
            className="draw-head-action"
            aria-label={fullscreen ? "Exit fullscreen" : "Fullscreen"}
            aria-pressed={fullscreen}
            title={fullscreen ? "Exit fullscreen · F11" : "Fullscreen · F11"}
            onClick={() => void toggleFullscreen()}
          >
            {fullscreen ? (
              <Minimize2 aria-hidden="true" strokeWidth={1.8} />
            ) : (
              <Maximize2 aria-hidden="true" strokeWidth={1.8} />
            )}
          </button>
          <button
            type="button"
            className="draw-head-action"
            aria-label="Close"
            title="Close"
            onClick={requestClose}
          >
            <X aria-hidden="true" strokeWidth={2} />
          </button>
        </div>
      </header>
      <div className="draw-surface" ref={surfaceRef}>
        <canvas
          ref={canvasRef}
          className={`draw-canvas tool-${tool}`}
          style={{ background: page }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onPointerLeave={onPointerLeave}
        />
        {empty && !textField ? (
          <p className="draw-hint">
            Draw with a tool, or drop a picture onto the page.
          </p>
        ) : null}
        {textField ? (
          <input
            ref={textInputRef}
            className="draw-text-input"
            aria-label="Text on the drawing"
            value={textField.value}
            style={{
              left: textField.left,
              top: textField.top,
              color: paintColor,
              background: page,
              fontSize: `${textSize}px`,
            }}
            onChange={(event) =>
              setTextField((current) => (current ? { ...current, value: event.target.value } : current))
            }
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                event.stopPropagation();
                commitText();
              } else if (event.key === "Escape") {
                // Reached before the window handler, which would close Draw.
                event.preventDefault();
                event.stopPropagation();
                cancelText();
              } else {
                event.stopPropagation();
              }
            }}
            onBlur={commitText}
          />
        ) : null}
      </div>

      {saveError ? (
        <p className="draw-notice" role="alert">
          {saveError}
        </p>
      ) : null}
      {discard ? (
        <div className="draw-confirm" role="alert">
          <span>Discard this drawing?</span>
          <div className="draw-confirm-actions">
            <button type="button" className="btn" onClick={() => setDiscard(false)} autoFocus>
              Cancel
            </button>
            <button type="button" className="btn danger" onClick={confirmDiscard}>
              Discard
            </button>
          </div>
        </div>
      ) : null}

      {/*
        Two rows, always: the tools above; the inks, widths and the three
        actions below. The one row this replaces broke at the 480px floor.
      */}
      <div className="draw-toolbar">
        <div className="draw-row">
          <div className="draw-group">
            {TOOLS.map(({ tool: id, label, key, Icon }, index) => (
              <button
                key={id}
                className={`draw-tool${tool === id ? " on" : ""}`}
                title={`${label} · ${key} · ${index + 1}`}
                aria-label={label}
                aria-pressed={tool === id}
                onClick={() => chooseTool(id)}
              >
                <Icon size={16} strokeWidth={1.8} aria-hidden="true" />
              </button>
            ))}
          </div>

          <div className="draw-sep" />

          <div className="draw-group">
            <button
              className="draw-tool"
              onClick={undoOnce}
              disabled={historyCounts.undo === 0}
              title="Undo · Ctrl+Z"
              aria-label="Undo"
            >
              <Undo2 size={16} strokeWidth={1.8} aria-hidden="true" />
            </button>
            <button
              className="draw-tool"
              onClick={redoOnce}
              disabled={historyCounts.redo === 0}
              title="Redo · Ctrl+Shift+Z"
              aria-label="Redo"
            >
              <Redo2 size={16} strokeWidth={1.8} aria-hidden="true" />
            </button>
          </div>

          <div className="draw-spacer" />

          <button
            className="draw-tool"
            onClick={() => void saveSvgFile()}
            disabled={empty || saving}
            title="Export as SVG…"
            aria-label="Export as SVG"
          >
            <Download size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button
            className="draw-tool"
            onClick={() => setHelp(true)}
            aria-expanded={help}
            aria-haspopup="dialog"
            title="Shortcuts · ?"
            aria-label="Shortcuts"
          >
            <CircleHelp size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
        </div>

        <div className="draw-row">
          <div className="draw-group">
            {colors.map((c) => (
              <button
                key={c}
                className={`draw-swatch${color === c ? " on" : ""}`}
                style={{ ["--swatch" as string]: c }}
                title={c === ink ? "Ink" : c}
                aria-label={c === ink ? "Ink" : `Colour ${c}`}
                aria-pressed={color === c}
                onClick={() => setColor(c)}
              />
            ))}
            <label
              className={`draw-swatch custom${penCustom ? " on" : ""}`}
              style={{ ["--swatch" as string]: customColor }}
              title="Custom colour"
            >
              <input
                type="color"
                aria-label="Custom pen colour"
                value={customColor}
                onChange={(event) => {
                  setCustomColor(event.target.value);
                  setColor(event.target.value);
                }}
              />
            </label>
          </div>

          <div className="draw-sep" />

          <div className="draw-group">
            {WIDTHS.map((w) => (
              <button
                key={w}
                className={`draw-width${width === w ? " on" : ""}`}
                title={`${w}px`}
                aria-label={`Width ${w}`}
                aria-pressed={width === w}
                onClick={() => setWidth(w)}
              >
                <span style={{ height: w }} />
              </button>
            ))}
          </div>

          <div className="draw-spacer" />

          <button
            className="draw-text-btn"
            onClick={() => void save("copy")}
            disabled={empty || saving}
            title="Save to Bard and copy to the clipboard · Ctrl+C"
          >
            Copy
          </button>
          <button
            className="draw-text-btn"
            onClick={() => void save("keep")}
            disabled={empty || saving}
            title="Save to Bard · Ctrl+S"
          >
            Keep
          </button>
          <button
            className="draw-primary"
            onClick={() => void save("send")}
            disabled={empty || saving}
            title="Save to Bard and paste it into the app behind · Enter"
          >
            Send
          </button>
        </div>
      </div>

      {help ? (
        <ShortcutSheet title="Draw shortcuts" groups={DRAW_SHORTCUT_GROUPS} onClose={() => setHelp(false)} />
      ) : null}
    </div>
  );
}
