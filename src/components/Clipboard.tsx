import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Check,
  Droplet,
  File as FileIcon,
  Image as ImageIcon,
  Link as LinkIcon,
  Mic,
  MoreHorizontal,
  Pause,
  PenTool,
  Pin,
  PinOff,
  Play,
  Search,
  Settings as SettingsIcon,
  Star,
  Trash2,
  Type,
  X,
  type LucideIcon,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type MouseEvent, type SetStateAction } from "react";
import { colorValue, linkParts } from "../content";
import {
  ago,
  agoLong,
  fileFolder,
  fileName,
  filePaths,
  filterShelf,
  itemKind,
  listCommand,
  nextIndex,
  TYPE_FILTERS,
  type ShelfKind,
  type TypeFilter,
} from "../shelf";
import { api } from "../store";
import type { AppState, Dock, Item, ShortcutState, SortOrder } from "../types";
import Menu, { type MenuEntry } from "./Menu";
import ResizeHandles from "./ResizeHandles";
import ShortcutSheet, { type ShortcutGroup } from "./ShortcutSheet";

const GLYPHS: Record<ShelfKind, LucideIcon> = {
  text: Type,
  image: ImageIcon,
  link: LinkIcon,
  color: Droplet,
  dictation: Mic,
  draw: PenTool,
  file: FileIcon,
};

/**
 * The orders the timeline can take. Newest first is the default; the others
 * exist because "no order they control" is the thing a clipboard history
 * usually gets wrong.
 */
const SORT_ORDERS: [SortOrder, string][] = [
  ["newest", "Newest first"],
  ["oldest", "Oldest first"],
  ["favorites", "Kept first"],
];

/**
 * How many rows to draw beyond the edge of the visible area.
 *
 * The list is windowed: a history can hold five thousand rows, and drawing
 * them all made every state change cost a full re-render of the shelf. A
 * screenful plus this many rows on each side keeps the scroll smooth without
 * a blank strip ever appearing under the pointer.
 */
const OVERSCAN = 8;

/**
 * The measured height of one row and of one group heading, in pixels.
 *
 * They are measured rather than written down so the window stays correct if
 * the type scale or the row's padding changes: a stale constant would put the
 * scrollbar out of step with the rows, and the list would jump when it was
 * scrolled.
 */
const FALLBACK_ROW_HEIGHT = 56;
const FALLBACK_HEADING_HEIGHT = 30;

/** How an item arrived, in the words a screen reader should read. */
const ARRIVED: Record<string, string> = {
  clipboard: "Copied",
  dictation: "Dictated",
  manual: "Added",
};

function reason(error: unknown, fallback: string): string {
  const text = String(error ?? "").replace(/^Error:\s*/i, "").trim();
  return text || fallback;
}

function Cover({ item }: { item: Item }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    void api.itemCoverUrl(item.id).then((next) => {
      if (alive) setUrl(next);
    });
    return () => {
      alive = false;
    };
  }, [item.id]);
  return url ? (
    <img className="item-cover" src={url} alt="" loading="lazy" draggable={false} />
  ) : (
    <span className="item-cover placeholder" />
  );
}

