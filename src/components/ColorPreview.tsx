import { Copy, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { colorName } from "../color/names";
import { colorChannels, toHex, toHslText, toRgbText } from "../shelf";
import { api } from "../store";
import type { Item } from "../types";

/*
  A colour card's preview, on the same terms as the shortcut sheet: a scrim,
  focus that starts inside and stays inside, Escape to leave, and focus
  returned to where it came from. Each notation is one press from the
  clipboard, so a colour can move to a design tool in the form it wants.
*/
export default function ColorPreview({
  item,
  onClose,
  notify,
}: {
  item: Item;
  onClose: () => void;
  notify: (text: string, tone?: "info" | "error") => void;
}) {
  const cardRef = useRef<HTMLDivElement>(null);
  const restoreFocusTo = useRef<HTMLElement | null>(null);

  const value = item.content.trim();
  const rgb = colorChannels(value);
  const hex = rgb ? toHex(rgb) : null;
  const rows: { label: string; text: string }[] = rgb
    ? [
        { label: "HEX", text: (hex as string).toUpperCase() },
        { label: "RGB", text: toRgbText(rgb) },
        { label: "HSL", text: toHslText(rgb) },
      ]
    : [{ label: "Value", text: value }];

  const copy = (text: string) => {
    void api
      .copyText(text)
      .then(() => notify(`Copied ${text}`))
      .catch((reason: unknown) => {
        const detail = reason instanceof Error ? reason.message : String(reason ?? "");
        notify(detail || "Couldn't copy.", "error");
      });
  };

  useEffect(() => {
    restoreFocusTo.current = document.activeElement as HTMLElement | null;
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

    // Capture: the shelf's own Escape order must not retract the window out
    // from under a dialog.
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
        className="sheet-card color-preview"
        role="dialog"
        aria-modal="true"
        aria-labelledby="color-preview-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="color-preview-head">
          <h2 id="color-preview-title">{rgb ? colorName(hex as string) : "Color"}</h2>
          <button type="button" className="icon-btn sheet-head-close" aria-label="Close" onClick={onClose}>
            <X aria-hidden="true" />
          </button>
        </div>
        <div className="color-preview-swatch" style={{ background: value }} aria-hidden="true" />
        {rows.map((row) => (
          <div key={row.label} className="color-preview-row">
            <span className="color-preview-label">{row.label}</span>
            <code className="color-preview-value">{row.text}</code>
            <button
              type="button"
              className="icon-btn color-preview-copy"
              aria-label={`Copy ${row.label} ${row.text}`}
              title={`Copy ${row.text}`}
              onClick={() => copy(row.text)}
            >
              <Copy size={14} strokeWidth={1.8} aria-hidden="true" />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
