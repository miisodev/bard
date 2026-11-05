import { Clipboard, Search } from "lucide-react";
import { TYPE_FILTERS, type TypeFilter } from "../shelf";

export default function ShelfEmptyState({ query, filter, favoritesOnly, empty, onReset }: {
  query: string;
  filter: TypeFilter;
  favoritesOnly: boolean;
  empty: boolean;
  onReset: () => void;
}) {
  const filtered = !!query.trim() || filter !== "all" || favoritesOnly;
  const type = TYPE_FILTERS.find((entry) => entry.id === filter)?.label.toLowerCase() ?? "items";
  const title = query.trim() ? "No matches"
    : favoritesOnly ? "No favorites here"
    : filtered ? `No ${type} here`
    : empty ? "Your shelf starts here" : "All caught up";
  const hint = query.trim() ? `Nothing matches “${query.trim()}”. Try another word or clear your filters.`
    : favoritesOnly ? "Star an item to keep it within reach."
    : filtered ? "Try another type, or see everything on your shelf."
    : "Copy something to keep it here, or add a note below.";
  const Glyph = filtered ? Search : Clipboard;
  return (
    <div className="empty-state">
      <Glyph className="empty-glyph" aria-hidden="true" strokeWidth={1.5} />
      <div className="title">{title}</div>
      <div className="hint">{hint}</div>
      {filtered ? <button type="button" className="btn" onClick={onReset}>Clear filters</button> : null}
    </div>
  );
}
