import { useRef } from "react";
import { TYPE_FILTERS, type TypeFilter } from "../shelf";

/*
  The All zone's type chips. They are filters, not groups — packs own grouping
  now — and they behave as a tablist: one Tab stop reaches the row and the
  arrows move and select together, so every choice is one keypress from any
  other.
*/
export default function TypeChips({
  value,
  onChange,
}: {
  value: TypeFilter;
  onChange: (next: TypeFilter) => void;
}) {
  const list = useRef<HTMLDivElement>(null);

  const select = (index: number) => {
    const wrapped = (index + TYPE_FILTERS.length) % TYPE_FILTERS.length;
    onChange(TYPE_FILTERS[wrapped].id);
    list.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[wrapped]?.focus();
  };

  return (
    <div
      ref={list}
      className="chips"
      role="tablist"
      aria-label="Type"
      onKeyDown={(event) => {
        const at = TYPE_FILTERS.findIndex((filter) => filter.id === value);
        if (event.key === "ArrowRight" || event.key === "ArrowDown") {
          event.preventDefault();
          select(at + 1);
        } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
          event.preventDefault();
          select(at - 1);
        } else if (event.key === "Home") {
          event.preventDefault();
          select(0);
        } else if (event.key === "End") {
          event.preventDefault();
          select(TYPE_FILTERS.length - 1);
        }
      }}
    >
      {TYPE_FILTERS.map((filter) => (
        <button
          key={filter.id}
          type="button"
          role="tab"
          id={`shelf-chip-${filter.id}`}
          aria-selected={value === filter.id}
          aria-controls="shelf-cards"
          tabIndex={value === filter.id ? 0 : -1}
          className={`chip${value === filter.id ? " on" : ""}`}
          onClick={() => onChange(filter.id)}
        >
          {filter.label}
        </button>
      ))}
    </div>
  );
}
