import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { Copy } from "lucide-react";
import { colorValue, linkParts } from "../content";
import { renderInline } from "../markdown";
import { api } from "../store";
import type { Attachment, Item } from "../types";

/*
  A card is a checkbox, its text, and a quiet Copy that surfaces only on the
  card you are pointing at or working on.

  Copper puts no timestamp, no action row and no branding on a card: everything
  else a card can do is reachable from the keyboard or the context menu, which
  keeps the list scannable at a glance and stops the chrome competing with the
  content. Resist adding affordances here — the empty space is the design.

  The row is one tab stop in the timeline, not one per card: the listbox owns
  the cursor, `tabIndex` roves, and the buttons inside stay in the tab order so
  a card's own commands are one Tab away. The row therefore handles no key
  itself — every command on it is the panel's, and duplicating them here is
  what used to make Space both select and complete.
*/

/** Long enough to identify a card, short enough to be read out loud. */
const PREVIEW_CHARS = 48;

/**
 * What a screen reader should call this card, and what the Copy button should
 * say it is copying. Nine cards all reading "Copy text" is nine identical
 * controls; the preview is what the button actually acts on, so it belongs in
 * the name. A captured image carries no text at all — it is stored with an
 * empty body and a path — so it is named for what it is until a caption gives
 * it something to be called.
 */
function preview(item: Item): string {
  const text = item.content.replace(/\s+/g, " ").trim();
  if (!text) return item.kind === "image" ? "image" : "empty note";
  return text.length > PREVIEW_CHARS ? `${text.slice(0, PREVIEW_CHARS)}…` : text;
}

