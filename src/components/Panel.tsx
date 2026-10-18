import { Ellipsis, Eraser, Package, PenTool, Search, Settings, Star, X } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import type { DictationState } from "../App";
import { playCue } from "../audio";
import {
  clampPage,
  escapeOutcome,
  filterItems,
  nextIndex,
  pageCount,
  pageItems,
  pageSize,
  shelfCommand,
  TYPE_FILTERS,
  type TypeFilter,
} from "../shelf";
import { api } from "../store";
import type { AppState, Attachment, Item, ShelfEdge, SortOrder } from "../types";
import { InlineConfirm } from "../settings/ui";
import ColorPreview from "./ColorPreview";
import ContextMenu, { type MenuAction } from "./ContextMenu";
import FilterSwitcher from "./FilterSwitcher";
import ItemCard from "./ItemCard";
import Menu from "./Menu";
import Pager from "./Pager";
import TypeChips from "./TypeChips";

/** A confirmation is a receipt. An error is the app reporting a failure. */
export type ToastTone = "info" | "error";

/** What every message in the shelf goes through, however it was raised. */
type Notify = (text: string, tone?: ToastTone) => void;

/** The `bard-toast` payload: a bare string is a confirmation. */
export interface ToastDetail {
  text: string;
  tone?: ToastTone;
}

