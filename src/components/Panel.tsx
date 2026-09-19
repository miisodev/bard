import { MoreHorizontal, Search } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DictationState } from "../App";
import { api, isShortcutRecording } from "../store";
import type { AppState, Item, ModelProgress, Section } from "../types";
import ContextMenu, { type MenuAction } from "./ContextMenu";
import ItemCard from "./ItemCard";
import Menu from "./Menu";
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
  const [showOverflow, setShowOverflow] = useState(false);
  const [composerMenu, setComposerMenu] = useState<{ x: number; y: number } | null>(null);
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
  const fileInputRef = useRef<HTMLInputElement>(null);
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
      if (isShortcutRecording()) return;
      if (showOverflow) {
        if (e.key === "Escape") setShowOverflow(false);
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
          // Space marks done, as in Copper. Toggling the whole selection off
          // the lead item's state keeps a mixed selection from splitting.
          if (sel.length > 0) {
            e.preventDefault();
            const lead = state.items.find((i) => i.id === sel[0]);
            const next = lead ? lead.done === 0 : true;
            for (const id of sel) void api.setDone(id, next);
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
  }, [
    state,
    query,
    onlyFavorites,
    selectedIds,
    anchorId,
    menu,
    showSwitcher,
    showSettings,
    showOverflow,
  ]);

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
    if (composerRef.current) composerRef.current.style.height = "auto";
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
  const activeSectionName =
    state.sections.find((s) => s.id === state.activeSectionId)?.name ?? null;
  const doneCount = state.items.filter((i) => i.done !== 0).length;

  return (
    <>
      {/* No wordmark and no window controls: the panel is dismissed from the
          keyboard, and its chrome is a search field and one overflow button. */}
      <header className="header" onMouseDown={onDrag}>
        <div className="search-field">
          <Search size={14} strokeWidth={2} aria-hidden="true" />
          <input
            ref={searchRef}
            className="search"
            placeholder="Search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button
          className={`icon-btn${showOverflow ? " active" : ""}`}
          aria-label="More"
          aria-haspopup="menu"
          aria-expanded={showOverflow}
          onClick={() => setShowOverflow((v) => !v)}
        >
          <MoreHorizontal size={16} strokeWidth={2} />
        </button>
      </header>

      <div className="resize-layer">
        {RESIZE_DIRS.map(([cls, dir]) => (
          <div key={cls} className={`resize-handle h-${cls}`} onMouseDown={onResizeStart(dir)} />
        ))}
      </div>

      <div className="timeline">
        {state.items.length === 0 ? (
          <EmptyState section={activeSectionName} />
        ) : grouped.length === 0 ? (
          <div className="empty-state">
            <div className="title">No matches</div>
            <div className="hint">Nothing here matches “{query}”.</div>
          </div>
        ) : (
          grouped.map((group) => (
            <div key={group.section ? `s${group.section.id}` : "inbox"}>
              {/* A letterspaced label and a hairline reaching the edge — the
                  count and the folder glyph were noise on a list you already
                  see in full. */}
              <button
                type="button"
                className={`group-header${
                  (group.section ? group.section.id : null) === state.activeSectionId
                    ? " active"
                    : ""
                }`}
                onClick={() => {
                  void api.setActiveSection(group.section ? group.section.id : null);
                  refresh();
                }}
              >
                <span className="group-name">
                  {group.section ? group.section.name : "Inbox"}
                </span>
                <span className="group-rule" aria-hidden="true" />
              </button>
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
                  onCopyImage={() => copySelection(state, [it.id], setToast)}
                  onToggleDone={() => {
                    void api.setDone(it.id, it.done === 0).then(refresh);
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
          <button
            type="button"
            className="composer-add"
            aria-label="Attach or draw"
            aria-haspopup="menu"
            onClick={(e) => {
              const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
              setComposerMenu({ x: r.left, y: r.top - 8 });
            }}
          >
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
              <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.3" />
              <path d="M8 4.8 V11.2 M4.8 8 H11.2" stroke="currentColor"
                    strokeWidth="1.3" strokeLinecap="round" />
            </svg>
          </button>
          <input
            ref={fileInputRef}
            type="file"
            multiple
            hidden
            onChange={(e) => {
              const files = [...(e.target.files ?? [])];
              void addFiles(files, setToast);
              e.target.value = "";
            }}
          />
          <textarea
            ref={composerRef}
            className="composer-input"
            rows={1}
            placeholder={`Add a note or a prompt${
              activeSectionName ? ` (${activeSectionName.toLowerCase()})` : ""
            }`}
            value={composer}
            onChange={(e) => {
              setComposer(e.target.value);
              e.target.style.height = "auto";
              e.target.style.height = `${Math.min(e.target.scrollHeight, 120)}px`;
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitComposer();
              }
            }}
          />
        </div>
      </div>

      {showOverflow ? (
        <Menu
          onClose={() => setShowOverflow(false)}
          entries={[
            {
              label: "Favorites only",
              checked: onlyFavorites,
              trailing: onlyFavorites ? "✓" : undefined,
              onClick: () => setOnlyFavorites(!onlyFavorites),
            },
            {
              label: "Clear completed",
              disabled: doneCount === 0,
              trailing: doneCount > 0 ? doneCount : undefined,
              onClick: () => {
                void api.clearCompleted().then(refresh);
                setToast(`Cleared ${doneCount} completed`);
              },
            },
            {
              label: "Export to Markdown",
              separated: true,
              onClick: () => {
                void api.exportMarkdown().then((p) => setToast(p ? "Exported" : "Export failed"));
              },
            },
            {
              label: "Settings",
              separated: true,
              onClick: () => setShowSettings(true),
            },
          ]}
        />
      ) : null}

      {composerMenu ? (
        <Menu
          anchor="point"
          at={composerMenu}
          onClose={() => setComposerMenu(null)}
          entries={[
            { label: "Attach Files…", onClick: () => fileInputRef.current?.click() },
            { label: "Create Drawing…", onClick: () => void api.openDraw() },
          ]}
        />
      ) : null}

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

/**
 * Add dropped or picked files. Images become image items; text files become
 * notes from their contents; anything else becomes a note naming the file.
 */
async function addFiles(files: File[], toast: (m: string) => void) {
  if (files.length === 0) return;
  for (const file of files) {
    if (file.type.startsWith("image/")) {
      const { b64, ext } = await readBlob(file);
      await api.addImage(b64, ext);
    } else if (file.type === "text/plain") {
      const text = (await file.text()).trim();
      if (text) await api.addItem(text);
    } else {
      await api.addItem(file.name);
    }
  }
  toast(files.length === 1 ? "Added 1 file" : `Added ${files.length} files`);
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

/*
  The empty state asks about the section you are actually in — "What should we
  build in ui?" rather than a generic nothing-here. It is the one place the app
  addresses you, so it stays a question, and the hint under it exists only
  because a first-run user cannot know about double-shift yet.
*/
function EmptyState({ section }: { section: string | null }) {
  return (
    <div className="empty-state">
      <svg className="empty-glyph" viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="9.25" fill="none" strokeWidth="1.5" />
        <path d="M8.6 14.2a4.2 4.2 0 0 0 6.8 0" fill="none" strokeWidth="1.5"
              strokeLinecap="round" />
        <circle cx="9.2" cy="10" r="1" stroke="none" />
        <circle cx="14.8" cy="10" r="1" stroke="none" />
      </svg>
      <div className="title">
        What should we build{section ? ` in ${section}` : ""}?
      </div>
      <div className="hint">
        Select text anywhere, then tap <kbd>Shift</kbd> <kbd>Shift</kbd>.
      </div>
    </div>
  );
}