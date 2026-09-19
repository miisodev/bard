import { FolderOpen, Minus, Settings, Star, X } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DictationState } from "../App";
import { api } from "../store";
import type { AppState, Item, ModelProgress, Section } from "../types";
import ContextMenu, { type MenuAction } from "./ContextMenu";
import ItemCard from "./ItemCard";
import SectionSwitcher from "./SectionSwitcher";
import SettingsSheet from "./SettingsSheet";

const win = getCurrentWindow();

type ResizeDir =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

const RESIZE_DIRS: [string, ResizeDir][] = [
  ["t", "North"],
  ["b", "South"],
  ["l", "West"],
  ["r", "East"],
  ["lt", "NorthWest"],
  ["rt", "NorthEast"],
  ["lb", "SouthWest"],
  ["rb", "SouthEast"],
];

export default function Panel({
  state,
  modelProgress,
  dictation,
  onState,
}: {
  state: AppState;
  modelProgress: ModelProgress | null;
  dictation: DictationState;
  onState: (s: AppState) => void;
}) {
  const [query, setQuery] = useState("");
  const [onlyFavorites, setOnlyFavorites] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [anchorId, setAnchorId] = useState<number | null>(null);
  const [composer, setComposer] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [showSwitcher, setShowSwitcher] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    ids: number[];
    item?: Item;
  } | null>(null);
  const [soundOnCapture, setSoundOnCapture] = useState(true);
  const [editingId, setEditingId] = useState<number | null>(null);

  const composerRef = useRef<HTMLTextAreaElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    void api.getState().then(onState);
    void api.getSoundOnCapture().then(setSoundOnCapture);
  }, []);

  useEffect(() => {
    const onToast = (e: Event) => {
      const msg = (e as CustomEvent).detail as string;
      setToast(msg);
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
      toastTimer.current = window.setTimeout(() => setToast(null), 2600);
    };
    const onCaptured = () => {
      if (soundOnCapture) playPop();
    };
    window.addEventListener("bard-toast", onToast);
    window.addEventListener("bard-captured", onCaptured);
    return () => {
      window.removeEventListener("bard-toast", onToast);
      window.removeEventListener("bard-captured", onCaptured);
    };
  }, [soundOnCapture]);

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];
    let timer: number | undefined;
    const save = () => {
      void Promise.all([win.outerPosition(), win.outerSize()]).then(([p, s]) => {
        void api.saveWindowGeometry(p.x, p.y, s.width, s.height);
      });
    };
    const debounced = () => {
      if (timer) window.clearTimeout(timer);
      timer = window.setTimeout(save, 400);
    };
    unlisteners.push(win.onMoved(debounced));
    unlisteners.push(win.onResized(debounced));
    return () => {
      for (const u of unlisteners) void u.then((fn) => fn());
      if (timer) window.clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const typing =
        target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
      if (menu) {
        if (e.key === "Escape") setMenu(null);
        return;
      }
      if (showSwitcher) return;
      if (showSettings) return;
      if (typing) {
        if (
          e.key === "Escape" &&
          (target as HTMLInputElement).value === "" &&
          target.tagName === "INPUT"
        ) {
          void api.hidePanel();
        }
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setShowSwitcher(true);
        return;
      }
      const flat = flatFiltered(state, query, onlyFavorites);
      const idx = (id: number) => flat.indexOf(id);
      const sel = [...selectedIds].sort((a, b) => idx(a) - idx(b));
      switch (e.key) {
        case "ArrowDown":
        case "ArrowUp": {
          e.preventDefault();
          if (flat.length === 0) break;
          const delta = e.key === "ArrowDown" ? 1 : -1;
          let next: number;
          if (sel.length === 0) {
            next = delta > 0 ? flat[0] : flat[flat.length - 1];
          } else {
            const lead = e.shiftKey ? sel[sel.length - 1] : sel[0];
            const cur = idx(lead);
            const target = Math.min(flat.length - 1, Math.max(0, cur + delta));
            next = flat[target];
          }
          if (e.shiftKey && anchorId !== null) {
            const a = idx(anchorId);
            const b = idx(next);
            setSelectedIds(new Set(flat.slice(Math.min(a, b), Math.max(a, b) + 1)));
          } else {
            setSelectedIds(new Set([next]));
            setAnchorId(next);
          }
          break;
        }
        case "Enter": {
          if (sel.length > 0) {
            e.preventDefault();
            copySelection(state, sel, setToast);
          }
          break;
        }
        case "Delete":
        case "Backspace": {
          if (sel.length > 0) {
            e.preventDefault();
            for (const id of sel) void api.deleteItem(id);
            setSelectedIds(new Set());
            refresh();
          }
          break;
        }
        case " ": {
          if (sel.length > 0) {
            e.preventDefault();
            for (const id of sel) {
              const item = state.items.find((i) => i.id === id);
              if (item) void api.setFavorite(id, !item.favorite);
            }
            refresh();
          }
          break;
        }
        case "Escape": {
          if (query) setQuery("");
          else if (selectedIds.size > 0) setSelectedIds(new Set());
          else void api.hidePanel();
          break;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state, query, onlyFavorites, selectedIds, anchorId, menu, showSwitcher, showSettings]);

  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;
      for (const item of items) {
        if (item.kind === "file" && item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) {
            void readBlob(file).then(({ b64, ext }) => {
              void api.addImage(b64, ext);
            });
          }
        }
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, []);

  useEffect(() => {
    const onDragOver = (e: DragEvent) => {
      if (e.dataTransfer?.types.includes("Files")) {
        e.preventDefault();
        if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      }
    };
    const onDragLeave = (e: DragEvent) => {
      if (e.relatedTarget === null) void (0);
    };
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      const dt = e.dataTransfer;
      let added = 0;
      if (!dt || !dt.files.length) return;
      const el = document.elementFromPoint(e.clientX, e.clientY);
      const card = el?.closest("[data-item-id]") as HTMLElement | null;
      const itemId = card ? Number(card.dataset.itemId) : null;
      const kind = card?.dataset.itemKind;
      const imageFiles: File[] = [];
      for (const file of [...dt.files]) {
        if (file.type.startsWith("image/")) {
          imageFiles.push(file);
          added += 1;
        }
      }
      if (itemId !== null && kind !== "image" && imageFiles.length > 0) {
        for (const file of imageFiles) {
          void readBlob(file).then(({ b64, ext }) => {
            void api.attachImage(itemId, b64, ext).then(refresh);
          });
        }
      } else {
        for (const file of imageFiles) {
          void readBlob(file).then(({ b64, ext }) => {
            void api.addImage(b64, ext);
          });
        }
        for (const file of [...dt.files]) {
          if (!file.type.startsWith("image/")) {
            if (file.type === "text/plain") {
              void file.text().then((t) => t.trim() && void api.addItem(t.trim()));
            } else {
              void api.addItem(file.name);
            }
            added += 1;
          }
        }
      }
      refresh();
      if (added > 0) setToast(`Added ${added} file(s)`);
    };
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("dragleave", onDragLeave);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragover", onDragOver);
      window.removeEventListener("dragleave", onDragLeave);
      window.removeEventListener("drop", onDrop);
    };
  }, []);

  const refresh = () => void api.getState().then(onState);

  const onDrag = (e: ReactMouseEvent) => {
    const t = e.target as HTMLElement;
    if (t.closest("button, input, textarea")) return;
    void win.startDragging();
  };

  const onResizeStart = (dir: ResizeDir) => (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    void win.startResizeDragging(dir);
  };

  const submitComposer = () => {
    const v = composer.trim();
    if (!v) return;
    if (v.startsWith("#")) {
      void api.createSection(v.replace(/^#+\s*/, "")).then(() => setToast(`Section created`));
    } else {
      void api.addItem(v);
    }
    setComposer("");
    refresh();
  };

  const onItemClick = (item: Item, e: ReactMouseEvent) => {
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selectedIds);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      setSelectedIds(next);
      setAnchorId(item.id);
      return;
    }
    if (e.shiftKey && anchorId !== null) {
      const flat = flatFiltered(state, query, onlyFavorites);
      const a = flat.indexOf(anchorId);
      const b = flat.indexOf(item.id);
      if (a >= 0 && b >= 0) {
        setSelectedIds(new Set(flat.slice(Math.min(a, b), Math.max(a, b) + 1)));
      }
      return;
    }
    setSelectedIds(new Set([item.id]));
    setAnchorId(item.id);
  };

  const onContextMenu = (item: Item, e: ReactMouseEvent) => {
    e.preventDefault();
    let ids = [item.id];
    if (selectedIds.has(item.id) && selectedIds.size > 1) {
      ids = [...selectedIds];
    }
    setMenu({ x: e.clientX, y: e.clientY, ids, item });
  };

  const onMenuAction = (action: MenuAction) => {
    if (!menu) return;
    const ids = menu.ids;
    switch (action) {
      case "copy":
        copySelection(state, ids, setToast);
        break;
      case "copyList":
        void api
          .copyText(ids.map((id) => state.items.find((i) => i.id === id)?.content).filter(Boolean).join("\n"))
          .then(() => setToast("Copied as list"));
        break;
      case "favorite":
        for (const id of ids) {
          const it = state.items.find((i) => i.id === id);
          if (it) void api.setFavorite(id, !it.favorite);
        }
        refresh();
        break;
      case "edit":
        if (menu.item && menu.item.kind === "text") setEditingId(menu.item.id);
        break;
      case "caption":
        if (menu.item) void api.addNoteToImage(menu.item.id).then(refresh);
        break;
      case "delete":
        for (const id of ids) void api.deleteItem(id);
        setSelectedIds(new Set());
        refresh();
        break;
    }
    setMenu(null);
  };

  const grouped = groupBySection(state, query, onlyFavorites);

  return (
    <>
      <header className="header" onMouseDown={onDrag}>
        <span className="wordmark">
          b<span className="dot">a</span>rd
        </span>
        <input
          ref={searchRef}
          className="search"
          placeholder="Search clipboard…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className={`icon-btn${onlyFavorites ? " active" : ""}`}
          title="Favorites only"
          onClick={() => setOnlyFavorites(!onlyFavorites)}
        >
          <Star size={15} strokeWidth={1.8} />
        </button>
        <button className="icon-btn" title="Settings" onClick={() => setShowSettings(true)}>
          <Settings size={15} strokeWidth={1.8} />
        </button>
        <span className="win-controls">
          <button className="icon-btn win-btn" title="Minimize" onClick={() => void api.minimizePanel()}>
            <Minus size={15} strokeWidth={1.8} />
          </button>
          <button
            className="icon-btn win-btn close"
            title="Close (hides to tray)"
            onClick={() => void api.hidePanel()}
          >
            <X size={15} strokeWidth={1.8} />
          </button>
        </span>
      </header>

      <div className="resize-layer">
        {RESIZE_DIRS.map(([cls, dir]) => (
          <div key={cls} className={`resize-handle h-${cls}`} onMouseDown={onResizeStart(dir)} />
        ))}
      </div>

      <div className="timeline">
        {state.items.length === 0 ? (
          <EmptyState />
        ) : grouped.length === 0 ? (
          <div className="empty-state">
            <div className="title">No matches</div>
            <div className="hint">Nothing here matches “{query}”.</div>
          </div>
        ) : (
          grouped.map((group) => (
            <div key={group.section ? `s${group.section.id}` : "inbox"}>
              <div
                className="group-header"
                style={{ cursor: "pointer" }}
                onClick={() => {
                  void api.setActiveSection(group.section ? group.section.id : null);
                  refresh();
                }}
              >
                <FolderOpen size={12} strokeWidth={1.8} />
                <span>{group.section ? group.section.name : "Inbox"}</span>
                <span className="group-count">{group.items.length}</span>
              </div>
              {group.items.map((it) => (
                <ItemCard
                  key={it.id}
                  item={it}
                  attachments={state.attachments.filter((a) => a.itemId === it.id)}
                  selected={selectedIds.has(it.id)}
                  editing={editingId === it.id}
                  search={query}
                  onClick={(e) => onItemClick(it, e)}
                  onContextMenu={(e) => onContextMenu(it, e)}
                  onCopy={() => copySelection(state, [it.id], setToast)}
                  onFavorite={() => {
                    void api.setFavorite(it.id, !it.favorite).then(refresh);
                  }}
                  onDelete={() => {
                    void api.deleteItem(it.id).then(() => {
                      setSelectedIds(new Set());
                      refresh();
                    });
                  }}
                  onEdit={() => setEditingId(it.id)}
                  onEditDone={() => setEditingId(null)}
                />
              ))}
            </div>
          ))
        )}
      </div>

      <div className="composer">
        {dictation.listening || dictation.transcribing ? (
          <div className="dictate-indicator">
            <span className="pulse-dot" />
            {dictation.transcribing ? "Transcribing…" : "Listening — speak now"}
          </div>
        ) : null}
        <div className="composer-row">
          <textarea
            ref={composerRef}
            className="composer-input"
            rows={1}
            placeholder="Type a note, or # Section name — Enter to add"
            value={composer}
            onChange={(e) => setComposer(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitComposer();
              }
            }}
          />
        </div>
      </div>

      {showSwitcher ? (
        <SectionSwitcher
          state={state}
          onClose={() => setShowSwitcher(false)}
          onPicked={(id) => {
            void api.setActiveSection(id);
            setShowSwitcher(false);
            refresh();
          }}
        />
      ) : null}

      {showSettings ? (
        <SettingsSheet
          state={state}
          modelProgress={modelProgress}
          onClose={() => setShowSettings(false)}
          onState={onState}
        />
      ) : null}

      {menu ? (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          multi={menu.ids.length > 1}
          item={menu.item}
          onAction={onMenuAction}
          onClose={() => setMenu(null)}
        />
      ) : null}

      {toast ? <div className="toast">{toast}</div> : null}
    </>
  );
}

