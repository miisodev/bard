import { useEffect, useRef, type ReactNode } from "react";

export interface MenuEntry {
  label: string;
  onClick: () => void;
  /** Right-aligned hint: a tick, a count, a shortcut. */
  trailing?: ReactNode;
  disabled?: boolean;
  /** Draw a divider above this entry. */
  separated?: boolean;
  checked?: boolean;
  /** One of a set of mutually exclusive choices, such as a sort order. */
  radio?: boolean;
  /** The entry removes something; it reads in the danger tone, unfilled. */
  danger?: boolean;
}

/*
  One small popover menu, used for both the header's overflow and the
  composer's attach menu. It positions either against the panel's own corner
  (`anchor: "top-right"`) or at a point in client coordinates, flipping upward
  when asked to open from the bottom edge.

  Escape and any outside click close it; the scrim is what catches the click,
  so nothing underneath needs to know the menu exists.
*/
export default function Menu({
  entries,
  onClose,
  anchor = "top-right",
  at,
  label = "Bard actions",
}: {
  entries: MenuEntry[];
  onClose: () => void;
  anchor?: "top-right" | "point";
  at?: { x: number; y: number };
  label?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    // Focus comes back to the control that opened the menu when it closes —
    // without this, dismissing a menu put the keyboard on `<body>` and the
    // next Tab restarted from the search field.
    previousFocus.current = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLButtonElement>("button:not(:disabled)");
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
      const rows = [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      if (!rows.length) return;
      e.preventDefault();
      const current = rows.indexOf(document.activeElement as HTMLButtonElement);
      const next = e.key === "Home" ? 0 : e.key === "End" ? rows.length - 1 : e.key === "ArrowDown" ? (current + 1) % rows.length : (current - 1 + rows.length) % rows.length;
      rows[next]?.focus();
    };
    // Capture phase: the panel's own Escape handling would otherwise hide the
    // whole window before the menu ever saw the key.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (previousFocus.current?.isConnected) previousFocus.current.focus();
    };
  }, [onClose]);

  // A point-anchored menu opens whichever way has room: below the point when
  // it is in the upper half of the window, above it otherwise. That is what
  // makes the composer's attach menu work on any of the four edges.
  const style =
    anchor === "point" && at
      ? at.y < window.innerHeight / 2
        ? { left: at.x, top: at.y + 4 }
        : { left: at.x, bottom: `calc(100% - ${at.y}px)` }
      : undefined;

  return (
    <>
      <div className="menu-scrim" onClick={onClose} onContextMenu={onClose} />
      <div
        ref={ref}
        className={`popover-menu${anchor === "point" ? " at-point" : ""}`}
        role="menu"
        aria-label={label}
        style={style}
      >
        {entries.map((e, i) => (
          <div key={e.label} className="menu-item-wrap">
            {e.separated && i > 0 ? <div className="menu-sep" /> : null}
            <button
              type="button"
              role={
                e.checked === undefined ? "menuitem" : e.radio ? "menuitemradio" : "menuitemcheckbox"
              }
              aria-checked={e.checked}
              className={e.danger ? "danger" : undefined}
              disabled={e.disabled}
              onClick={() => {
                if (e.disabled) return;
                e.onClick();
                onClose();
              }}
            >
              {e.label}
              {e.trailing !== undefined ? <span className="trailing">{e.trailing}</span> : null}
            </button>
          </div>
        ))}
      </div>
    </>
  );
}