export default function ItemCard({
  item,
  attachments,
  selected,
  active = false,
  fresh = false,
  editing,
  search,
  onClick,
  onFocus,
  onContextMenu,
  onOpenContextMenu,
  onToggleDone,
  onCopyImage,
  onDelete,
  onEdit,
  onEditDone,
}: {
  item: Item;
  attachments: Attachment[];
  selected: boolean;
  /** The roving tab stop and the keyboard cursor. Exactly one card at a time. */
  active?: boolean;
  /** A capture that has not been on screen before; it alone animates in. */
  fresh?: boolean;
  editing: boolean;
  search: string;
  onClick: (e: ReactMouseEvent) => void;
  /** The row — or anything inside it — took DOM focus. */
  onFocus: () => void;
  onContextMenu: (e: ReactMouseEvent) => void;
  onOpenContextMenu: (e: ReactKeyboardEvent<HTMLDivElement>) => void;
  onToggleDone: () => void;
  onCopyImage: () => void;
  onDelete: () => void;
  onEdit: () => void;
  onEditDone: () => void;
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [imageMissing, setImageMissing] = useState(false);
  const [draft, setDraft] = useState(item.content);
  const [thumbUrls, setThumbUrls] = useState<Map<number, string>>(new Map());
  const [clamped, setClamped] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  const done = item.done !== 0;
  const color = item.kind === "text" ? colorValue(item.content) : null;
  const link = item.kind === "text" ? linkParts(item.content) : null;

  useEffect(() => {
    if (item.kind !== "image") return;
    void api.itemImageData(item.id).then((url) => {
      setImageUrl(url);
      setImageMissing(!url);
    });
  }, [item.id, item.kind]);

  useEffect(() => {
    // Rebuilt, not accumulated. Detach an attachment and its file is unlinked, but
    // the base64 data URL for the dead id would otherwise stay in state for the
    // life of the card — real memory held for a thumbnail of a file that no longer
    // exists, across every card in the timeline.
    let alive = true;
    setThumbUrls(new Map());
    for (const a of attachments) {
      void api.attachmentImageData(a.id).then((url) => {
        if (!alive || !url) return;
        setThumbUrls((m) => new Map(m).set(a.id, url));
      });
    }
    return () => {
      alive = false;
    };
  }, [attachments.map((a) => a.id).join(",")]);

  useEffect(() => {
    setDraft(item.content);
  }, [item.content]);

  // The six-line clamp hides the rest of a note with nothing to say so. CSS
  // cannot tell a clamped box from a short one, so the fade is measured: once
  // per content change, and again only when the box itself changes size.
  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const measure = () => setClamped(body.scrollHeight > body.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(body);
    return () => observer.disconnect();
  }, [item.content, search]);

  const commitEdit = () => {
    const v = draft.trim();
    if (v !== item.content) {
      if (v) void api.updateItem(item.id, v);
      else void onDelete();
    }
    onEditDone();
  };

  return (
    <div
      className={`item-card${selected ? " selected" : ""}${active ? " active" : ""}${done ? " done" : ""}${
        fresh ? " fresh" : ""
      }`}
      role="option"
      // Roving: one card is in the tab order, the rest are reached by arrow keys.
      tabIndex={active ? 0 : -1}
      aria-selected={selected}
      aria-label={preview(item)}
      data-item-id={item.id}
      data-item-kind={item.kind}
      onClick={onClick}
      onFocus={onFocus}
      onContextMenu={onContextMenu}
      onKeyDown={(event) => {
        // Only the context-menu key belongs to the row. Escape, arrows, Space,
        // Enter and Delete are the panel's, and handling them here as well
        // meant each of them ran twice against two different items.
        if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
          event.preventDefault();
          event.stopPropagation();
          onOpenContextMenu(event);
        }
      }}
    >
      <button
        type="button"
        className="check"
        role="checkbox"
        aria-checked={done}
        aria-label={done ? "Mark as not done" : "Mark as done"}
        onClick={(e) => {
          e.stopPropagation();
          onToggleDone();
        }}
      >
        {done ? (
          <svg viewBox="0 0 18 18" aria-hidden="true">
            <circle cx="9" cy="9" r="9" />
            <path d="M5 9.2l2.6 2.6L13 6.6" fill="none" strokeWidth="2"
                  strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : (
          <svg viewBox="0 0 18 18" aria-hidden="true">
            <circle cx="9" cy="9" r="8.25" fill="none" strokeWidth="1.5" />
          </svg>
        )}
      </button>

      <div className="item-main">
        {item.kind === "image" && !imageUrl ? (
          // An image row must never render as an empty card: while it loads it
          // holds its place, and a file that is gone says so.
          <div className={imageMissing ? "item-missing" : "item-image-pending"}>
            {imageMissing ? "Image unavailable" : null}
          </div>
        ) : item.kind === "image" && imageUrl ? (
          // Clicking the image copies it back to the clipboard, as Copper does.
          <img src={imageUrl} alt="" className="item-image" onClick={(e) => {
            e.stopPropagation();
            onCopyImage();
          }} />
        ) : editing && item.kind === "text" ? (
          <textarea
            className="inline-edit"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                commitEdit();
              }
              if (e.key === "Escape") onEditDone();
            }}
            onBlur={commitEdit}
          />
        ) : (
          <div
            ref={bodyRef}
            className={`item-body${clamped ? " clamped" : ""}${color ? " is-color" : ""}`}
            // Only where text is actually hidden: a tooltip that repeats what
            // the card already shows is noise on every row in the list.
            title={clamped ? item.content : undefined}
            onDoubleClick={item.kind === "text" ? onEdit : undefined}
          >
            {color ? <span className="item-swatch" style={{ background: color }} aria-hidden="true" /> : null}
            {link && !search ? (
              <>
                <span className="item-link-host">{link.host}</span>
                <span className="item-link-rest">{link.rest}</span>
              </>
            ) : (
              renderInline(item.content, search)
            )}
          </div>
        )}

        {attachments.length > 0 ? (
          <div className="thumb-row">
            {attachments.map((a, index) => {
              const url = thumbUrls.get(a.id);
              return (
                <span key={a.id} className="thumb">
                  {/* Copying an attachment is only reachable by clicking it, so
                      it is a real control: named, focusable, and it stops the
                      click before the card turns it into a selection. */}
                  {url ? (
                    <button
                      type="button"
                      className="thumb-img"
                      aria-label={`Copy attachment ${index + 1}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        void api.copyAttachment(a.id);
                      }}
                    >
                      <img src={url} alt="" />
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="thumb-x"
                    aria-label={
                      attachments.length > 1
                        ? `Remove attachment ${index + 1} of ${attachments.length}`
                        : "Remove attachment"
                    }
                    onClick={(e) => {
                      e.stopPropagation();
                      void api.detachAttachment(a.id);
                    }}
                  >
                    ×
                  </button>
                </span>
              );
            })}
          </div>
        ) : null}
      </div>

      {/* In the card's own column, so text never runs underneath it. */}
      <button
        type="button"
        className="item-copy"
        aria-label={`Copy ${preview(item)}`}
        title="Copy"
        onClick={(e) => {
          e.stopPropagation();
          onCopyImage();
        }}
      >
        {/* Lucide, like every other icon in the product. This was hand-drawn at a
            1.4 stroke beside 1.8 Lucide in the same 380px panel, and at 15px the
            two rounded rectangles read muddy next to the header's crisp glyphs. */}
        <Copy size={17} aria-hidden="true" />
      </button>
    </div>
  );
}
