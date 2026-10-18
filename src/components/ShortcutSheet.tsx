import { useEffect, useRef } from "react";
import { X } from "lucide-react";

/** One block of the sheet: a named group of key rows, with an optional caveat. */
export interface ShortcutGroup {
  name: string;
  rows: { keys: string[]; what: string }[];
  /** A sentence the key rows cannot carry — an honesty note, a limitation. */
  note?: string;
}

/**
 * The shortcut sheet, shared by the panel and Draw so both surfaces state the
 * same keys in the same voice. It is a modal: a scrim the content cannot be
 * clicked through, focus that starts inside and cannot Tab out, Escape to
 * leave, and the focus returned to wherever it came from.
 */
export default function ShortcutSheet({
  title,
  groups,
  onClose,
}: {
  title: string;
  groups: ShortcutGroup[];
  onClose: () => void;
}) {
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

    // Capture: the window's own handler would otherwise act on the key before
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
          <h2 id="shortcut-sheet-title">{title}</h2>
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
          {groups.map((group) => (
            <div key={group.name} className="shortcut-group" role="group" aria-label={group.name}>
              <h3 className="shortcut-group-name">{group.name}</h3>
              {group.rows.map((row, index) => (
                <div key={`${group.name}-${index}`} className="shortcut-row">
                  <span className="shortcut-keys">
                    {row.keys.map((key, index) => (
                      <kbd key={`${key}-${index}`}>{key}</kbd>
                    ))}
                  </span>
                  <span className="shortcut-what">{row.what}</span>
                </div>
              ))}
              {group.note ? <p className="shortcut-note">{group.note}</p> : null}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