/** One item: its kind, its content, and when. Clicking copies it. */
const Row = memo(function Row({
  item,
  slot,
  active,
  current,
  position,
  total,
  editing,
  editText,
  onCopy,
  onMenu,
  onPin,
  onDelete,
  onEdit,
  onEditChange,
  onEditCommit,
  onEditCancel,
}: {
  item: Item;
  slot: number | null;
  active: boolean;
  current: boolean;
  position: number;
  total: number;
  editing: boolean;
  editText: string;
  onCopy: (item: Item, hide: boolean) => void;
  onMenu: (item: Item, at?: { x: number; y: number }) => void;
  onPin: (item: Item) => void;
  onDelete: (item: Item) => void;
  onEdit: (item: Item) => void;
  onEditChange: (value: string) => void;
  onEditCommit: () => void;
  onEditCancel: () => void;
}) {
  const kind = itemKind(item);
  const Glyph = GLYPHS[kind];
  const link = kind === "link" ? linkParts(item.content) : null;
  const color = kind === "color" ? colorValue(item.content) : null;
  const paths = kind === "file" ? filePaths(item) : [];
  const pinned = item.pinnedAt !== null;

  let body;
  if (kind === "image" || kind === "draw") {
    body = (
      <span className="item-media">
        <Cover item={item} />
        <span className="item-sub">{kind === "draw" ? "Drawing" : "Image"}</span>
      </span>
    );
  } else if (link) {
    body = (
      <span className="item-lines">
        <span className="item-main">{link.host}</span>
        {link.rest && <span className="item-sub">{link.rest}</span>}
      </span>
    );
  } else if (color) {
    body = (
      <span className="item-color">
        <span className="swatch" style={{ "--swatch": color } as React.CSSProperties} />
        <span className="item-main mono">{color}</span>
      </span>
    );
  } else if (kind === "file") {
    body = (
      <span className="item-lines">
        <span className="item-main">
          {fileName(paths[0] ?? "")}
          {paths.length > 1 && <span className="item-count">+{paths.length - 1}</span>}
        </span>
        <span className="item-sub">{fileFolder(paths[0] ?? "")}</span>
      </span>
    );
  } else {
    body = editing ? (
      <textarea
        className="item-edit"
        value={editText}
        aria-label="Edit this item"
        autoFocus
        spellCheck={false}
        onChange={(event) => onEditChange(event.target.value)}
        onKeyDown={(event) => {
          // Enter saves, Shift+Enter breaks the line, Escape puts the original
          // back. The panel's own key handling must not also act on these keys.
          event.stopPropagation();
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            onEditCommit();
            return;
          }
          if (event.key === "Escape") {
            event.preventDefault();
            onEditCancel();
          }
        }}
        onBlur={onEditCommit}
      />
    ) : (
      <span className="item-text">{item.content}</span>
    );
  }

  return (
    <li
      className={`item${active ? " is-active" : ""}${current ? " is-current" : ""}${
        editing ? " is-editing" : ""
      }`}
      data-kind={kind}
      role="option"
      aria-selected={active}
      aria-posinset={position + 1}
      aria-setsize={total}
      id={`item-${item.id}`}
      onClick={() => {
        if (editing) return;
        onCopy(item, false);
      }}
      onDoubleClick={(event) => {
        // A double click on text opens it for editing. On everything else it
        // copies and closes, which is the faster answer.
        event.preventDefault();
        if (kind === "text" || kind === "dictation") {
          onEdit(item);
          return;
        }
        onCopy(item, true);
      }}
      onContextMenu={(event: MouseEvent) => {
        event.preventDefault();
        onMenu(item, { x: event.clientX, y: event.clientY });
      }}
    >
      <span className="item-glyph" aria-hidden="true">
        <Glyph size={14} strokeWidth={2} />
      </span>
      <span className="item-body">{body}</span>
      {/* What the row means, in words. The compact time on the right is for
          the eye; this is for anyone listening, and it carries the moment the
          item arrived as well as the two states a row can be in. */}
      <span className="sr-only">
        {`${ARRIVED[item.origin] ?? "Added"} ${agoLong(item.createdAt)}.`}
        {current ? " On the clipboard now." : ""}
        {pinned ? " Pinned to a slot." : ""}
        {item.favorite !== 0 ? " Kept forever." : ""}
        {slot !== null ? ` In slot ${slot}.` : ""}
      </span>
      <span className="item-end">
        <span className="item-meta">
          {current && (
            <span className="badge current" title="On the clipboard">
              <Check size={11} strokeWidth={3} aria-hidden="true" />
            </span>
          )}
          {item.favorite !== 0 && <Star className="item-flag" size={12} fill="currentColor" aria-label="Kept forever" />}
          {pinned && <Pin className="item-flag" size={12} aria-label="Pinned" />}
          {slot !== null ? (
            <kbd className="badge slot" title={`Alt+Shift+${slot}`}>
              {slot}
            </kbd>
          ) : (
            <span className="item-time" title={agoLong(item.createdAt)}>
              {ago(item.createdAt)}
            </span>
          )}
        </span>
        {/* Pointer shortcuts only: the same actions are Ctrl+P, Delete and the
            row's menu, which the context-menu key opens, so these stay out of
            the tab order and out of the option's accessible content. */}
        <span className="item-actions" aria-hidden="true">
          <span
            className="item-btn"
            data-action={pinned ? "unpin" : "pin"}
            title={pinned ? "Unpin" : "Pin to slots 1–3"}
            onClick={(event) => {
              event.stopPropagation();
              onPin(item);
            }}
          >
            {pinned ? <PinOff size={14} /> : <Pin size={14} />}
          </span>
          <span
            className="item-btn"
            data-action="delete"
            title="Delete"
            onClick={(event) => {
              event.stopPropagation();
              onDelete(item);
            }}
          >
            <Trash2 size={14} />
          </span>
        </span>
      </span>
    </li>
  );
});