/** A length from a computed custom property, with a floor under bad input. */
function px(value: string, fallback: number): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export default function Panel({
  state,
  dictation,
  onState,
  activeItemId = null,
  onActiveItemChange,
  edge,
}: {
  state: AppState;
  dictation: DictationState;
  onState: (s: AppState) => void;
  activeItemId?: number | null;
  onActiveItemChange?: (id: number | null) => void;
  /** Which edge the window is docked to; the layout follows it. */
  edge: ShelfEdge;
}) {
  const vertical = edge === "left" || edge === "right";

  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<TypeFilter>("all");
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [page, setPage] = useState(0);
  const [perPage, setPerPage] = useState(5);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [anchorId, setAnchorId] = useState<number | null>(null);
  const [composer, setComposer] = useState("");
  const [searchOpen, setSearchOpen] = useState(true);
  const [showFilters, setShowFilters] = useState(false);
  const [showOverflow, setShowOverflow] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [preview, setPreview] = useState<Item | null>(null);
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
  // Which companions are out, so their rail buttons read as pressed.
  const [companions, setCompanions] = useState({ draw: false, settings: false });
  // The one capture that has not been on screen yet, so only it animates in.
  const [freshId, setFreshId] = useState<number | null>(null);

  const composerRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const pageRef = useRef<HTMLDivElement>(null);
  const packsRef = useRef<HTMLElement>(null);
  const toastTimer = useRef<number | undefined>(undefined);
  // The last id the backend was told about, so focus, clicks and arrow keys all
  // cost one write between them rather than one each.
  const activeReported = useRef<number | null>(activeItemId);
  const seenIds = useRef<Set<number> | null>(null);
  /**
   * The item to focus once a re-render has happened: an id, `undefined` for
   * "the shelf is now empty, put focus in the composer", or `null` for
   * "nothing to do".
   */
  const focusAfterRender = useRef<number | undefined | null>(null);

  const attachmentsByItem = useMemo(() => {
    const index = new Map<number, Attachment[]>();
    for (const attachment of state.attachments) {
      const list = index.get(attachment.itemId);
      if (list) list.push(attachment);
      else index.set(attachment.itemId, [attachment]);
    }
    return index;
  }, [state.attachments]);

  const visible = useMemo(
    () => filterItems(state.items, query, filter, favoritesOnly),
    [state.items, query, filter, favoritesOnly],
  );

  /** Visible order, by id. Every arrow key and every range is a lookup here. */
  const order = useMemo(() => {
    const index = new Map<number, number>();
    visible.forEach((item, position) => index.set(item.id, position));
    return index;
  }, [visible]);

  const pages = pageCount(visible.length, perPage);
  const currentPage = Math.min(page, pages - 1);
  const pageCards = useMemo(
    () => pageItems(visible, currentPage, perPage),
    [visible, currentPage, perPage],
  );

  const counts = useMemo(() => {
    const next = {} as Record<TypeFilter, number>;
    for (const chip of TYPE_FILTERS) {
      next[chip.id] = filterItems(state.items, query, chip.id, favoritesOnly).length;
    }
    return next;
  }, [state.items, query, favoritesOnly]);

  const doneCount = useMemo(
    () => state.items.reduce((count, item) => (item.done !== 0 ? count + 1 : count), 0),
    [state.items],
  );

  /**
   * Roving tabindex: the card the arrows are pointing at is the zone's only
   * tab stop. When the active card is not on this page, the first card takes
   * it — a list whose tab stop is missing cannot be tabbed into at all.
   */
  const rovingId = useMemo(() => {
    if (activeItemId !== null && order.has(activeItemId) && pageCards.some((item) => item.id === activeItemId)) {
      return activeItemId;
    }
    return pageCards[0]?.id ?? null;
  }, [activeItemId, order, pageCards]);

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

  /** Move DOM focus to a card. */
  const focusCard = useCallback((id: number) => {
    pageRef.current?.querySelector<HTMLElement>(`[data-item-id="${id}"]`)?.focus();
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

  // Every overlay takes a stable close: the menus and the sheets restore focus
  // and re-focus their first row when `onClose` changes, so a new closure on
  // each render makes them thrash.
  const closeMenu = useCallback(() => setMenu(null), []);
  const closeOverflow = useCallback(() => setShowOverflow(false), []);
  const closeComposerMenu = useCallback(() => setComposerMenu(null), []);
  const closeFilters = useCallback(() => setShowFilters(false), []);
  const closeShortcuts = useCallback(() => setShowShortcuts(false), []);

  /*
    How many cards fit the zone is measured, not assumed: the zones of the four
    edges are differently sized windows, and the rule is that a page shows only
    whole cards — a clipped card is a scroller Bard does not have. The card's
    own size and the gap come from the tokens, so the maths and the layout can
    never drift apart.
  */
  useEffect(() => {
    const zone = pageRef.current;
    if (!zone) return;
    const measure = () => {
      const style = getComputedStyle(zone);
      const gap = px(style.getPropertyValue("--shelf-card-gap"), 12);
      const card = vertical
        ? px(style.getPropertyValue("--shelf-card-h"), 148)
        : px(style.getPropertyValue("--shelf-card-w"), 236);
      const fit = vertical ? zone.clientHeight : zone.clientWidth;
      setPerPage(pageSize(fit, card, gap));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(zone);
    return () => observer.disconnect();
  }, [vertical]);

  /*
    One delete path. Every call site used to do the same five things, and each
    of them dropped the cursor on the floor: the focused card unmounted, focus
    fell to the document, and the next Tab restarted at the search field. The
    position is recorded here and the neighbour is focused once the shorter
    list has actually rendered.
  */
  const deleteItems = useCallback(
    (ids: number[]) => {
      if (ids.length === 0) return;
      const visibleIds = visible.map((item) => item.id);
      const at = visibleIds.findIndex((id) => ids.includes(id));
      const remaining = visibleIds.filter((id) => !ids.includes(id));
      // `undefined` when the zone is now empty: the composer is the natural
      // place for focus, and returning without moving it drops the person on
      // `<body>`, which is the defect this was written to fix.
      focusAfterRender.current =
        at < 0 ? null : remaining.length === 0 ? undefined : remaining[Math.min(at, remaining.length - 1)];
      for (const id of ids) void api.deleteItem(id);
      setSelectedIds(new Set());
      if (activeItemId !== null && ids.includes(activeItemId)) markActive(null);
      refresh();
    },
    [visible, markActive, refresh, activeItemId],
  );

  useEffect(() => {
    const id = focusAfterRender.current;
    if (id === null) return;
    focusAfterRender.current = null;
    if (id === undefined) {
      composerRef.current?.focus();
      return;
    }
    focusCard(id);
  }, [pageCards, focusCard]);

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

  /*
    The one movement rule, shared by every key that moves the cursor. The page
    follows the card, so the end of a page is never a dead end; focus follows
    the card a frame later when it was not rendered yet.
  */
  const moveCursor = useCallback(
    (delta: -1 | 1, extend: boolean) => {
      if (visible.length === 0) return;
      const from = rovingId !== null ? order.get(rovingId) ?? -1 : -1;
      const next = nextIndex(from, delta, visible.length);
      if (next < 0) return;
      const nextItem = visible[next];
      if (extend && anchorId !== null && order.has(anchorId)) {
        const a = order.get(anchorId)!;
        setSelectedIds(new Set(visible.slice(Math.min(a, next), Math.max(a, next) + 1).map((i) => i.id)));
      } else {
        setSelectedIds(new Set([nextItem.id]));
        setAnchorId(nextItem.id);
        markActive(nextItem.id);
      }
      const nextPage = Math.floor(next / perPage);
      if (nextPage !== currentPage) {
        setPage(nextPage);
        focusAfterRender.current = nextItem.id;
      } else {
        focusCard(nextItem.id);
      }
    },
    [visible, order, rovingId, anchorId, perPage, currentPage, markActive, focusCard],
  );

  /** Change page and point the cursor at its first card. */
  const goToPage = useCallback(
    (next: number) => {
      const clamped = clampPage(next, visible.length, perPage);
      const first = pageItems(visible, clamped, perPage)[0];
      if (!first) return;
      setPage(clamped);
      setSelectedIds(new Set([first.id]));
      setAnchorId(first.id);
      markActive(first.id);
      if (clamped === currentPage) focusCard(first.id);
      else focusAfterRender.current = first.id;
    },
    [visible, perPage, currentPage, markActive, focusCard],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const typing =
        target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;

      // A confirmation is the first thing Escape answers, then any overlay.
      // Each overlay also owns its keys in the capture phase; this is the
      // belt for when focus has wandered outside it.
      if (confirmingClear) {
        if (e.key === "Escape") {
          e.preventDefault();
          setConfirmingClear(false);
        }
        return;
      }
      if (menu || showShortcuts || showOverflow || showFilters || preview) return;

      if (typing) {
        if (e.key !== "Escape") return;
        // Escape reads the fixed order even while a field has focus. A
        // non-empty search you cannot clear while the caret is in it is the
        // defect this replaces.
        switch (
          escapeOutcome({
            query,
            composer,
            composerFocused: target === composerRef.current,
            selectionSize: selectedIds.size,
          })
        ) {
          case "search":
            setQuery("");
            break;
          case "composer":
            setComposer("");
            break;
          case "selection":
            setSelectedIds(new Set());
            break;
          case "retract":
            void api.hidePanel();
            break;
        }
        return;
      }

      const command = shelfCommand(e);
      if (!command) {
        // Start typing anywhere and the search begins, as Paste's shelf does.
        // Space is not a letter: it is the Done key.
        if (
          !e.ctrlKey &&
          !e.metaKey &&
          !e.altKey &&
          !e.isComposing &&
          e.key.length === 1 &&
          e.key !== " "
        ) {
          e.preventDefault();
          setSearchOpen(true);
          setQuery((current) => current + e.key);
          window.setTimeout(() => {
            const input = searchRef.current;
            if (!input) return;
            input.focus();
            input.setSelectionRange(input.value.length, input.value.length);
          }, 0);
        }
        return;
      }

      if (command.type === "escape") {
        switch (
          escapeOutcome({ query, composer, composerFocused: false, selectionSize: selectedIds.size })
        ) {
          case "search":
            e.preventDefault();
            setQuery("");
            break;
          case "selection":
            e.preventDefault();
            setSelectedIds(new Set());
            break;
          case "retract":
            e.preventDefault();
            void api.hidePanel();
            break;
          case "composer":
            // The composer is not focused, so its draft is not what the
            // person is looking at. Nothing to clear here.
            break;
        }
        return;
      }

      /*
        A control outside a card — a chip, the pager, a rail button — owns its
        own keys; the shelf only claims what nothing else will. Inside a card,
        Enter and Space belong to the card's own controls.
      */
      const onCard = target.closest("[data-item-id]") !== null;
      const onControl =
        target.closest('button, a[href], [role="button"], input, textarea, select') !== null;
      if (command.type === "move" && onControl && !onCard) return;
      if ((command.type === "paste" || command.type === "toggleDone") && onControl) return;

      /*
        The acted-on item is the focused one, never the selection this render
        happened to capture — a Space or Enter used to run the card's own
        handler, then bubble here and act on the previous selection.
      */
      const row = target.closest<HTMLElement>("[data-item-id]");
      const focusedId = row?.dataset.itemId ? Number(row.dataset.itemId) : null;
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

      switch (command.type) {
        case "move":
          e.preventDefault();
          moveCursor(command.delta, command.extend);
          break;
        case "page":
          e.preventDefault();
          goToPage(currentPage + command.delta);
          break;
        case "paste":
          if (actionSel.length > 0) {
            e.preventDefault();
            copySelection(state, actionSel, notify, true);
          }
          break;
        case "pasteNth": {
          const card = pageCards[command.index];
          if (card) {
            e.preventDefault();
            copySelection(state, [card.id], notify, true);
          }
          break;
        }
        case "copy":
          if (actionSel.length > 0) {
            e.preventDefault();
            copySelection(state, actionSel, notify);
          }
          break;
        case "copyMarkdown":
          if (actionSel.length > 0) {
            e.preventDefault();
            copyAsList(state, actionSel, notify);
          }
          break;
        case "pin":
          if (actionSel.length > 0) {
            e.preventDefault();
            const leadItem = state.items.find((i) => i.id === lead);
            void setPins(actionSel, leadItem ? leadItem.pinnedAt === null : true, notify, refresh);
          }
          break;
        case "delete":
          if (actionSel.length > 0) {
            e.preventDefault();
            deleteItems(actionSel);
          }
          break;
        case "toggleDone": {
          if (actionSel.length === 0) break;
          e.preventDefault();
          const leadItem = state.items.find((i) => i.id === lead);
          const next = leadItem ? leadItem.done === 0 : true;
          for (const id of actionSel) void api.setDone(id, next);
          refresh();
          break;
        }
        case "filters":
          e.preventDefault();
          setShowFilters(true);
          break;
        case "shortcuts":
          e.preventDefault();
          setShowShortcuts(true);
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    state,
    query,
    composer,
    order,
    visible,
    rovingId,
    pageCards,
    perPage,
    currentPage,
    activeItemId,
    selectedIds,
    anchorId,
    menu,
    preview,
    confirmingClear,
    notify,
    markActive,
    refresh,
    deleteItems,
    moveCursor,
    goToPage,
    showFilters,
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

  // Opening the shelf lands in search: type to filter, Enter pastes the top
  // match. Re-asserted on every `panel-shown`, because the window is hidden
  // and shown, never unmounted.
  useEffect(() => {
    const focusSearch = () => {
      // WebView2 can report the webview unfocused for a tick after the native
      // window takes focus, and `focus()` is a no-op until it catches up.
      window.setTimeout(() => {
        setSearchOpen(true);
        searchRef.current?.focus();
        searchRef.current?.select();
      }, 0);
    };

    focusSearch();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("panel-shown", focusSearch).then((off) => {
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

  const submitComposer = () => {
    if (!state.license.canUse) {
      notify(state.license.message || "Activate Bard to add captures.", "error");
      return;
    }
    const v = composer.trim();
    if (!v) return;
    void api.addItem(v);
    setComposer("");
    refresh();
  };

  /** What a plain click on a card does: paste it and leave. */
  const pasteItem = (item: Item) => {
    setSelectedIds(new Set([item.id]));
    setAnchorId(item.id);
    markActive(item.id);
    copySelection(state, [item.id], notify, true);
  };

  const onItemClick = (item: Item, e: ReactMouseEvent) => {
    // The modifier clicks still select — that is how a mouse reaches the
    // multi-item commands — and every other click is the paste.
    if (e.ctrlKey || e.metaKey) {
      const next = new Set(selectedIds);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      setSelectedIds(next);
      setAnchorId(item.id);
      markActive(item.id);
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
    pasteItem(item);
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
      case "pin":
        void setPins(ids, menu.item?.pinnedAt === null, notify, refresh);
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

  const clearCaptures = () => {
    void api
      .clearCaptures()
      .then(() => {
        setConfirmingClear(false);
        notify("Cleared captures");
        refresh();
      })
      .catch((reason: unknown) => {
        setConfirmingClear(false);
        playCue("error");
        const detail = reason instanceof Error ? reason.message : String(reason ?? "");
        notify(detail || "Couldn't clear captures.", "error");
      });
  };

  const openSearch = () => {
    setSearchOpen(true);
    window.setTimeout(() => searchRef.current?.focus(), 0);
  };

  return (
    <>
      {/* The rail is the shelf's whole chrome: what the All zone is pointed at
          on the left, the tools it is not on the right. On the side edges it
          wraps, and the search collapses to an icon until it is wanted. */}
      <div className="shelf-rail">
        <div className="rail-start">
          <button
            type="button"
            className={`icon-btn rail-btn${favoritesOnly ? " active" : ""}`}
            aria-label="Favorites"
            aria-pressed={favoritesOnly}
            title="Show favorites only"
            onClick={() => {
              setFavoritesOnly((v) => !v);
              setPage(0);
            }}
          >
            <Star size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="icon-btn rail-btn"
            aria-label="Clear captures"
            title="Clear captures…"
            onClick={() => setConfirmingClear(true)}
          >
            <Eraser size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
          {vertical && !searchOpen ? (
            <button
              type="button"
              className="icon-btn rail-btn"
              aria-label="Search"
              title="Search"
              onClick={openSearch}
            >
              <Search size={16} strokeWidth={2} aria-hidden="true" />
            </button>
          ) : null}
        </div>

        {!vertical || searchOpen ? (
          <div className="search-field">
            <Search size={14} strokeWidth={2} aria-hidden="true" />
            <input
              ref={searchRef}
              className="search"
              placeholder="Search"
              value={query}
              aria-label="Search"
              onChange={(e) => {
                setQuery(e.target.value);
                setPage(0);
              }}
              onBlur={() => {
                if (vertical && !query) setSearchOpen(false);
              }}
              onKeyDown={(e) => {
                // Enter in the field pastes the card the arrows last pointed
                // at, or the first one; ArrowDown drops into the row.
                const target =
                  activeItemId !== null && order.has(activeItemId)
                    ? state.items.find((i) => i.id === activeItemId)
                    : pageCards[0];
                if (!target) return;
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setSelectedIds(new Set([target.id]));
                  setAnchorId(target.id);
                  markActive(target.id);
                  focusCard(target.id);
                } else if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  copySelection(state, [target.id], notify, true);
                }
              }}
            />
          </div>
        ) : null}

        <TypeChips
          value={filter}
          onChange={(next) => {
            setFilter(next);
            setPage(0);
          }}
        />

        <span className="rail-spring" />

        <div className="rail-end" role="group" aria-label="Bard tools">
          {/* Draw and Settings are toggles: a second click puts the window
              away again, and each reads as pressed while its window is out. */}
          <button
            type="button"
            className={`icon-btn rail-btn${companions.draw ? " active" : ""}`}
            aria-label="Draw"
            aria-pressed={companions.draw}
            title="Draw"
            onClick={() => void api.toggleCompanion("draw")}
          >
            <PenTool size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button
            type="button"
            className={`icon-btn rail-btn${companions.settings ? " active" : ""}`}
            aria-label="Settings"
            aria-pressed={companions.settings}
            title="Settings"
            onClick={() => void api.toggleCompanion("settings")}
          >
            <Settings size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button
            type="button"
            className={`icon-btn rail-btn${showOverflow ? " active" : ""}`}
            aria-label="More"
            title="More"
            aria-haspopup="menu"
            aria-expanded={showOverflow}
            onClick={() => setShowOverflow((v) => !v)}
          >
            <Ellipsis size={16} strokeWidth={1.8} aria-hidden="true" />
          </button>
        </div>
      </div>

      <div className="shelf-body">
        {/* The All zone: the current page of cards, and the pager. Its one tab
            stop is the roving card; the pager is its own group of buttons. */}
        <section className="shelf-zone all" aria-label="Cards">
          <div
            ref={pageRef}
            id="shelf-cards"
            className="shelf-page"
            role="listbox"
            aria-label="Cards"
            aria-multiselectable="true"
            // The container takes the tab stop when the zone is empty. Every
            // card is `tabIndex={-1}` in that case — there is no row to point
            // at — so without this the zone cannot be reached by Tab at all.
            tabIndex={rovingId === null ? 0 : -1}
          >
            {state.items.length === 0 ? (
              <EmptyState query={query} filter={filter} favoritesOnly={favoritesOnly} empty />
            ) : pageCards.length === 0 ? (
              <EmptyState query={query} filter={filter} favoritesOnly={favoritesOnly} empty={false} />
            ) : (
              pageCards.map((item, index) => (
                <ItemCard
                  key={item.id}
                  item={item}
                  index={index}
                  attachments={attachmentsByItem.get(item.id) ?? EMPTY_ATTACHMENTS}
                  selected={selectedIds.has(item.id)}
                  active={rovingId === item.id}
                  fresh={freshId === item.id}
                  editing={editingId === item.id}
                  search={query}
                  onClick={(e) => onItemClick(item, e)}
                  onFocus={() => markActive(item.id)}
                  onContextMenu={(e) => onContextMenu(item, e)}
                  onMore={({ x, y }) => openContextMenu(item, x, y)}
                  onToggleDone={() => {
                    void api.setDone(item.id, item.done === 0).then(refresh);
                  }}
                  onCopy={() => {
                    copySelection(state, [item.id], notify);
                    markActive(item.id);
                  }}
                  onFavorite={() => {
                    void api.setFavorite(item.id, item.favorite === 0);
                    refresh();
                  }}
                  onPreview={() => setPreview(item)}
                  onDelete={() => deleteItems([item.id])}
                  onEditDone={() => setEditingId(null)}
                />
              ))
            )}
          </div>
          <Pager page={currentPage} pages={pages} onPage={goToPage} label="Card pages" />
        </section>

        <div className="shelf-divider" aria-hidden="true" />

        {/* The packs shell: the zone, its label, and the empty state a new
            shelf shows until packs arrive. The dividers and the layout are
            final; the feature lands in a later lane. */}
        <section
          ref={packsRef}
          id="shelf-packs"
          className="shelf-zone packs"
          aria-label="Packs"
          tabIndex={0}
        >
          <h2 className="packs-title">Packs</h2>
          <div className="packs-empty">
            <Package size={18} strokeWidth={1.6} aria-hidden="true" />
            <div className="title">No packs yet</div>
            <div className="hint">Packs will hold the items you paste together.</div>
          </div>
        </section>
      </div>

      {/* The foot is a positioning context, so the toast and the confirmation
          sit directly above the composer whatever its height. */}
      <div className="shelf-foot">
        {state.license.state === "restricted" ? (
          <div className="license-gate" role="status">
            <span>{state.license.message || "Activate Bard to continue capturing."}</span>
            <button type="button" className="btn" onClick={() => void api.openSettings()}>Activate</button>
          </div>
        ) : null}

        {confirmingClear ? (
          <InlineConfirm
            confirmLabel="Clear captures"
            onCancel={() => setConfirmingClear(false)}
            onConfirm={clearCaptures}
          >
            Remove automatic clipboard and dictation history? Your notes and favourites stay.
          </InlineConfirm>
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
            <input
              ref={composerRef}
              className="composer-input"
              type="text"
              disabled={!state.license.canUse}
              placeholder="Add a note"
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
          label="More"
          onClose={closeOverflow}
          entries={[
            ...SORT_ORDERS.map(([orderName, label]) => ({
              label,
              radio: true,
              checked: state.sort === orderName,
              trailing: state.sort === orderName ? "✓" : undefined,
              onClick: () => void api.setSort(orderName).then(refresh),
            })),
            {
              label: "Clear completed",
              separated: true,
              disabled: doneCount === 0,
              trailing: doneCount > 0 ? String(doneCount) : undefined,
              onClick: () => {
                void api.clearCompleted().then(refresh);
                notify(`Cleared ${doneCount} completed`);
              },
            },
            {
              label: "Keyboard shortcuts",
              trailing: "Ctrl /",
              onClick: () => setShowShortcuts(true),
            },
          ]}
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

      {showFilters ? (
        <FilterSwitcher
          value={filter}
          counts={counts}
          onClose={closeFilters}
          onPick={(next) => {
            setFilter(next);
            setPage(0);
            setShowFilters(false);
          }}
          onPacks={() => {
            setShowFilters(false);
            window.requestAnimationFrame(() => packsRef.current?.focus());
          }}
        />
      ) : null}

      {showShortcuts ? <ShortcutSheet onClose={closeShortcuts} /> : null}

      {preview ? (
        <ColorPreview item={preview} onClose={() => setPreview(null)} notify={notify} />
      ) : null}

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

/** The All zone's orders, in the menu's order. Newest first is the default. */
const SORT_ORDERS: [SortOrder, string][] = [
  ["newest", "Newest first"],
  ["oldest", "Oldest first"],
  ["favorites", "Favorites first"],
  ["completed", "Completed last"],
];

/*
  The fixed gesture map, read off gesture.rs: Left Shift captures, Ctrl
  dictates, Right Shift opens the shelf, Left Alt draws, Right Alt opens
  Settings. The sheet is the only place in the app that states all of them, so
  a row that names the wrong side teaches the wrong gesture.
*/
const SHORTCUT_GROUPS: { name: string; rows: { keys: string[]; what: string }[] }[] = [
  {
    name: "Gestures",
    rows: [
      { keys: ["Left Shift", "Left Shift"], what: "Capture the selection" },
      { keys: ["Ctrl", "Ctrl"], what: "Dictate — hold, or tap twice" },
      { keys: ["Right Shift", "Right Shift"], what: "Show or hide the shelf" },
      { keys: ["Right Shift", "Right Shift", "1 / 2 / 3"], what: "Paste one of the first three — pins first" },
      { keys: ["Left Alt", "Left Alt"], what: "Draw" },
      { keys: ["Right Alt", "Right Alt"], what: "Settings" },
      { keys: ["F11"], what: "Fullscreen Draw" },
    ],
  },
  {
    name: "Keys",
    rows: [
      { keys: ["Enter"], what: "Paste into your app" },
      { keys: ["Ctrl", "1–9"], what: "Paste that card" },
      { keys: ["←", "→"], what: "Move between cards" },
      { keys: ["Shift", "←/→"], what: "Extend the selection" },
      { keys: ["PgUp", "PgDn"], what: "Change page" },
      { keys: ["Ctrl", "C"], what: "Copy" },
      { keys: ["Ctrl", "Shift", "M"], what: "Copy as Markdown" },
      { keys: ["Ctrl", "P"], what: "Pin or unpin" },
      { keys: ["Ctrl", "K"], what: "Filters and packs" },
      { keys: ["Ctrl", "/"], what: "These shortcuts" },
      { keys: ["Space"], what: "Done" },
      { keys: ["Esc"], what: "Clear search, then selection, then close" },
    ],
  },
];

/**
 * The shortcut sheet is a modal, so it behaves like one: a scrim that the
 * cards cannot be clicked through, focus that starts inside and cannot Tab
 * out, Escape to leave, and the focus returned to wherever it came from.
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

    // Capture: the shelf's own handler would otherwise act on the key before
    // the sheet ever saw it.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (restoreFocusTo.current?.isConnected) restoreFocusTo.current.focus();
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

/**
 * The zone with nothing to show. It names the reason — nothing captured, no
 * matches, the favourite view, the active chip — because "nothing here" on a
 * shelf holding two hundred items is a lie.
 */
function EmptyState({
  query,
  filter,
  favoritesOnly,
  empty,
}: {
  query: string;
  filter: TypeFilter;
  favoritesOnly: boolean;
  empty: boolean;
}) {
  if (empty) {
    return (
      <div className="empty-state">
        <EmptyGlyph />
        <div className="title">What should we build?</div>
        <div className="hint">
          Select text anywhere, then tap <kbd>Left Shift</kbd> twice.
        </div>
      </div>
    );
  }
  if (query.trim()) {
    return (
      <div className="empty-state">
        <div className="title">No matches</div>
        <div className="hint">Nothing here matches “{query.trim()}”.</div>
      </div>
    );
  }
  if (favoritesOnly) {
    return (
      <div className="empty-state">
        <div className="title">No favorites yet</div>
        <div className="hint">Star an item and it stays within reach.</div>
      </div>
    );
  }
  const chip = TYPE_FILTERS.find((entry) => entry.id === filter);
  return (
    <div className="empty-state">
      <div className="title">No {chip ? chip.label.toLowerCase() : "cards"} yet</div>
      <div className="hint">Capture something and it lands here.</div>
    </div>
  );
}

function EmptyGlyph() {
  return (
    <svg className="empty-glyph" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="9.25" fill="none" strokeWidth="1.5" />
      <path d="M8.6 14.2a4.2 4.2 0 0 0 6.8 0" fill="none" strokeWidth="1.5"
            strokeLinecap="round" />
      <circle cx="9.2" cy="10" r="1" stroke="none" />
      <circle cx="14.8" cy="10" r="1" stroke="none" />
    </svg>
  );
}

/**
 * Pin or unpin in order, stopping at the first refusal: a fourth pin is
 * refused by Rust, and the toast says why rather than the list quietly not
 * changing.
 */
async function setPins(ids: number[], pinned: boolean, notify: Notify, refresh: () => void) {
  try {
    for (const id of ids) await api.setPinned(id, pinned);
  } catch (reason) {
    playCue("error");
    notify(reason instanceof Error ? reason.message : String(reason ?? "Bard could not save the pin."), "error");
  } finally {
    refresh();
  }
}

/**
 * Write the items to the clipboard. With `paste`, then hand focus back to the
 * app the shelf was opened over and paste there — CopyCat's Return. The paste
 * only happens after Rust has confirmed the clipboard write, so a failed write
 * never pastes whatever was on the clipboard before.
 */
function copySelection(state: AppState, ids: number[], notify: Notify, paste = false) {
  if (ids.length === 0) return;
  const items = ids
    .map((id) => state.items.find((i) => i.id === id))
    .filter((i): i is Item => !!i);
  // Rust verifies the clipboard write and returns an error when it could not be
  // confirmed, so an unhandled rejection here used to leave the shelf playing a
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
        if (paste) return api.pasteBack();
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
      if (paste) return api.pasteBack();
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
