import { ArrowUpDown, ListChecks, Paintbrush, Search, Settings2, X } from "lucide-react";
import { playCue } from "../audio";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DictationState } from "../App";
import { api } from "../store";
import type { AppState, Attachment, Item, Section, SortOrder } from "../types";
import ContextMenu, { type MenuAction } from "./ContextMenu";
import ItemCard from "./ItemCard";
import Menu from "./Menu";
import SectionSwitcher from "./SectionSwitcher";

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

/** A confirmation is a receipt. An error is the app reporting a failure. */
export type ToastTone = "info" | "error";

/** What every message in the panel goes through, however it was raised. */
type Notify = (text: string, tone?: ToastTone) => void;

/** The `bard-toast` payload: a bare string is a confirmation. */
export interface ToastDetail {
  text: string;
  tone?: ToastTone;
}

export default function Panel({
  state,
  dictation,
  onState,
  activeItemId = null,
  onActiveItemChange,
}: {
  state: AppState;
  dictation: DictationState;
  onState: (s: AppState) => void;
  activeItemId?: number | null;
  onActiveItemChange?: (id: number | null) => void;
}) {
  const [query, setQuery] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [anchorId, setAnchorId] = useState<number | null>(null);
  const [composer, setComposer] = useState("");
  const [showSwitcher, setShowSwitcher] = useState(false);
  const [showOverflow, setShowOverflow] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [composerMenu, setComposerMenu] = useState<{ x: number; y: number } | null>(null);
  const [toast, setToast] = useState<{ text: string; tone: ToastTone } | null>(null);
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    ids: number[];
    item?: Item;
  } | null>(null);
  const [soundOnCapture, setSoundOnCapture] = useState(true);
  const [editingId, setEditingId] = useState<number | null>(null);
  // Which companions are out, so their header buttons read as pressed.
  const [companions, setCompanions] = useState({ draw: false, settings: false });
  // The one capture that has not been on screen yet, so only it animates in.
  const [freshId, setFreshId] = useState<number | null>(null);

  const composerRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const timelineRef = useRef<HTMLDivElement>(null);
  const toastTimer = useRef<number | undefined>(undefined);
  // The last id the backend was told about, so focus, clicks and arrow keys all
  // cost one write between them rather than one each.
  const activeReported = useRef<number | null>(activeItemId);
  const seenIds = useRef<Set<number> | null>(null);
  /** Timeline position to focus once the list next renders — set by a delete. */
  const focusAfterRender = useRef<number | null>(null);

  /*
    Everything the timeline draws is derived from `state.items` and `query`,
    and both change identity on every `refresh` — which fires after every
    click, keypress, capture and sort change. Derived in the body, that meant
    re-filtering, re-grouping and re-counting the whole timeline per render,
    and O(items × attachments) on top of it, 20 times a second during a
    dictation take. Each one is a memo now, on its own real inputs.
  */
  const attachmentsByItem = useMemo(() => {
    const index = new Map<number, Attachment[]>();
    for (const attachment of state.attachments) {
      const list = index.get(attachment.itemId);
      if (list) list.push(attachment);
      else index.set(attachment.itemId, [attachment]);
    }
    return index;
  }, [state.attachments]);

  const visible = useMemo(() => filterItems(state.items, query), [state.items, query]);

  /** Visible order, by id. Every arrow key and every range is a lookup here. */
  const order = useMemo(() => {
    const index = new Map<number, number>();
    visible.forEach((item, position) => index.set(item.id, position));
    return index;
  }, [visible]);

  const grouped = useMemo(
    () => groupBySection(visible, state.sections, state.activeSectionId),
    [visible, state.sections, state.activeSectionId],
  );

  const doneCount = useMemo(
    () => state.items.reduce((count, item) => (item.done !== 0 ? count + 1 : count), 0),
    [state.items],
  );

  /**
   * Roving tabindex: the row the arrows are pointing at is the timeline's only
   * tab stop. When the active item has been filtered out, or has never been set,
   * the first visible row takes it — a list whose tab stop is missing cannot be
   * tabbed into at all.
   */
  const rovingId = useMemo(() => {
    if (activeItemId !== null && order.has(activeItemId)) return activeItemId;
    return visible[0]?.id ?? null;
  }, [activeItemId, order, visible]);

  useEffect(() => {
    const seen = seenIds.current;
    if (seen === null) {
      seenIds.current = new Set(state.items.map((item) => item.id));
      return;
    }
    const added = state.items.find((item) => !seen.has(item.id)) ?? null;
    for (const item of state.items) seen.add(item.id);
    if (added) setFreshId(added.id);
  }, [state.items]);

  useEffect(() => {
    activeReported.current = activeItemId;
  }, [activeItemId]);

  const refresh = useCallback(() => void api.getState().then(onState), [onState]);

  const markActive = useCallback(
    (id: number | null) => {
      if (activeReported.current === id) return;
      activeReported.current = id;
      onActiveItemChange?.(id);
      void api.setActiveItem(id);
    },
    [onActiveItemChange],
  );

  /** Move DOM focus to a row, and keep it on screen. */
  const focusCard = useCallback((id: number) => {
    const card = timelineRef.current?.querySelector<HTMLElement>(`[data-item-id="${id}"]`);
    if (!card) return;
    card.focus();
    card.scrollIntoView({ block: "nearest" });
  }, []);

  const notify = useCallback((text: string, tone: ToastTone = "info") => {
    setToast({ text, tone });
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    // A confirmation has done its job in under three seconds. An error does
    // not: a licence refusal or a copy that did not land has to stay up until
    // it is read and dismissed, and it carries the control to dismiss it.
    if (tone === "info") {
      toastTimer.current = window.setTimeout(() => setToast(null), 2600);
    }
  }, []);

  const dismissToast = useCallback(() => {
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    setToast(null);
  }, []);

  // Every overlay takes a stable close: the menus and the shortcut sheet restore
  // focus and re-focus their first row when `onClose` changes, so a new closure
  // on each render makes them thrash.
  const closeMenu = useCallback(() => setMenu(null), []);
  const closeOverflow = useCallback(() => setShowOverflow(false), []);
  const closeComposerMenu = useCallback(() => setComposerMenu(null), []);
  const closeSwitcher = useCallback(() => setShowSwitcher(false), []);
  const closeShortcuts = useCallback(() => setShowShortcuts(false), []);

  /*
    One delete path. Three call sites each used to do the same five things, and
    each of them dropped the cursor on the floor: the focused card unmounted,
    focus fell to the document, and the next Tab restarted at the search field.
    The position is recorded here and the neighbour is focused once the shorter
    list has actually rendered.
  */
  const deleteItems = useCallback(
    (ids: number[]) => {
      if (ids.length === 0) return;
      const first = order.get([...ids].sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0))[0]);
      focusAfterRender.current = first ?? 0;
      for (const id of ids) void api.deleteItem(id);
      setSelectedIds(new Set());
      if (activeItemId !== null && ids.includes(activeItemId)) markActive(null);
      refresh();
    },
    [order, markActive, refresh, activeItemId],
  );

  useEffect(() => {
    const position = focusAfterRender.current;
    if (position === null) return;
    focusAfterRender.current = null;
    const cards = timelineRef.current?.querySelectorAll<HTMLElement>("[data-item-id]");
    if (!cards || cards.length === 0) return;
    const neighbour = cards[Math.min(position, cards.length - 1)];
    neighbour.focus();
    neighbour.scrollIntoView({ block: "nearest" });
  }, [visible]);

  useEffect(() => {
    void api.getState().then(onState);
    void api.getSoundOnCapture().then(setSoundOnCapture);
  }, []);

  useEffect(() => {
    const pending = listen<{ draw: boolean; settings: boolean }>("companions", (event) =>
      setCompanions(event.payload),
    );
    return () => void pending.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    const onToast = (event: Event) => {
      const detail = (event as CustomEvent).detail as string | ToastDetail;
      if (typeof detail === "string") notify(detail);
      else if (detail && typeof detail.text === "string") notify(detail.text, detail.tone ?? "info");
    };
    const onCaptured = () => {
      if (soundOnCapture) playCue("capture");
    };
    window.addEventListener("bard-toast", onToast);
    window.addEventListener("bard-captured", onCaptured);
    return () => {
      window.removeEventListener("bard-toast", onToast);
      window.removeEventListener("bard-captured", onCaptured);
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
    };
  }, [soundOnCapture, notify]);

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
      if (showShortcuts) {
        if (e.key === "Escape") setShowShortcuts(false);
        return;
      }
      if (showOverflow) {
        if (e.key === "Escape") setShowOverflow(false);
        return;
      }
      if (showSwitcher) return;
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

      /*
        The acted-on item is the focused one, never the selection this render
        happened to capture. A Space or Enter on a card used to run the card's
        own handler, which changed the selection, and then bubble to here and
        act on the *previous* selection — so the keys read on the timeline were
        not the keys that took effect.
      */
      const row = target instanceof Element ? target.closest<HTMLElement>("[data-item-id]") : null;
      const focusedId = row?.dataset.itemId ? Number(row.dataset.itemId) : null;
      // A control inside a card keeps its own Space and Enter; the panel only
      // claims them where nothing else will.
      const onControl =
        target instanceof Element &&
        target.closest('button, a[href], [role="button"], input, textarea, select') !== null;
      const idx = (id: number) => order.get(id) ?? -1;
      const sel = [...selectedIds].sort((a, b) => idx(a) - idx(b));
      const lead = focusedId ?? sel[0] ?? activeItemId;
      const actionSel =
        focusedId !== null
          ? sel.includes(focusedId)
            ? sel
            : [focusedId]
          : sel.length > 0
            ? sel
            : activeItemId !== null && order.has(activeItemId)
              ? [activeItemId]
              : [];

      if (e.ctrlKey || e.metaKey) {
        const key = e.key.toLowerCase();
        if (key === "k") {
          e.preventDefault();
          setShowSwitcher(true);
          return;
        }
        if (key === "/") {
          e.preventDefault();
          setShowShortcuts(true);
          return;
        }
        if (key === "c" && actionSel.length > 0) {
          e.preventDefault();
          copySelection(state, actionSel, notify);
          return;
        }
        if (key === "m" && e.shiftKey && actionSel.length > 0) {
          e.preventDefault();
          copyAsList(state, actionSel, notify);
          return;
        }
      }
      switch (e.key) {
        case "ArrowDown":
        case "ArrowUp": {
          e.preventDefault();
          if (visible.length === 0) break;
          const delta = e.key === "ArrowDown" ? 1 : -1;
          const at = idx(rovingId ?? -1);
          let next: number;
          if (at < 0) {
            next = delta > 0 ? visible[0].id : visible[visible.length - 1].id;
          } else {
            next = visible[Math.min(visible.length - 1, Math.max(0, at + delta))].id;
          }
          if (e.shiftKey && anchorId !== null && idx(anchorId) >= 0) {
            const a = idx(anchorId);
            const b = idx(next);
            setSelectedIds(new Set(visible.slice(Math.min(a, b), Math.max(a, b) + 1).map((i) => i.id)));
          } else {
            setSelectedIds(new Set([next]));
            setAnchorId(next);
            markActive(next);
          }
          // Focus follows the cursor. Without this the ring, the selection and
          // the next Space keypress were all describing different cards.
          focusCard(next);
          break;
        }
        case "Enter": {
          // On a control, Enter is that control's own command.
          if (!onControl && actionSel.length > 0) {
            e.preventDefault();
            copySelection(state, actionSel, notify);
          }
          break;
        }
        case "Delete":
        case "Backspace": {
          if (actionSel.length > 0) {
            e.preventDefault();
            deleteItems(actionSel);
          }
          break;
        }
        case " ": {
          // Space marks done, as in Copper. On a control it is that control's
          // own key, and running both would toggle the same item twice.
          if (onControl || actionSel.length === 0) break;
          e.preventDefault();
          const leadItem = state.items.find((i) => i.id === lead);
          const next = leadItem ? leadItem.done === 0 : true;
          for (const id of actionSel) void api.setDone(id, next);
          refresh();
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
    order,
    visible,
    rovingId,
    activeItemId,
    selectedIds,
    anchorId,
    menu,
    notify,
    markActive,
    refresh,
    deleteItems,
    showSwitcher,
    showOverflow,
    showShortcuts,
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

  // The composer holds focus by default, per the blueprint: "Typing — Goes to
  // the composer". Without this the panel draws a focus ring on
  // `.composer-row:focus-within` while nothing inside it is focused, so a
  // keyboard-first panel looks ready and the first keystrokes go nowhere.
  //
  // Focus is re-asserted on every `panel-shown` rather than on mount alone: the
  // panel window is hidden and shown, never unmounted, so mount fires once and
  // the second and later openings would silently lose focus again.
  useEffect(() => {
    const focusComposer = () => {
      const composer = composerRef.current;
      if (!composer || composer.disabled) return;
      // The window is shown and focused by the time this arrives, but WebView2
      // can still report the webview as unfocused for a tick, and `focus()` is
      // then a no-op.
      window.setTimeout(() => composer.focus(), 0);
    };

    focusComposer();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("panel-shown", focusComposer).then((off) => {
      if (cancelled) off();
      else unlisten = off;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const onDragOver = (e: DragEvent) => {
      if (e.dataTransfer?.types.includes("Files")) {
        e.preventDefault();
        if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      }
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
      if (added > 0) notify(added === 1 ? "Added 1 file" : `Added ${added} files`);
    };
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("dragover", onDragOver);
      window.removeEventListener("drop", onDrop);
    };
  }, [notify, refresh]);

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
    if (!state.license.canUse) {
      notify(state.license.message || "Activate Bard to add captures.", "error");
      return;
    }
    const v = composer.trim();
    if (!v) return;
    if (v.startsWith("#")) {
      void api.createSection(v.replace(/^#+\s*/, "")).then(() => notify("Section created"));
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
      const a = order.get(anchorId) ?? -1;
      const b = order.get(item.id) ?? -1;
      if (a >= 0 && b >= 0) {
        setSelectedIds(new Set(visible.slice(Math.min(a, b), Math.max(a, b) + 1).map((i) => i.id)));
      }
      return;
    }
    setSelectedIds(new Set([item.id]));
    setAnchorId(item.id);
  };

  const openContextMenu = (item: Item, x: number, y: number) => {
    let ids = [item.id];
    if (selectedIds.has(item.id) && selectedIds.size > 1) {
      ids = [...selectedIds];
    } else {
      markActive(item.id);
    }
    setMenu({ x, y, ids, item });
  };

  const onContextMenu = (item: Item, e: ReactMouseEvent) => {
    e.preventDefault();
    openContextMenu(item, e.clientX, e.clientY);
  };

  const onMenuAction = (action: MenuAction) => {
    const multi = menu?.ids.length ? menu.ids.length > 1 : false;
    if (!menu) return;
    const ids = menu.ids;
    switch (action) {
      case "copy":
        copySelection(state, ids, notify);
        if (ids.length === 1) markActive(ids[0]);
        break;
      case "copyList":
        copyAsList(state, ids, notify);
        break;
      case "favorite":
        for (const id of ids) {
          const it = state.items.find((i) => i.id === id);
          if (it) void api.setFavorite(id, multi ? !menu.item?.favorite : !it.favorite);
        }
        refresh();
        break;
      case "edit":
        if (menu.item && menu.item.kind === "text") setEditingId(menu.item.id);
        break;
      case "caption":
        if (menu.item?.kind === "image") void api.addNoteToImage(menu.item.id).then(refresh);
        break;
      case "delete":
        deleteItems(ids);
        break;
    }
    setMenu(null);
  };

  const activeSectionName =
    state.sections.find((s) => s.id === state.activeSectionId)?.name ?? null;

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
        <div className="header-actions" role="group" aria-label="Bard tools">
          {/* Draw and Settings are toggles: a second click puts the window
              away again, and each reads as pressed while its window is out. */}
          <button
            type="button"
            className={`icon-btn header-action${companions.draw ? " active" : ""}`}
            aria-label="Draw"
            aria-pressed={companions.draw}
            title="Draw"
            onClick={() => void api.toggleCompanion("draw")}
          >
            <Paintbrush size={16} strokeWidth={1.8} />
          </button>
          <button
            type="button"
            className={`icon-btn header-action${companions.settings ? " active" : ""}`}
            aria-label="Settings"
            aria-pressed={companions.settings}
            title="Settings"
            onClick={() => void api.toggleCompanion("settings")}
          >
            <Settings2 size={16} strokeWidth={1.8} />
          </button>
          <button
            type="button"
            className="icon-btn header-action"
            aria-label={doneCount > 0 ? `Clear ${doneCount} completed` : "Clear completed"}
            title={doneCount > 0 ? `Clear completed (${doneCount})` : "Clear completed"}
            disabled={doneCount === 0}
            onClick={() => {
              void api.clearCompleted().then(refresh);
              notify(`Cleared ${doneCount} completed`);
            }}
          >
            <ListChecks size={16} strokeWidth={1.8} />
          </button>
          <button
            type="button"
            className={`icon-btn header-action${showOverflow ? " active" : ""}`}
            aria-label="Sort"
            title="Sort"
            aria-haspopup="menu"
            aria-expanded={showOverflow}
            onClick={() => setShowOverflow((v) => !v)}
          >
            <ArrowUpDown size={16} strokeWidth={1.8} />
          </button>
        </div>
      </header>

      <div className="resize-layer">
        {RESIZE_DIRS.map(([cls, dir]) => (
          <div key={cls} className={`resize-handle h-${cls}`} onMouseDown={onResizeStart(dir)} />
        ))}
      </div>

      {/* The listbox holds the whole timeline: notes, drawings, images and
          dictations alike, which is why it is not labelled "Captured items".
          Its one tab stop is `rovingId`; the arrows move focus with it. */}
      <div
        ref={timelineRef}
        className="timeline"
        role="listbox"
        aria-label="Timeline"
        aria-multiselectable="true"
      >
        {state.items.length === 0 ? (
          <EmptyState section={activeSectionName} />
        ) : grouped.length === 0 ? (
          <div className="empty-state">
            <div className="title">No matches</div>
            <div className="hint">Nothing here matches “{query}”.</div>
          </div>
        ) : (
          grouped.map((group) => {
            const groupId = group.section ? `section-${group.section.id}` : "section-inbox";
            return (
              <div key={groupId} role="group" aria-label={group.section ? group.section.name : "Inbox"}>
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
                    attachments={attachmentsByItem.get(it.id) ?? EMPTY_ATTACHMENTS}
                    selected={selectedIds.has(it.id)}
                    active={rovingId === it.id}
                    fresh={freshId === it.id}
                    editing={editingId === it.id}
                    search={query}
                    onClick={(e) => onItemClick(it, e)}
                    onFocus={() => markActive(it.id)}
                    onContextMenu={(e) => onContextMenu(it, e)}
                    onOpenContextMenu={(event) => {
                      const rect = event.currentTarget.getBoundingClientRect();
                      openContextMenu(it, rect.left + rect.width / 2, rect.top + rect.height / 2);
                    }}
                    onCopyImage={() => {
                      copySelection(state, [it.id], notify);
                      markActive(it.id);
                    }}
                    onToggleDone={() => {
                      void api.setDone(it.id, it.done === 0).then(refresh);
                    }}
                    onDelete={() => deleteItems([it.id])}
                    onEdit={() => setEditingId(it.id)}
                    onEditDone={() => setEditingId(null)}
                  />
                ))}
              </div>
            );
          })
        )}
      </div>

      {/* The foot is a positioning context, so the toast sits directly above the
          composer at any composer height — it was pinned to `bottom: 14px`,
          which put it on top of the field you were typing into. */}
      <div className="panel-foot">
        {state.license.state === "restricted" ? (
          <div className="license-gate" role="status">
            <span>{state.license.message || "Activate Bard to continue capturing."}</span>
            <button type="button" className="btn" onClick={() => void api.openSettings()}>Activate</button>
          </div>
        ) : null}

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
              aria-label="Attach"
              disabled={!state.license.canUse}
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
                void addFiles(files, notify);
                e.target.value = "";
              }}
            />
            <textarea
              ref={composerRef}
              className="composer-input"
              rows={1}
              disabled={!state.license.canUse}
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

        {/* The live region is created on the first paint and stays empty until
            there is something to say: a region created with its content is not
            reliably announced, and a 2.6-second one is removed before the
            announcement has been read. */}
        <div
          className="toast-layer"
          role="status"
          aria-live={toast?.tone === "error" ? "assertive" : "polite"}
          aria-atomic="true"
        >
          {toast ? (
            <div className={`toast${toast.tone === "error" ? " error" : ""}`}>
              <span className="toast-text">{toast.text}</span>
              {toast.tone === "error" ? (
                <button
                  type="button"
                  className="toast-dismiss"
                  aria-label="Dismiss"
                  onClick={dismissToast}
                >
                  <X size={13} strokeWidth={2} aria-hidden="true" />
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>

      {showOverflow ? (
        <Menu
          label="Sort"
          onClose={closeOverflow}
          entries={SORT_ORDERS.map(([orderName, label]) => ({
            label,
            radio: true,
            checked: state.sort === orderName,
            trailing: state.sort === orderName ? "✓" : undefined,
            onClick: () => void api.setSort(orderName).then(refresh),
          }))}
        />
      ) : null}

      {composerMenu ? (
        <Menu
          anchor="point"
          at={composerMenu}
          onClose={closeComposerMenu}
          entries={[
            { label: "Attach", onClick: () => fileInputRef.current?.click() },
          ]}
        />
      ) : null}

      {showSwitcher ? (
        <SectionSwitcher
          state={state}
          onClose={closeSwitcher}
          onPicked={(id) => {
            void api.setActiveSection(id);
            setShowSwitcher(false);
            refresh();
          }}
        />
      ) : null}

      {showShortcuts ? <ShortcutSheet onClose={closeShortcuts} /> : null}

      {menu ? (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          multi={menu.ids.length > 1}
          item={menu.item}
          onAction={onMenuAction}
          onClose={closeMenu}
        />
      ) : null}
    </>
  );
}

/** Shared by every card with no attachments, so the prop keeps one identity. */
const EMPTY_ATTACHMENTS: Attachment[] = [];

/** The timeline's orders, in the menu's order. Newest first is the default. */
const SORT_ORDERS: [SortOrder, string][] = [
  ["newest", "Newest first"],
  ["oldest", "Oldest first"],
  ["favorites", "Favorites first"],
  ["completed", "Completed last"],
];

/*
  The fixed gesture map, read off gesture.rs: Left Shift captures, Ctrl
  dictates, Right Shift opens the panel, Left Alt draws, Right Alt opens
  Settings. The sheet is the only place in the app that states all of them, so
  a row that names the wrong side teaches the wrong gesture.
*/
const SHORTCUT_GROUPS: { name: string; rows: { keys: string[]; what: string }[] }[] = [
  {
    name: "Gestures",
    rows: [
      { keys: ["Left Shift", "Left Shift"], what: "Capture the selection" },
      { keys: ["Ctrl", "Ctrl"], what: "Dictate — hold, or tap twice" },
      { keys: ["Right Shift", "Right Shift"], what: "Show or hide Bard" },
      { keys: ["Right Shift", "Right Shift", "1 / 2 / 3"], what: "Paste a timeline item" },
      { keys: ["Left Alt", "Left Alt"], what: "Draw" },
      { keys: ["Right Alt", "Right Alt"], what: "Settings" },
      { keys: ["F11"], what: "Fullscreen Draw" },
    ],
  },
  {
    name: "Keys",
    rows: [
      { keys: ["Enter"], what: "Copy" },
      { keys: ["Ctrl", "Shift", "M"], what: "Copy as list" },
      { keys: ["Ctrl", "K"], what: "Sections" },
      { keys: ["Ctrl", "/"], what: "These shortcuts" },
      { keys: ["Space"], what: "Done" },
      { keys: ["Esc"], what: "Close" },
    ],
  },
];

/**
 * The shortcut sheet is a modal, so it behaves like one: a scrim that the
 * timeline cannot be clicked through, focus that starts inside and cannot Tab
 * out, Escape to leave, and the focus returned to wherever it came from. The
 * previous version declared `aria-modal` and none of that.
 */
function ShortcutSheet({ onClose }: { onClose: () => void }) {
  const cardRef = useRef<HTMLDivElement>(null);
  const restoreFocusTo = useRef<HTMLElement | null>(null);

  useEffect(() => {
    restoreFocusTo.current = document.activeElement as HTMLElement | null;
    // Initial focus on the close control: the sheet is a list to read, not a
    // form, so there is nothing inside it worth landing on first.
    cardRef.current?.querySelector<HTMLElement>("button")?.focus();

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      const card = cardRef.current;
      if (event.key !== "Tab" || !card) return;
      const focusable = [
        ...card.querySelectorAll<HTMLElement>(
          'button:not(:disabled), [href], input:not(:disabled), select, textarea, [tabindex]:not([tabindex="-1"])',
        ),
      ];
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const current = document.activeElement;
      const inside = current !== null && card.contains(current);
      if (event.shiftKey && (!inside || current === first)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (!inside || current === last)) {
        event.preventDefault();
        first.focus();
      }
    };

    // Capture: the panel's own handler would otherwise hide the whole window
    // before the sheet ever saw the key.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      restoreFocusTo.current?.focus();
    };
  }, [onClose]);

  return (
    <div className="sheet" onClick={onClose}>
      <div
        ref={cardRef}
        className="sheet-card shortcuts"
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcut-sheet-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="shortcut-sheet-head">
          <h2 id="shortcut-sheet-title">Shortcuts</h2>
          <button
            type="button"
            className="icon-btn sheet-head-close"
            aria-label="Close shortcuts"
            onClick={onClose}
          >
            <X aria-hidden="true" />
          </button>
        </div>
        <div className="shortcut-groups">
          {SHORTCUT_GROUPS.map((group) => (
            <div key={group.name} className="shortcut-group" role="group" aria-label={group.name}>
              <h3 className="shortcut-group-name">{group.name}</h3>
              {group.rows.map((row) => (
                <div key={`${group.name}-${row.keys.join("-")}`} className="shortcut-row">
                  <span className="shortcut-keys">
                    {row.keys.map((key, index) => (
                      <kbd key={`${key}-${index}`}>{key}</kbd>
                    ))}
                  </span>
                  <span className="shortcut-what">{row.what}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function filterItems(items: Item[], query: string): Item[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((it) => {
    if (it.kind === "image") return false;
    return it.content.toLowerCase().includes(q);
  });
}

function groupBySection(
  items: Item[],
  sections: Section[],
  activeSectionId: number | null,
): { section: Section | null; items: Item[] }[] {
  const ordered = [...sections];
  if (activeSectionId !== null) {
    ordered.sort((a, b) => (a.id === activeSectionId ? -1 : b.id === activeSectionId ? 1 : a.id - b.id));
  }
  const out: { section: Section | null; items: Item[] }[] = [];
  const inbox = items.filter((i) => i.sectionId === null);
  if (inbox.length > 0) out.push({ section: null, items: inbox });
  for (const section of ordered) {
    const group = items.filter((i) => i.sectionId === section.id);
    if (group.length > 0) out.push({ section, items: group });
  }
  return out;
}

function copySelection(state: AppState, ids: number[], notify: Notify) {
  if (ids.length === 0) return;
  const items = ids
    .map((id) => state.items.find((i) => i.id === id))
    .filter((i): i is Item => !!i);
  // Rust verifies the clipboard write and returns an error when it could not be
  // confirmed, so an unhandled rejection here used to leave the panel playing a
  // success cue and claiming "Copied" for a copy that never happened. A person
  // who walks away from a confirmed copy pastes whatever was there before.
  const reportFailure = (reason: unknown) => {
    playCue("error");
    const detail = reason instanceof Error ? reason.message : String(reason ?? "");
    notify(detail || "Couldn't copy.", "error");
  };

  if (items.length === 1) {
    const it = items[0];
    const write = it.kind === "image" ? api.copyItem(it.id) : api.copyText(it.content);
    void write
      .then(() => {
        playCue("copy");
        notify("Copied");
      })
      .catch(reportFailure);
    return;
  }
  void api
    .copyText(
      items.map((i) => (i.kind === "text" ? `- ${i.content}` : `- [image]`)).join("\n")
    )
    .then(() => {
      playCue("copy");
      notify("Copied as list");
    })
    .catch(reportFailure);
}

function copyAsList(state: AppState, ids: number[], notify: Notify) {
  const markdown = ids
    .map((id) => state.items.find((item) => item.id === id))
    .filter((item): item is Item => !!item)
    .map((item) => (item.kind === "text" ? `- ${item.content}` : "- [image]"))
    .join("\n");
  if (!markdown) return;
  // The same unhandled rejection `copySelection` had: a failed list write said
  // nothing at all, and left the last "Copied" standing as if it had worked.
  void api
    .copyText(markdown)
    .then(() => {
      playCue("copy");
      notify("Copied as list");
    })
    .catch((reason: unknown) => {
      playCue("error");
      const detail = reason instanceof Error ? reason.message : String(reason ?? "");
      notify(detail || "Couldn't copy.", "error");
    });
}

/**
 * Add dropped or picked files. Images become image items; text files become
 * notes from their contents; anything else becomes a note naming the file.
 */
async function addFiles(files: File[], notify: Notify) {
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
  notify(files.length === 1 ? "Added 1 file" : `Added ${files.length} files`);
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

/*
  The empty state asks about the section you are actually in — "What should we
  build in ui?" rather than a generic nothing-here. It is the one place the app
  addresses you, so it stays a question, and the hint under it exists only
  because a first-run user cannot know the capture gesture yet. It names Left
  Shift: Right Shift opens the panel, and a user who taps the wrong one decides
  the product is broken.
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
        Select text anywhere, then tap <kbd>Left Shift</kbd> twice.
      </div>
    </div>
  );
}