function guideGroups(state: ShortcutState | null): ShortcutGroup[] {
  const rows = state?.rows ?? [];
  const keys = (id: string) => {
    const row = rows.find((r) => r.id === id);
    if (!row || !row.display) return ["Off"];
    return row.display.split("+");
  };
  const row = (id: string, what: string) => ({ keys: keys(id), what });
  return [
    {
      name: "Anywhere",
      rows: [
        row("main", "Show or hide Bard"),
        row("dictate", "Dictate — hold to talk, tap to start and stop"),
        row("slot-1", "Copy item 1 (2 and 3 the same way)"),
        row("cycle", "Cycle through items 1–3"),
        row("preview", "Preview what is on the clipboard"),
        row("pause", "Pause or resume keeping what you copy"),
        row("draw", "Open Draw"),
        row("settings", "Open Settings"),
        row("guide", "Show this guide"),
      ],
      note:
        state?.backend === "portal"
          ? "Your desktop owns these shortcuts. Change them in Settings › Shortcuts."
          : "Escape cancels a dictation while it is listening. Change any shortcut in Settings › Shortcuts.",
    },
    {
      name: "In this window",
      rows: [
        { keys: ["↑", "↓"], what: "Move" },
        { keys: ["Enter"], what: "Copy and close" },
        { keys: ["Shift", "Enter"], what: "Copy and stay" },
        { keys: ["Ctrl", "P"], what: "Pin or unpin" },
        { keys: ["Ctrl", "S"], what: "Keep forever" },
        { keys: ["Delete"], what: "Delete" },
        { keys: ["Ctrl", "F"], what: "Search" },
        { keys: ["Esc"], what: "Clear search, then close" },
      ],
    },
  ];
}