function flatFiltered(state: AppState, query: string, onlyFavorites: boolean): number[] {
  return filterItems(state, query, onlyFavorites).map((i) => i.id);
}

function filterItems(state: AppState, query: string, onlyFavorites: boolean): Item[] {
  const q = query.trim().toLowerCase();
  return state.items.filter((it) => {
    if (onlyFavorites && !it.favorite) return false;
    if (!q) return true;
    if (it.kind === "image") return false;
    return it.content.toLowerCase().includes(q);
  });
}

function groupBySection(
  state: AppState,
  query: string,
  onlyFavorites: boolean
): { section: Section | null; items: Item[] }[] {
  const items = filterItems(state, query, onlyFavorites);
  const sections = [...state.sections];
  const active = state.activeSectionId;
  if (active !== null) {
    sections.sort((a, b) => (a.id === active ? -1 : b.id === active ? 1 : a.id - b.id));
  }
  const out: { section: Section | null; items: Item[] }[] = [];
  const inbox = items.filter((i) => i.sectionId === null);
  if (inbox.length > 0) out.push({ section: null, items: inbox });
  for (const s of sections) {
    const group = items.filter((i) => i.sectionId === s.id);
    if (group.length > 0) out.push({ section: s, items: group });
  }
  return out;
}

function copySelection(state: AppState, ids: number[], toast: (m: string) => void) {
  if (ids.length === 0) return;
  const items = ids
    .map((id) => state.items.find((i) => i.id === id))
    .filter((i): i is Item => !!i);
  if (items.length === 1) {
    const it = items[0];
    if (it.kind === "image" && it.imagePath) {
      void api.copyItem(it.id).then(() => toast("Copied"));
    } else {
      void api.copyText(it.content).then(() => toast("Copied"));
    }
    return;
  }
  void api
    .copyText(
      items.map((i) => (i.kind === "text" ? `- ${i.content}` : `- [image]`)).join("\n")
    )
    .then(() => toast("Copied as list"));
}

