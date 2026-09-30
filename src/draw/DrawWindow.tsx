import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowRight,
  Circle,
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
  ChevronDown,
} from "lucide-react";
import ResizeHandles from "../components/ResizeHandles";
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

const WIDTHS = [2, 4, 8];

/*
  One icon family. The seven tools used to be hand-drawn inline SVG at a
  different stroke weight from the Lucide Undo and Redo beside them, in the
  same 28px button: two icon systems, one toolbar.
*/
const TOOLS: { tool: Tool; label: string; Icon: typeof MousePointer2 }[] = [
  { tool: "select", label: "Select", Icon: MousePointer2 },
  { tool: "pen", label: "Pen", Icon: Pen },
  { tool: "line", label: "Line", Icon: Minus },
  { tool: "arrow", label: "Arrow", Icon: ArrowRight },
  { tool: "rect", label: "Rectangle", Icon: Square },
  { tool: "ellipse", label: "Ellipse", Icon: Circle },
  { tool: "text", label: "Text", Icon: Type },
];

/*
  The swatches are drawing inks, and the first of them is the page's own ink:
  dark on a light page, light on a dark one, so a stroke in ink stays readable
  when the page turns. The three hues beside it are the theme's own accent,
  danger and success read at render time — the toolbar used to hold the light
  theme's values as literals, which put a light blue swatch beside a dark
  theme's blue selection ring, two blues 2px apart.
*/
const HUE_FALLBACK = ["#2f6fed", "#e5484d", "#2c9e5b"] as const;

/**
 * Amber has no token. It is a drawing ink, and a mid amber reads on a light
 * page and a dark one alike, so it stays a literal here — a `--warning` token
 * in tokens.css would be the correct home for it.
 */
const AMBER_INK = "#e8a33d";

function tokenInk(property: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(property).trim();
  return value || fallback;
}

/** The theme's inks, re-read whenever the resolved theme can have changed. */
function useHues(themeKey: string): string[] {
  const [hues, setHues] = useState<string[]>([...HUE_FALLBACK, AMBER_INK]);
  useEffect(() => {
    // Runs after `useTheme`'s own effect has written the new theme, so this
    // reads the tokens the window is actually painted with.
    setHues([
      tokenInk("--accent", HUE_FALLBACK[0]),
      tokenInk("--danger", HUE_FALLBACK[1]),
      tokenInk("--success", HUE_FALLBACK[2]),
      AMBER_INK,
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

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
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
  // Undo used to be disabled and Redo never was, because the redo stack lives
  // in a ref and nothing in state said whether it held anything.
  const [redoCount, setRedoCount] = useState(0);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [discard, setDiscard] = useState(false);
  const [textField, setTextField] = useState<{ at: Point; left: number; top: number; value: string } | null>(null);
  const textSize = Math.max(14, width * 5);
  const [savedOnce, setSavedOnce] = useState(false);
  const savedIdRef = useRef<number | null>(null);

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

  // Browser preview only: a seeded sketch, so screenshots show Draw in use.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const seed = (window as { __bardPreviewShapes?: Shape[] }).__bardPreviewShapes;
    if (seed && shapesRef.current.length === 0) {
      shapesRef.current = seed.map((shape) => ({ ...shape }));
      paint();
    }
  }, [paint]);

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
      setRedoCount(0);
      setEmpty(false);
    }
    draftRef.current = null;
    paint();
  };

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

    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);

    if (tool === "select") {
      const hit = hitTest(shapesRef.current, p);
      setSelected(hit?.id ?? null);
      dragRef.current = hit ? { id: hit.id, last: p } : null;
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
    setRedoCount(undoneRef.current.length);
    setSelected(null);
    setEmpty(shapesRef.current.length === 0);
    paint();
  };

  const redo = () => {
    const s = undoneRef.current.pop();
    if (!s) return;
    shapesRef.current = [...shapesRef.current, s];
    setRedoCount(undoneRef.current.length);
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
    // Read the shapes from the ref, not from `empty`: the key handler closes
    // over whichever render registered it, and a stale `empty` there silently
    // swallowed Ctrl+Enter.
    if (!canvas || saving || shapesRef.current.length === 0) return;
    setSaving(true);
    setSaveError(null);
    try {
      let id = savedIdRef.current;
      if (id == null) {
        const b64 = exportPng(shapesRef.current, canvas.clientWidth, canvas.clientHeight, 2, page);
        if (!b64) throw new Error("The page could not be rendered.");
        id = await api.addImage(b64, "png");
        // The drawing is in Bard from here: it is no longer unsaved work, so
        // closing stops having to ask before throwing it away.
        savedIdRef.current = id || null;
        setSavedOnce(true);
      }
      if (alsoCopy && id) {
        try {
          await api.copyItem(id);
        } catch {
          // In Bard, not on the clipboard. Worth saying, and not worth a
          // second copy of the same drawing in the timeline if Save is pressed
          // again — hence the id remembered above.
          setSaveError("Saved to Bard, but Bard could not copy it to the clipboard.");
          return;
        }
      }
      await win.close();
    } catch {
      // Previously an unhandled rejection: the drawing was never saved, nothing
      // said so, and closing the window took it with it.
      setSaveError("Couldn't save this drawing. Try again.");
    } finally {
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
      // F11 is Draw's own while Draw has focus — the universal fullscreen key,
      // never taken from whichever app you are actually using.
      if (e.key === "F11") {
        e.preventDefault();
        void toggleFullscreen();
        return;
      }
      // An open text field owns the keyboard; its own handler stops the event.
      if (textField) return;
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
        else if (discard) setDiscard(false);
        else if (selected != null) setSelected(null);
        else if (fullscreen) void toggleFullscreen();
        else requestClose();
      } else if (mod && e.key === "Enter") {
        e.preventDefault();
        void save(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, saving, fullscreen, canvasOpen, discard, textField, requestClose, toggleFullscreen]);

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
        />
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

      <div className="draw-toolbar">
        <div className="draw-group">
          {TOOLS.map(({ tool: id, label, Icon }) => (
            <button
              key={id}
              className={`draw-tool${tool === id ? " on" : ""}`}
              title={label}
              aria-label={label}
              aria-pressed={tool === id}
              onClick={() => {
                setTool(id);
                if (id !== "select") setSelected(null);
              }}
            >
              <Icon size={16} strokeWidth={1.8} aria-hidden="true" />
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
          <button
            className="draw-tool"
            onClick={redo}
            disabled={redoCount === 0}
            title="Redo · Ctrl+Shift+Z"
            aria-label="Redo"
          >
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