export default function Clipboard({
  state,
  onState,
  activeItemId,
}: {
  state: AppState;
  /**
   * The panel's one way back into the app's state. It takes a function as well
   * as a value, so a change built from the state as it is now cannot be
   * overwritten by a snapshot taken before the change.
   */
  onState: Dispatch<SetStateAction<AppState | null>>;
  activeItemId: number | null;
}) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<TypeFilter>("all");
  const [cursor, setCursor] = useState(-1);
  const [menu, setMenu] = useState<{ entries: MenuEntry[]; at?: { x: number; y: number } } | null>(null);
  const [guide, setGuide] = useState(false);
  const [shortcuts, setShortcuts] = useState<ShortcutState | null>(null);
  const [rowHeight, setRowHeight] = useState(FALLBACK_ROW_HEIGHT);
  const [headingHeight, setHeadingHeight] = useState(FALLBACK_HEADING_HEIGHT);
  const [window_, setWindow] = useState({ start: 0, end: 0 });
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  // The edit's own bookkeeping, kept out of the state so saving an edit does
  // not re-render every row in the shelf.
  const editGuard = useRef(false);
  const editOriginal = useRef("");
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const docked = state.dock !== "float";

  const view = useMemo(() => filterShelf(state.items, query, filter), [state.items, query, filter]);
  const visible = view.visible;
  // The slot numbers come from the backend, which is where the shortcuts are
  // decided. Deriving them from the first three rows on screen would label a
  // row with a slot that copies something else whenever the timeline was
  // ordered differently from the slots.
  const slotIds = state.slotIds;
  const firstUnpinned = visible.findIndex((item) => item.pinnedAt === null);
  const hasPins = visible.length > 0 && visible[0].pinnedAt !== null;

  /**
   * Where every row starts, in pixels from the top of the list.
   *
   * The list is drawn a screenful at a time. The offsets are what make that
   * possible with rows of different heights: the two group headings take up
   * space of their own, and everything after them shifts down by that much.
   */
  const offsets = useMemo(() => {
    const marks = new Array<number>(visible.length + 1);
    let y = hasPins ? headingHeight : 0;
    marks[0] = y;
    for (let index = 0; index < visible.length; index++) {
      y += rowHeight;
      if (hasPins && index === firstUnpinned) y += headingHeight;
      marks[index + 1] = y;
    }
    return marks;
  }, [visible, rowHeight, headingHeight, hasPins, firstUnpinned]);

  const totalHeight = offsets[offsets.length - 1] ?? 0;

  /** The rows that cover the visible area, plus the cursor's row. */
  const windowFor = useCallback(
    (scrollTop: number, viewport: number) => {
      if (visible.length === 0) return { start: 0, end: 0 };
      const from = Math.max(0, scrollTop - OVERSCAN * rowHeight);
      const to = scrollTop + viewport + OVERSCAN * rowHeight;
      let start = 0;
      while (start < visible.length && offsets[start + 1] <= from) start++;
      let end = start;
      while (end < visible.length && offsets[end] < to) end++;
      // The keyboard cursor must always be on the page: the arrow keys move it,
      // and `aria-activedescendant` points at it.
      if (cursor >= 0) {
        if (cursor < start) start = cursor;
        if (cursor >= end) end = cursor + 1;
      }
      return { start, end };
    },
    [cursor, offsets, rowHeight, visible.length],
  );

  const measure = useCallback(() => {
    const scroll = scrollRef.current;
    if (!scroll) return;
    const row = scroll.querySelector<HTMLElement>(".item");
    if (row) {
      const height = Math.round(row.getBoundingClientRect().height);
      if (height > 0 && height !== rowHeight) setRowHeight(height);
    }
    const heading = scroll.querySelector<HTMLElement>(".clip-heading");
    if (heading) {
      const height = Math.round(heading.getBoundingClientRect().height);
      if (height > 0 && height !== headingHeight) setHeadingHeight(height);
    }
    setWindow(windowFor(scroll.scrollTop, scroll.clientHeight));
  }, [rowHeight, headingHeight, windowFor]);

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  const loadShortcuts = useCallback(() => {
    void api.getShortcuts().then(setShortcuts).catch(() => undefined);
  }, []);

  useEffect(() => {
    loadShortcuts();
    const offs = [
      listen("show-guide", () => {
        loadShortcuts();
        setGuide(true);
      }),
      listen("shortcuts-changed", loadShortcuts),
      listen("panel-shown", () => {
        setCursor(-1);
        listRef.current?.scrollTo({ top: 0 });
        requestAnimationFrame(() => searchRef.current?.focus());
      }),
    ];
    return () => {
      for (const off of offs) void off.then((fn) => fn());
    };
  }, [loadShortcuts]);

  useEffect(() => {
    setCursor(-1);
  }, [query, filter]);

  // The window is recalculated whenever the list, the row height or the
  // cursor changes, and whenever the panel is resized.
  useEffect(() => {
    measure();
    const scroll = scrollRef.current;
    if (!scroll || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => measure());
    observer.observe(scroll);
    return () => observer.disconnect();
  }, [measure, visible]);

  useEffect(() => {
    if (cursor < 0) return;
    const scroll = scrollRef.current;
    if (!scroll) return;
    // The cursor's row may not be on the page: End on a five hundred item
    // history moves the selection to the far end, which is nowhere near the
    // window that is drawn. So the scroll is moved first, by the row's own
    // offset, and the window is then recomputed from where the scroll is.
    // Doing it by offset rather than by looking the row up in the DOM is
    // deliberate: a row that is not drawn yet cannot be scrolled to, and
    // waiting for it to be drawn is a frame the person can see.
    const target = offsets[cursor];
    if (target === undefined) return;
    const view = scroll.clientHeight;
    const viewTop = scroll.scrollTop;
    const viewBottom = viewTop + view;
    let next = viewTop;
    if (target < viewTop) {
      next = target - 8;
    } else if (target + rowHeight > viewBottom) {
      next = target + rowHeight - view + 8;
    }
    const max = Math.max(0, scroll.scrollHeight - view);
    next = Math.max(0, Math.min(next, max));
    if (Math.abs(next - viewTop) > 1) {
      scroll.scrollTop = next;
    }
    setWindow(windowFor(next, view));
  }, [cursor, visible, offsets, rowHeight, windowFor]);

  const refresh = useCallback(() => {
    void api.getState().then(onState).catch(() => undefined);
  }, [onState]);

  const copy = useCallback(
    (item: Item, hide: boolean) => {
      void api
        .copyItem(item.id)
        .then(() => {
          if (hide && docked) void api.hidePanel();
        })
        .catch(() => undefined);
    },
    [docked],
  );

  const pin = useCallback(
    (item: Item) => {
      void api
        .setPinned(item.id, item.pinnedAt === null)
        .then(refresh)
        .catch((error) => void api.notifyError(reason(error, "Couldn't pin that item")));
    },
    [refresh],
  );

  const keep = useCallback(
    (item: Item) => {
      void api
        .setFavorite(item.id, item.favorite === 0)
        .then(refresh)
        .catch((error) => void api.notifyError(reason(error, "Couldn't keep that item")));
    },
    [refresh],
  );

  const remove = useCallback(
    (item: Item) => {
      // The update is written as a function of the current state, not of the
      // state this callback was built from. The old form spread a snapshot
      // taken at render time, so a capture that landed between that render and
      // this click was dropped from the shelf — silently, and only until the
      // next refresh put it back.
      onState((current) =>
        current
          ? { ...current, items: current.items.filter((existing) => existing.id !== item.id) }
          : current,
      );
      void api
        .deleteItem(item.id)
        .catch((error) => {
          void api.notifyError(reason(error, "Couldn't delete that item"));
          refresh();
        });
    },
    [onState, refresh],
  );

  /**
   * Open a captured text for editing.
   *
   * A copy is often almost right — a command with a stray newline, a URL with
   * a tracking parameter, a name with a typo. Being able to fix it in place
   * is the difference between a clipboard manager and a log.
   */
  const startEdit = useCallback((item: Item) => {
    editGuard.current = false;
    editOriginal.current = item.content;
    setEditingId(item.id);
    setEditText(item.content);
  }, []);

  /**
   * Put the keyboard back where it was.
   *
   * The editor is a field inside the row, and when it goes the focus goes
   * with it — to nothing at all, because the row it was in is still there but
   * the field is not. A keypress then lands on the page instead of on the
   * panel, and the arrow keys stop moving the selection. The list takes the
   * focus back, so the person carries on from the row they were editing.
   */
  const restoreListFocus = useCallback(() => {
    requestAnimationFrame(() => listRef.current?.focus());
  }, []);

  const commitEdit = useCallback(() => {
    // Saving happens from Enter and from the field losing the focus, so the
    // same edit can arrive twice. The first one wins.
    if (editGuard.current) return;
    const id = editingId;
    if (id === null) return;
    editGuard.current = true;
    const original = editOriginal.current;
    const next = editText;
    setEditingId(null);
    if (next === original) {
      restoreListFocus();
      return;
    }
    if (!next.trim()) {
      void api.notifyError("An empty item is not kept.");
      restoreListFocus();
      return;
    }
    onState((current) =>
      current
        ? {
            ...current,
            items: current.items.map((item) => (item.id === id ? { ...item, content: next } : item)),
          }
        : current,
    );
    void api
      .updateItem(id, next)
      .then(refresh)
      .catch((error) => {
        void api.notifyError(reason(error, "Couldn't save that change"));
        refresh();
      })
      .finally(restoreListFocus);
  }, [editingId, editText, onState, refresh, restoreListFocus]);

  const cancelEdit = useCallback(() => {
    editGuard.current = true;
    setEditingId(null);
    setEditText("");
    restoreListFocus();
  }, [restoreListFocus]);

  const openMenu = useCallback(
    (item: Item, at?: { x: number; y: number }) => {
      const kind = itemKind(item);
      const entries: MenuEntry[] = [
        { label: "Copy", onClick: () => copy(item, false), trailing: "Enter" },
      ];
      if (kind === "text" || kind === "dictation") {
        entries.push({ label: "Edit", onClick: () => startEdit(item), trailing: "F2" });
      }
      entries.push(
        { label: item.pinnedAt === null ? "Pin to slots" : "Unpin", onClick: () => pin(item), trailing: "Ctrl+P" },
        {
          label: item.favorite === 0 ? "Keep forever" : "Stop keeping forever",
          onClick: () => keep(item),
          trailing: "Ctrl+S",
        },
      );
      if (kind === "image" || kind === "draw") {
        entries.push({ label: "Open in Draw", onClick: () => void api.openDrawWithImage(item.id) });
      }
      entries.push({ label: "Delete", onClick: () => remove(item), danger: true, separated: true, trailing: "Del" });
      setMenu({ entries, at });
    },
    [copy, keep, pin, remove, startEdit],
  );

  /**
   * Open a row's menu from the keyboard, next to the row it belongs to.
   *
   * The pointer buttons on a row are decoration: they sit inside the row's
   * option and are hidden from assistive technology, so the menu is the only
   * way a person who cannot use a pointer reaches "Pin", "Keep forever" and
   * "Delete". The context-menu key is what opens it.
   */
  const openMenuAtCursor = useCallback(() => {
    const item = cursor >= 0 ? visible[cursor] : visible[0];
    if (!item) return;
    const row = document.getElementById(`item-${item.id}`);
    if (!row) {
      openMenu(item);
      return;
    }
    const rect = row.getBoundingClientRect();
    openMenu(item, { x: Math.round(rect.left + 24), y: Math.round(rect.bottom - 4) });
  }, [cursor, openMenu, visible]);

  const setDock = (dock: Dock) => {
    void api.setDock(dock).catch((error) => void api.notifyError(reason(error, "Couldn't move the window")));
  };

  const setSort = (order: SortOrder) => {
    void api
      .setSort(order)
      .then(refresh)
      .catch((error) => void api.notifyError(reason(error, "Couldn't change the order")));
  };

  const openMore = () => {
    const entries: MenuEntry[] = [
      { label: "Float", onClick: () => setDock("float"), checked: state.dock === "float", radio: true },
      { label: "Dock left", onClick: () => setDock("left"), checked: state.dock === "left", radio: true },
      { label: "Dock right", onClick: () => setDock("right"), checked: state.dock === "right", radio: true },
      ...SORT_ORDERS.map<MenuEntry>(([order, label]) => ({
        label,
        radio: true,
        checked: state.sort === order,
        onClick: () => setSort(order),
      })),
      {
        label: "Shortcuts",
        onClick: () => {
          loadShortcuts();
          setGuide(true);
        },
        trailing: "?",
        separated: true,
      },
    ];
    setMenu({ entries });
  };

  const togglePause = () => {
    void api
      .setCapturePaused(!state.capturePaused)
      .then(refresh)
      .catch((error) => void api.notifyError(reason(error, "Couldn't change capture")));
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (menu || guide) return;
    const inSearch = document.activeElement === searchRef.current;
    const command = listCommand({
      key: event.key,
      ctrlKey: event.ctrlKey,
      metaKey: event.metaKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      inSearch,
    });
    if (!command) {
      // Typing anywhere goes to search.
      if (!inSearch && event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
        searchRef.current?.focus();
      }
      return;
    }
    const item = cursor >= 0 ? visible[cursor] : visible[0];
    switch (command.type) {
      case "move":
        event.preventDefault();
        setCursor((current) => nextIndex(current, command.delta, visible.length));
        break;
      case "edge":
        event.preventDefault();
        setCursor(command.to === "first" ? 0 : visible.length - 1);
        break;
      case "copy":
        if (inSearch && event.key.toLowerCase() === "c") return;
        event.preventDefault();
        if (item) copy(item, command.hide);
        break;
      case "delete":
        if (cursor >= 0 && item) {
          event.preventDefault();
          remove(item);
          setCursor((current) => Math.min(current, visible.length - 2));
        }
        break;
      case "pin":
        event.preventDefault();
        if (item) pin(item);
        break;
      case "keep":
        event.preventDefault();
        if (item) keep(item);
        break;
      case "search":
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        break;
      case "guide":
        event.preventDefault();
        loadShortcuts();
        setGuide(true);
        break;
      case "menu":
        event.preventDefault();
        openMenuAtCursor();
        break;
      case "edit": {
        event.preventDefault();
        const target = cursor >= 0 ? visible[cursor] : visible[0];
        if (target && (itemKind(target) === "text" || itemKind(target) === "dictation")) {
          startEdit(target);
        }
        break;
      }
      case "escape":
        event.preventDefault();
        if (query) setQuery("");
        else if (cursor >= 0) setCursor(-1);
        else void api.hidePanel();
        break;
    }
  };

  const restricted = !state.license.canUse;
  const mainChord = shortcuts?.rows.find((row) => row.id === "main")?.display;

  return (
    <div className={`clip${docked ? " is-docked" : ""}`} data-dock={state.dock} onKeyDown={onKeyDown}>
      <ResizeHandles />
      <header
        className="clip-head"
        onMouseDown={(event) => {
          // A floating window moves by its title bar; a docked one stays put
          // until it is set to float from the menu or Settings.
          if (docked || event.button !== 0 || (event.target as HTMLElement).closest("button, input")) return;
          void getCurrentWindow().startDragging();
        }}
      >
        {/* What the dot means, said once: the panel's capture switch, which is
            also in the footer and in Settings. */}
        <span
          className={`status-dot${state.capturePaused ? " paused" : ""}`}
          role="img"
          aria-label={state.capturePaused ? "Not keeping what you copy" : "Keeping what you copy"}
          title={state.capturePaused ? "Not keeping what you copy" : "Keeping what you copy"}
        />
        <h1 className="clip-title">Clipboard</h1>
        {state.capturePaused && <span className="badge paused-badge">Paused</span>}
        <span className="clip-head-spring" />
        <button
          type="button"
          className="head-btn"
          aria-label={state.capturePaused ? "Resume keeping what you copy" : "Pause keeping what you copy"}
          title={state.capturePaused ? "Resume" : "Pause"}
          aria-pressed={state.capturePaused}
          onClick={togglePause}
        >
          {state.capturePaused ? <Play size={15} /> : <Pause size={15} />}
        </button>
        <button type="button" className="head-btn" aria-label="Draw" title="Draw" onClick={() => void api.toggleCompanion("draw")}>
          <PenTool size={15} />
        </button>
        <button type="button" className="head-btn" aria-label="Settings" title="Settings" onClick={() => void api.toggleCompanion("settings")}>
          <SettingsIcon size={15} />
        </button>
        <button type="button" className="head-btn" aria-label="More" title="Dock and shortcuts" aria-haspopup="menu" onClick={openMore}>
          <MoreHorizontal size={16} />
        </button>
        <button type="button" className="head-btn close" aria-label="Close" title="Close" onClick={() => void api.hidePanel()}>
          <X size={16} />
        </button>
      </header>

      <div className="clip-tools">
        <label className="search-field">
          <Search size={15} aria-hidden="true" />
          <input
            ref={searchRef}
            className="search"
            type="search"
            placeholder="Search"
            value={query}
            spellCheck={false}
            aria-label="Search the clipboard history"
            aria-controls="clip-list"
            onChange={(event) => setQuery(event.target.value)}
          />
          {query && (
            <button type="button" className="search-clear" aria-label="Clear search" onClick={() => setQuery("")}>
              <X size={13} />
            </button>
          )}
        </label>
        <div className="filters" role="tablist" aria-label="Kind">
          {TYPE_FILTERS.map((chip) => (
            <button
              key={chip.id}
              type="button"
              role="tab"
              aria-selected={filter === chip.id}
              className={`filter${filter === chip.id ? " on" : ""}`}
              disabled={chip.id !== "all" && view.counts[chip.id] === 0}
              onClick={() => setFilter(chip.id)}
            >
              {chip.label}
            </button>
          ))}
        </div>
      </div>

      <div
        className="clip-scroll"
        ref={scrollRef}
        onScroll={(event) => {
          const el = event.currentTarget;
          setWindow(windowFor(el.scrollTop, el.clientHeight));
        }}
      >
        {visible.length === 0 ? (
          <div className="clip-empty">
            {state.items.length === 0 ? (
              <>
                <div className="empty-title">Copy anything</div>
                <p>Text, links, images and files you copy land here, newest first.</p>
                {mainChord && (
                  <p className="empty-hint">
                    <kbd>{mainChord}</kbd> brings this window back from anywhere.
                  </p>
                )}
              </>
            ) : (
              <>
                <div className="empty-title">Nothing matches</div>
                <p>Try another word or kind.</p>
              </>
            )}
          </div>
        ) : (
          <ul
            id="clip-list"
            ref={listRef}
            className="clip-list"
            role="listbox"
            aria-label="Clipboard history"
            tabIndex={0}
            aria-activedescendant={cursor >= 0 && visible[cursor] ? `item-${visible[cursor].id}` : undefined}
            onFocus={(event) => {
              // Tabbing in starts at the top; a click lands on its own row and
              // must not scroll the list out from under the pointer.
              if (event.target.matches(":focus-visible")) {
                setCursor((current) => (current < 0 && visible.length ? 0 : current));
              }
            }}
          >
            {/* Only the rows on screen are drawn. The two spacers keep the
                scrollbar honest: the list is as tall as the whole history, so
                the thumb moves the way it would in a list of every row. */}
            {window_.start > 0 && (
              <li className="clip-spacer" style={{ height: offsets[window_.start] }} aria-hidden="true" />
            )}
            {visible.slice(window_.start, window_.end).map((item, offset) => {
              const index = window_.start + offset;
              return (
                <FragmentWithHeading
                  key={item.id}
                  heading={
                    index === 0 && hasPins ? "Pinned" : index === firstUnpinned && hasPins ? "Recent" : null
                  }
                >
                  <Row
                    item={item}
                    slot={slotIds.includes(item.id) ? slotIds.indexOf(item.id) + 1 : null}
                    active={index === cursor}
                    current={item.id === activeItemId}
                    position={index}
                    total={visible.length}
                    editing={editingId === item.id}
                    editText={editingId === item.id ? editText : item.content}
                    onCopy={copy}
                    onMenu={openMenu}
                    onPin={pin}
                    onDelete={remove}
                    onEdit={startEdit}
                    onEditChange={setEditText}
                    onEditCommit={commitEdit}
                    onEditCancel={cancelEdit}
                  />
                </FragmentWithHeading>
              );
            })}
            {window_.end < visible.length && (
              <li
                className="clip-spacer"
                style={{ height: Math.max(0, totalHeight - offsets[window_.end]) }}
                aria-hidden="true"
              />
            )}
          </ul>
        )}
      </div>

      <footer className="clip-foot">
        {restricted ? (
          <>
            <span className="foot-warn">{state.license.message ?? "Bard needs a licence to keep new copies."}</span>
            <button type="button" className="btn small primary" onClick={() => void api.openSettings()}>
              Activate
            </button>
          </>
        ) : (
          <>
            <span className="foot-count">
              {state.items.length.toLocaleString()} {state.items.length === 1 ? "item" : "items"}
            </span>
            {mainChord && <span className="foot-hint">{mainChord}</span>}
          </>
        )}
      </footer>

      {menu && <Menu entries={menu.entries} anchor={menu.at ? "point" : "top-right"} at={menu.at} onClose={() => setMenu(null)} />}
      {guide && <ShortcutSheet title="Shortcuts" groups={guideGroups(shortcuts)} onClose={() => setGuide(false)} />}
    </div>
  );
}

function FragmentWithHeading({ heading, children }: { heading: string | null; children: React.ReactNode }) {
  return (
    <>
      {heading && (
        <li className="clip-heading" role="presentation">
          {heading}
        </li>
      )}
      {children}
    </>
  );
}
