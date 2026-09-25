import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Redo2, Undo2, X } from "lucide-react";
import { api } from "../store";
import { useTheme } from "../useTheme";
import { DRAW_CANVAS, drawCanvasFor, inkFor, isHexColor, type BackgroundMode } from "../appearance";
import {
  bounds,
  exportPng,
  hitTest,
  INKS,
  move,
  newId,
  render,
  type Point,
  type Shape,
  type Tool,
} from "./shapes";

const win = getCurrentWindow();

/** The first swatch is the ink: dark on a light page, light on a dark one. */
const HUES = ["#2f6fed", "#e5484d", "#2c9e5b", "#e8a33d"];
const WIDTHS = [2, 4, 8];

const TOOLS: { tool: Tool; label: string; icon: JSX.Element }[] = [
  { tool: "select", label: "Select", icon: <path d="M4 3l8.5 6.2-3.7.6L11 14l-1.7.8-1.9-3.7L4.6 13z" /> },
  { tool: "pen", label: "Draw", icon: <path d="M3 13l1-3.2 6.4-6.4a1.3 1.3 0 0 1 1.9 0l.3.3a1.3 1.3 0 0 1 0 1.9L6.2 12 3 13z" /> },
  { tool: "line", label: "Line", icon: <path d="M3.5 12.5L12.5 3.5" /> },
  { tool: "arrow", label: "Arrow", icon: <path d="M3.5 12.5L12.5 3.5M12.5 3.5H8M12.5 3.5V8" /> },
  { tool: "rect", label: "Rectangle", icon: <rect x="3" y="4" width="10" height="8" rx="1.5" /> },
  { tool: "ellipse", label: "Ellipse", icon: <ellipse cx="8" cy="8" rx="5.2" ry="4.2" /> },
  { tool: "text", label: "Text", icon: <path d="M4 4h8M8 4v9" /> },
];

