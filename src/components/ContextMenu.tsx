import {
  Captions,
  Copy,
  FileText,
  Pencil,
  Pin,
  PinOff,
  Star,
  Trash2,
} from "lucide-react";
import { useEffect, useRef } from "react";
import type { Item } from "../types";

export type MenuAction = "copy" | "copyList" | "favorite" | "pin" | "edit" | "caption" | "delete";

interface ContextMenuProps {
  x: number;
  y: number;
  multi: boolean;
  item?: Item;
  onAction: (a: MenuAction) => void;
  onClose: () => void;
}

type RowAction = {
  label: string;
  action: MenuAction;
  icon: React.ReactNode;
  disabled?: boolean;
  danger?: boolean;
  hint?: string;
};

export default function ContextMenu({
  x,
  y,
  multi,
  item,
  onAction,
  onClose,
}: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const firstEnabled = useRef<HTMLButtonElement | null>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const typeahead = useRef({ value: "", at: 0 });

  useEffect(() => {
    previousFocus.current = document.activeElement as HTMLElement | null;
    firstEnabled.current = ref.current?.querySelector<HTMLButtonElement>("button:not(:disabled)") ?? null;
    firstEnabled.current?.focus();

    const onDown = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
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

  const rows: RowAction[] = item
    ? [
        {
          label: multi ? "Copy selection" : "Copy",
          action: "copy",
          icon: <Copy size={14} strokeWidth={1.8} />,
        },
        {
          label: "Copy as list",
          action: "copyList",
          icon: <FileText size={14} strokeWidth={1.8} />,
          disabled: !multi,
          hint: multi ? "Copies selected items as a Markdown list" : "Select multiple items",
        },
        { label: multi ? "Toggle favorite" : item.favorite ? "Remove favorite" : "Add favorite", action: "favorite", icon: <Star size={14} strokeWidth={1.8} /> },
        {
          label: multi ? "Toggle pin" : item.pinnedAt !== null ? "Unpin" : "Pin to top",
          action: "pin",
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
          action: "edit",
          icon: <Pencil size={14} strokeWidth={1.8} />,
          disabled: item.kind !== "text" || multi,
          hint: multi ? "Select one text item" : item.kind === "text" ? "Edit this text" : "Text items only",
        },
        {
          label: "Add caption",
          action: "caption",
          icon: <Captions size={14} strokeWidth={1.8} />,
          disabled: item.kind !== "image" || multi,
          hint: multi ? "Select one image" : item.kind === "image" ? "Turn this image into a note" : "Images only",
        },
        { label: multi ? "Delete selection" : "Delete", action: "delete", icon: <Trash2 size={14} strokeWidth={1.8} />, danger: true, hint: "Removes the selected item(s)" },
      ]
    : [];

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
      {rows.map((row, index) => (
        <div key={row.action}>
          {index === 2 || index === 5 ? <div className="ctx-sep" role="separator" /> : null}
          <button
            type="button"
            role="menuitem"
            className={`ctx-row${row.danger ? " danger" : ""}`}
            disabled={row.disabled}
            title={row.hint}
            onClick={() => {
              if (row.disabled) return;
              onAction(row.action);
            }}
          >
            {row.icon}
            <span>{row.label}</span>
          </button>
        </div>
      ))}
    </div>
  );
}
