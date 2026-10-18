import { Package } from "lucide-react";
import { useEffect, useRef } from "react";
import { TYPE_FILTERS, type TypeFilter } from "../shelf";

/*
  Ctrl+K: the chip and zone switcher, standing where the sections switcher
  did. It lists what the shelf can be pointed at — each type with the number
  of cards it holds, and the Packs zone — and it creates nothing: grouping is
  packs' business now, not sections'.
*/
export default function FilterSwitcher({
  value,
  counts,
  onPick,
  onPacks,
  onClose,
}: {
  value: TypeFilter;
  counts: Record<TypeFilter, number>;
  onPick: (filter: TypeFilter) => void;
  onPacks: () => void;
  onClose: () => void;
}) {
  const cardRef = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    previousFocus.current = document.activeElement as HTMLElement | null;
    // The keyboard lands on the active filter, which is where the eye is.
    cardRef.current?.querySelector<HTMLButtonElement>(`[data-filter="${value}"]`)?.focus();

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      const rows = [...(cardRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
      if (rows.length === 0) return;
      event.preventDefault();
      const current = rows.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        event.key === "ArrowDown" ? (current + 1) % rows.length : (current - 1 + rows.length) % rows.length;
      rows[next]?.focus();
    };

    // Capture, so the shelf's own Escape order never sees the key first.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      // A switcher that closes onto <body> loses the keyboard entirely.
      if (previousFocus.current?.isConnected) previousFocus.current.focus();
    };
  }, [onClose, value]);

  return (
    <div className="sheet" onClick={onClose}>
      <div
        ref={cardRef}
        className="sheet-card compact"
        role="dialog"
        aria-modal="true"
        aria-label="Filters and zones"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="switcher-list">
          {TYPE_FILTERS.map((filter) => (
            <button
              key={filter.id}
              type="button"
              data-filter={filter.id}
              className={`switcher-row${value === filter.id ? " on" : ""}`}
              aria-pressed={value === filter.id}
              onClick={() => onPick(filter.id)}
            >
              <span>{filter.label}</span>
              <span className="switcher-count">{counts[filter.id]}</span>
            </button>
          ))}
        </div>
        <div className="menu-sep" />
        <button type="button" className="switcher-row" onClick={onPacks}>
          <Package size={13} strokeWidth={1.8} aria-hidden="true" />
          <span>Packs</span>
        </button>
      </div>
    </div>
  );
}