/*
  The canvas is driven imperatively.

  Shapes live in a ref and the canvas is repainted by hand, because a stroke
  produces a pointer sample every few milliseconds and routing each one through
  React state would re-render the whole window per sample for no benefit.
  React state holds only what the chrome shows: the tool, the colour, the
  width, and which shape is selected.
*/
export default function DrawWindow() {
  // Draw's window wears the theme and the panel's background like every other
  // window; its page is the one surface with a colour of its own.
  const appearance = useTheme();
  const [canvasPreview, setCanvasPreview] = useState<string | null>(null);
  const page = canvasPreview ?? (appearance ? drawCanvasFor(appearance) : DRAW_CANVAS.light);
  const ink = inkFor(page);
  const colors = [ink, ...HUES];

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const shapesRef = useRef<Shape[]>([]);
  const draftRef = useRef<Shape | null>(null);
  const dragRef = useRef<{ id: number; last: Point } | null>(null);
  const undoneRef = useRef<Shape[]>([]);

  const [tool, setTool] = useState<Tool>("pen");
  const [color, setColor] = useState(INKS[0]);
  const [customColor, setCustomColor] = useState("#8e5cf6");
  const [canvasOpen, setCanvasOpen] = useState(false);
  const canvasWell = useRef<HTMLInputElement>(null);
  const canvasChoice = useRef<HTMLDivElement>(null);
  const penCustom = !colors.includes(color);

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
    render(ctx, all, w, h, { dpr, background: page, ink, selected });
  }, [selected, page, ink]);

  useEffect(() => {
    paint();
    const onResize = () => paint();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [paint]);

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

  const commit = (s: Shape | null) => {
    if (s) {
      shapesRef.current = [...shapesRef.current, s];
      undoneRef.current = [];
      setEmpty(false);
    }
    draftRef.current = null;
    paint();
  };

  const at = (e: React.PointerEvent): Point => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const onPointerDown = (e: React.PointerEvent) => {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const p = at(e);

    if (tool === "select") {
      const hit = hitTest(shapesRef.current, p);
      setSelected(hit?.id ?? null);
      dragRef.current = hit ? { id: hit.id, last: p } : null;
      return;
    }

    if (tool === "text") {
      const text = window.prompt("Text");
      if (text && text.trim()) {
        commit({ kind: "text", id: newId(), color, size: Math.max(14, width * 5), at: p, text: text.trim() });
      }
      return;
    }

    const id = newId();
    draftRef.current =
      tool === "pen"
        ? { kind: "pen", id, color, width, points: [p] }
        : { kind: tool, id, color, width, a: p, b: p };
    paint();
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const p = at(e);

    if (dragRef.current) {
      const s = shapesRef.current.find((x) => x.id === dragRef.current!.id);
      if (s) {
        move(s, p.x - dragRef.current.last.x, p.y - dragRef.current.last.y);
        dragRef.current.last = p;
        paint();
      }
      return;
    }

    const draft = draftRef.current;
    if (!draft) return;
    if (draft.kind === "pen") draft.points.push(p);
    else if (draft.kind !== "text") draft.b = p;
    paint();
  };

  const onPointerUp = () => {
    dragRef.current = null;
    const draft = draftRef.current;
    if (!draft) return;
    // Discard an accidental click: a zero-area shape is a mis-click, not a mark.
    const b = bounds(draft);
    const trivial =
      draft.kind === "pen" ? draft.points.length < 2 : b.w < 2 && b.h < 2;
    commit(trivial ? null : draft);
  };

  const undo = () => {
    const last = shapesRef.current[shapesRef.current.length - 1];
    if (!last) return;
    shapesRef.current = shapesRef.current.slice(0, -1);
    undoneRef.current.push(last);
    setSelected(null);
    setEmpty(shapesRef.current.length === 0);
    paint();
  };

  const redo = () => {
    const s = undoneRef.current.pop();
    if (!s) return;
    shapesRef.current = [...shapesRef.current, s];
    setEmpty(false);
    paint();
  };

  const remove = () => {
    if (selected == null) return;
    shapesRef.current = shapesRef.current.filter((s) => s.id !== selected);
    setSelected(null);
    setEmpty(shapesRef.current.length === 0);
    paint();
  };

  const save = async (alsoCopy: boolean) => {
    const canvas = canvasRef.current;
    if (!canvas || saving) return;
    setSaving(true);
    try {
      const b64 = exportPng(shapesRef.current, canvas.clientWidth, canvas.clientHeight, 2, page);
      if (b64) {
        const id = await api.addImage(b64, "png");
        if (alsoCopy && id) await api.copyItem(id);
      }
      await win.close();
    } finally {
      setSaving(false);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      // F11 is Draw's own while Draw has focus — the universal fullscreen key,
      // never taken from whichever app you are actually using.
      if (e.key === "F11") {
        e.preventDefault();
        void toggleFullscreen();
        return;
      }
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        e.shiftKey ? redo() : undo();
      } else if (e.key === "Delete" || e.key === "Backspace") {
        if (selected != null) {
          e.preventDefault();
          remove();
        }
      } else if (e.key === "Escape") {
        if (canvasOpen) setCanvasOpen(false);
        else if (selected != null) setSelected(null);
        else if (fullscreen) void toggleFullscreen();
        else void win.close();
      } else if (mod && e.key === "Enter") {
        e.preventDefault();
        void save(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, saving, fullscreen, canvasOpen, toggleFullscreen]);

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
      {/* The header is the handle: an ordinary window can be moved by it. */}
      <header className="draw-head" data-tauri-drag-region>
        <div className="draw-head-lead" data-tauri-drag-region>
          <p className="companion-title" data-tauri-drag-region>Draw</p>
          <div className="canvas-choice" ref={canvasChoice}>
            <button
              type="button"
              className="canvas-toggle"
              aria-expanded={canvasOpen}
              aria-haspopup="true"
              title="Canvas"
              onClick={() => setCanvasOpen((open) => !open)}
            >
              <span className="canvas-chip" style={{ background: page }} aria-hidden="true" />
              Canvas
            </button>
            {canvasOpen ? (
              <div className="canvas-pop" role="group" aria-label="Canvas">
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
            onClick={() => void win.close()}
          >
            <X aria-hidden="true" strokeWidth={2} />
          </button>
        </div>
      </header>
      <div className="draw-surface">
        <canvas
          ref={canvasRef}
          className={`draw-canvas tool-${tool}`}
          style={{ background: page }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        />
      </div>

      <div className="draw-toolbar">
        <div className="draw-group">
          {TOOLS.map((t) => (
            <button
              key={t.tool}
              className={`draw-tool${tool === t.tool ? " on" : ""}`}
              title={t.label}
              aria-label={t.label}
              aria-pressed={tool === t.tool}
              onClick={() => {
                setTool(t.tool);
                if (t.tool !== "select") setSelected(null);
              }}
            >
              <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"
                   fill="none" stroke="currentColor" strokeWidth="1.4"
                   strokeLinecap="round" strokeLinejoin="round">
                {t.icon}
              </svg>
            </button>
          ))}
        </div>

        <div className="draw-sep" />

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

        <div className="draw-group">
          <button className="draw-tool" onClick={undo} disabled={empty} title="Undo · Ctrl+Z" aria-label="Undo">
            <Undo2 size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button className="draw-tool" onClick={redo} title="Redo · Ctrl+Shift+Z" aria-label="Redo">
            <Redo2 size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
        </div>

        <div className="draw-sep" />

        {/* Copy saves too: the drawing lands in the timeline either way. */}
        <button
          className="draw-text-btn"
          onClick={() => void save(true)}
          disabled={empty || saving}
          title="Save to Bard and copy to the clipboard"
        >
          Copy
        </button>
        <button
          className="draw-primary"
          onClick={() => void save(false)}
          disabled={empty || saving}
          title="Save to Bard · Ctrl+Enter"
        >
          Save
        </button>
      </div>
    </div>
  );
}