function readBlob(file: File): Promise<{ b64: string; ext: string }> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      const comma = url.indexOf(",");
      resolve({
        b64: url.slice(comma + 1),
        ext: (file.type.split("/")[1] || "png").replace("jpeg", "jpg").replace(/\+\w+$/, ""),
      });
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

let audioCtx: AudioContext | null = null;
function playPop() {
  try {
    if (!audioCtx) audioCtx = new AudioContext();
    const c = audioCtx;
    const t = c.currentTime;
    const osc = c.createOscillator();
    const gain = c.createGain();
    osc.type = "sine";
    osc.frequency.setValueAtTime(1500, t);
    osc.frequency.exponentialRampToValueAtTime(480, t + 0.05);
    gain.gain.setValueAtTime(0.22, t);
    gain.gain.exponentialRampToValueAtTime(0.0008, t + 0.07);
    osc.connect(gain);
    gain.connect(c.destination);
    osc.start(t);
    osc.stop(t + 0.08);
  } catch {
    /* audio unavailable */
  }
}

function EmptyState() {
  return (
    <div className="empty-state">
      <div className="title">Nothing captured yet</div>
      <div className="hint">
        Copy anything, anywhere — it lands here. Or tap <kbd>Shift</kbd>{" "}
        <kbd>Shift</kbd> to capture a selection without copying it.
      </div>
    </div>
  );
}