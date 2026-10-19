import {
  Captions,
  ChevronLeft,
  ChevronRight,
  Copy,
  FileText,
  Package,
  PackagePlus,
  Pencil,
  PenLine,
  Pin,
  PinOff,
  Star,
  Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Item, PackSummary } from "../types";

export type MenuAction =
  | "copy"
  | "copyList"
  | "favorite"
  | "pin"
  | "edit"
  | "caption"
  | "annotate"
  | "delete";

interface ContextMenuProps {
  x: number;
  y: number;
  multi: boolean;
  item?: Item;
  /** Every pack the selection can be added to, in the order Rust sent them. */
  packs: PackSummary[];
  onAction: (a: MenuAction) => void;
  /** Make a pack from the selection; Rust answers an empty name itself. */
  onCreatePack: (name: string) => void;
  onAddToPack: (id: number) => void;
  onClose: () => void;
}

type RowAction = {
  label: string;
  onClick: () => void;
  icon: React.ReactNode;
  disabled?: boolean;
  danger?: boolean;
  hint?: string;
  /** The row's handle, so focus can return to it after a step. */
  row?: string;
  trailing?: React.ReactNode;
  separated?: boolean;
};

/**
 * The menu's pages. The pack steps live in the same box rather than a flyout:
 * the shelf is a band, a nested menu would open off its edge or into a
 * scroller, and stepping forward and back keeps every choice one key away.
 */
type View = "actions" | "add" | "name";

