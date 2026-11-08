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
import { memo, useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { colorValue, linkParts } from "../content";
import {
  ago,
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
import type { AppState, Dock, Item, ShortcutState } from "../types";
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

/** How many rows render at once; the rest arrive as the list scrolls. */
const PAGE = 80;

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
  onCopy,
  onMenu,
  onPin,
  onDelete,
}: {
  item: Item;
  slot: number | null;
  active: boolean;
  current: boolean;
  onCopy: (item: Item, hide: boolean) => void;
  onMenu: (item: Item, at: { x: number; y: number }) => void;
  onPin: (item: Item) => void;
  onDelete: (item: Item) => void;
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
    body = <span className="item-text">{item.content}</span>;
  }

  return (
    <li
      className={`item${active ? " is-active" : ""}${current ? " is-current" : ""}`}
      data-kind={kind}
      role="option"
      aria-selected={active}
      id={`item-${item.id}`}
      onClick={() => onCopy(item, false)}
      onDoubleClick={() => onCopy(item, true)}
      onContextMenu={(event: MouseEvent) => {
        event.preventDefault();
        onMenu(item, { x: event.clientX, y: event.clientY });
      }}
    >
      <span className="item-glyph" aria-hidden="true">
        <Glyph size={14} strokeWidth={2} />
      </span>
      <span className="item-body">{body}</span>
      {(current || pinned) && (
        <span className="sr-only">
          {current ? ", on the clipboard" : ""}
          {pinned ? ", pinned" : ""}
        </span>
      )}
      <span className="item-end">
        <span className="item-meta">
          {current && (
            <span className="badge current" title="On the clipboard">
              <Check size={11} strokeWidth={3} aria-hidden="true" />
            </span>
          )}
          {item.favorite !== 0 && <Star className="item-flag" size={12} aria-label="Kept" />}
          {pinned && <Pin className="item-flag" size={12} aria-label="Pinned" />}
          {slot !== null ? (
            <kbd className="badge slot" title={`Alt+Shift+${slot}`}>
              {slot}
            </kbd>
          ) : (
            <span className="item-time">{ago(item.createdAt)}</span>
          )}
        </span>
        {/* Pointer shortcuts only: the same actions are Ctrl+P, Delete and the
            context menu from the keyboard, so these stay out of the tab order
            and out of the option's accessible content. */}
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
  onState: (next: AppState) => void;
  activeItemId: number | null;
}) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<TypeFilter>("all");
  const [cursor, setCursor] = useState(-1);
  const [limit, setLimit] = useState(PAGE);
  const [menu, setMenu] = useState<{ entries: MenuEntry[]; at?: { x: number; y: number } } | null>(null);
  const [guide, setGuide] = useState(false);
  const [shortcuts, setShortcuts] = useState<ShortcutState | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const docked = state.dock !== "float";

  const view = useMemo(() => filterShelf(state.items, query, filter), [state.items, query, filter]);
  const visible = view.visible;
  const slotIds = useMemo(() => state.items.slice(0, 3).map((item) => item.id), [state.items]);
  const firstUnpinned = visible.findIndex((item) => item.pinnedAt === null);
  const hasPins = visible.length > 0 && visible[0].pinnedAt !== null;

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
    setLimit(PAGE);
  }, [query, filter]);

  useEffect(() => {
    if (cursor < 0) return;
    document.getElementById(`item-${visible[cursor]?.id}`)?.scrollIntoView({ block: "nearest" });
  }, [cursor, visible]);

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
      void api.setFavorite(item.id, item.favorite === 0).then(refresh);
    },
    [refresh],
  );

  const remove = useCallback(
    (item: Item) => {
      onState({ ...state, items: state.items.filter((existing) => existing.id !== item.id) });
      void api
        .deleteItem(item.id)
        .catch((error) => {
          void api.notifyError(reason(error, "Couldn't delete that item"));
          refresh();
        });
    },
    [onState, refresh, state],
  );

  const openMenu = useCallback(
    (item: Item, at: { x: number; y: number }) => {
      const kind = itemKind(item);
      const entries: MenuEntry[] = [
        { label: "Copy", onClick: () => copy(item, false), trailing: "Enter" },
        { label: item.pinnedAt === null ? "Pin to slots" : "Unpin", onClick: () => pin(item), trailing: "Ctrl+P" },
        {
          label: item.favorite === 0 ? "Keep forever" : "Stop keeping forever",
          onClick: () => keep(item),
          trailing: "Ctrl+S",
        },
      ];
      if (kind === "image" || kind === "draw") {
        entries.push({ label: "Open in Draw", onClick: () => void api.openDrawWithImage(item.id) });
      }
      entries.push({ label: "Delete", onClick: () => remove(item), danger: true, separated: true, trailing: "Del" });
      setMenu({ entries, at });
    },
    [copy, keep, pin, remove],
  );

  const setDock = (dock: Dock) => {
    void api.setDock(dock).catch((error) => void api.notifyError(reason(error, "Couldn't move the window")));
  };

  const openMore = () => {
    const entries: MenuEntry[] = [
      { label: "Float", onClick: () => setDock("float"), checked: state.dock === "float", radio: true },
      { label: "Dock left", onClick: () => setDock("left"), checked: state.dock === "left", radio: true },
      { label: "Dock right", onClick: () => setDock("right"), checked: state.dock === "right", radio: true },
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
    void api.setCapturePaused(!state.capturePaused).then(refresh);
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
          if (event.button !== 0 || (event.target as HTMLElement).closest("button, input")) return;
          // Dragging a docked window undocks it.
          if (docked) void api.setDock("float");
          void getCurrentWindow().startDragging();
        }}
      >
        <span className={`status-dot${state.capturePaused ? " paused" : ""}`} aria-hidden="true" />
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
            aria-activedescendant={cursor >= 0 && visible[cursor] ? `item-${visible[cursor].id}` : undefined}
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
        onScroll={(event) => {
          const el = event.currentTarget;
          if (el.scrollTop + el.clientHeight > el.scrollHeight - 400 && limit < visible.length) {
            setLimit((current) => current + PAGE);
          }
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
            {visible.slice(0, limit).map((item, index) => (
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
                  onCopy={copy}
                  onMenu={openMenu}
                  onPin={pin}
                  onDelete={remove}
                />
              </FragmentWithHeading>
            ))}
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