export default function ContextMenu({
  x,
  y,
  multi,
  item,
  packs,
  onAction,
  onCreatePack,
  onAddToPack,
  onClose,
}: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const typeahead = useRef({ value: "", at: 0 });
  const [view, setView] = useState<View>("actions");
  const [draft, setDraft] = useState("New pack");
  const nameRef = useRef<HTMLInputElement>(null);
  const viewRef = useRef<View>("actions");
  /** Where a step returns to, and which row should greet it. */
  const backTo = useRef<View>("actions");
  const returnTo = useRef("new-pack");
  const firstView = useRef(true);

  useEffect(() => {
    viewRef.current = view;
  });

  useEffect(() => {
    previousFocus.current = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();

    const onDown = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      const typing = (event.target as HTMLElement | null)?.tagName === "INPUT";
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        // One step back at a time: the flow steps are not the menu's exit.
        const current = viewRef.current;
        if (current === "add") {
          backTo.current = "actions";
          returnTo.current = "add";
          setView("actions");
        } else if (current === "name") {
          returnTo.current = backTo.current === "add" ? "add-new" : "new-pack";
          setView(backTo.current);
        } else {
          onClose();
        }
        return;
      }
      // The name field owns every other key, including the arrows it needs.
      if (typing) return;
      const rows = [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
        const now = Date.now();
        const state = typeahead.current;
        state.value = now - state.at > 700 ? event.key : state.value + event.key;
        state.at = now;
        const match = rows.find((row) => row.textContent?.trim().toLowerCase().startsWith(state.value.toLowerCase()));
        if (match) {
          event.preventDefault();
          match.focus();
        }
        return;
      }
      // Right steps into the pack list, Left steps back out of it.
      if (event.key === "ArrowRight" && (document.activeElement as HTMLElement)?.dataset.ctxRow === "add") {
        event.preventDefault();
        backTo.current = "actions";
        setView("add");
        return;
      }
      if (event.key === "ArrowLeft" && viewRef.current === "add") {
        event.preventDefault();
        returnTo.current = "add";
        setView("actions");
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Home" && event.key !== "End") {
        return;
      }
      if (rows.length === 0) return;
      event.preventDefault();
      const current = rows.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? rows.length - 1
            : event.key === "ArrowDown"
              ? (current + 1 + rows.length) % rows.length
              : (current - 1 + rows.length) % rows.length;
      rows[next]?.focus();
    };

    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey, true);
      if (previousFocus.current?.isConnected) previousFocus.current.focus();
    };
  }, [onClose]);

  // A step change moves focus with it: the name field when it opens, the row
  // it came from when a step is left.
  useEffect(() => {
    if (firstView.current) {
      firstView.current = false;
      return;
    }
    if (view === "name") {
      nameRef.current?.focus();
      nameRef.current?.select();
      return;
    }
    const wanted = view === "add" ? "add-back" : returnTo.current;
    ref.current?.querySelector<HTMLButtonElement>(`[data-ctx-row="${wanted}"]`)?.focus();
  }, [view]);

  const stepTo = (next: View, from: View, to?: string) => {
    backTo.current = from;
    if (to) returnTo.current = to;
    setView(next);
  };

  const createPack = () => {
    onCreatePack(draft);
    onClose();
  };

  const actions: RowAction[] = item
    ? [
        {
          label: multi ? "Copy selection" : "Copy",
          onClick: () => onAction("copy"),
          icon: <Copy size={14} strokeWidth={1.8} />,
        },
        {
          label: "Copy as list",
          onClick: () => onAction("copyList"),
          icon: <FileText size={14} strokeWidth={1.8} />,
          disabled: !multi,
          hint: multi ? "Copies selected items as a Markdown list" : "Select multiple items",
        },
        {
          label: multi ? "Toggle favorite" : item.favorite ? "Remove favorite" : "Add favorite",
          onClick: () => onAction("favorite"),
          icon: <Star size={14} strokeWidth={1.8} />,
          separated: true,
        },
        {
          label: multi ? "Toggle pin" : item.pinnedAt !== null ? "Unpin" : "Pin to top",
          onClick: () => onAction("pin"),
          icon:
            !multi && item.pinnedAt !== null ? (
              <PinOff size={14} strokeWidth={1.8} />
            ) : (
              <Pin size={14} strokeWidth={1.8} />
            ),
          hint: "Up to three pins; Right Shift twice, then 1, 2 or 3 pastes them",
        },
        {
          label: "Edit",
          onClick: () => onAction("edit"),
          icon: <Pencil size={14} strokeWidth={1.8} />,
          disabled: item.kind !== "text" || multi,
          hint: multi ? "Select one text item" : item.kind === "text" ? "Edit this text" : "Text items only",
        },
        {
          label: "Add caption",
          onClick: () => onAction("caption"),
          icon: <Captions size={14} strokeWidth={1.8} />,
          disabled: item.kind !== "image" || multi,
          hint: multi ? "Select one image" : item.kind === "image" ? "Turn this image into a note" : "Images only",
          separated: true,
        },
        {
          label: "Annotate",
          onClick: () => onAction("annotate"),
          icon: <PenLine size={14} strokeWidth={1.8} />,
          disabled: item.kind !== "image" || multi,
          hint: multi ? "Select one image" : item.kind === "image" ? "Draw on this image" : "Images only",
        },
        {
          label: multi ? "Delete selection" : "Delete",
          onClick: () => onAction("delete"),
          icon: <Trash2 size={14} strokeWidth={1.8} />,
          danger: true,
          hint: "Removes the selected item(s)",
        },
        {
          label: "New pack from selection",
          onClick: () => stepTo("name", "actions"),
          icon: <PackagePlus size={14} strokeWidth={1.8} />,
          row: "new-pack",
          separated: true,
          hint: "Groups the selected items; pasting the pack pastes their text together",
        },
        {
          label: "Add to pack",
          onClick: () => stepTo("add", "actions"),
          icon: <Package size={14} strokeWidth={1.8} />,
          row: "add",
          trailing: <ChevronRight size={13} strokeWidth={1.8} aria-hidden="true" />,
          hint: "Appends the selected items to an existing pack",
        },
      ]
    : [];

  const addRows: RowAction[] = [
    {
      label: "Back",
      onClick: () => stepTo("actions", "add", "add"),
      icon: <ChevronLeft size={14} strokeWidth={1.8} />,
      row: "add-back",
    },
    ...packs.map((pack) => ({
      label: pack.name,
      onClick: () => {
        onAddToPack(pack.id);
        onClose();
      },
      icon: <Package size={14} strokeWidth={1.8} />,
      row: `pack-${pack.id}`,
      trailing: String(pack.itemCount),
      separated: pack.id === packs[0]?.id,
    })),
    {
      label: "New pack…",
      onClick: () => stepTo("name", "add", "add-new"),
      icon: <PackagePlus size={14} strokeWidth={1.8} />,
      row: "add-new",
      separated: true,
    },
  ];

  const rows = view === "actions" ? actions : view === "add" ? addRows : [];
  const left = Math.max(8, Math.min(x, window.innerWidth - 220));
  const top = Math.max(8, Math.min(y, window.innerHeight - (multi ? 300 : 320)));

  return (
    <div
      ref={ref}
      className="ctx-menu"
      style={{ left, top }}
      role="menu"
      aria-label="Item actions"
      onContextMenu={(event) => event.preventDefault()}
    >
      {view === "name" ? (
        <div className="ctx-form">
          <span className="ctx-form-title">
            {backTo.current === "add" ? "New pack" : "New pack from selection"}
          </span>
          <input
            ref={nameRef}
            className="ctx-input"
            value={draft}
            aria-label="Pack name"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return;
              event.preventDefault();
              event.stopPropagation();
              createPack();
            }}
          />
          <button
            type="button"
            role="menuitem"
            className="ctx-row"
            data-ctx-row="create"
            onClick={createPack}
          >
            <PackagePlus size={14} strokeWidth={1.8} />
            <span>Create pack</span>
          </button>
          <button
            type="button"
            role="menuitem"
            className="ctx-row"
            data-ctx-row="name-back"
            onClick={() => {
              returnTo.current = backTo.current === "add" ? "add-new" : "new-pack";
              setView(backTo.current);
            }}
          >
            <ChevronLeft size={14} strokeWidth={1.8} />
            <span>Back</span>
          </button>
        </div>
      ) : (
        rows.map((row, index) => (
          <div key={`${row.row ?? row.label}-${index}`}>
            {row.separated && index > 0 ? <div className="ctx-sep" role="separator" /> : null}
            <button
              type="button"
              role="menuitem"
              className={`ctx-row${row.danger ? " danger" : ""}`}
              data-ctx-row={row.row}
              disabled={row.disabled}
              title={row.hint}
              onClick={row.onClick}
            >
              {row.icon}
              <span className="ctx-label">{row.label}</span>
              {row.trailing}
            </button>
          </div>
        ))
      )}
    </div>
  );
}
