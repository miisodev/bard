import { useEffect, useState, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent } from "react";
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
*/
export default function ItemCard({
  item,
  attachments,
  selected,
  active = false,
  editing,
  search,
  onClick,
  onActivate,
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
  active?: boolean;
  editing: boolean;
  search: string;
  onClick: (e: ReactMouseEvent) => void;
  onActivate: () => void;
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

  const done = item.done !== 0;

  useEffect(() => {
    if (item.kind !== "image") return;
    void api.itemImageData(item.id).then((url) => {
      setImageUrl(url);
      setImageMissing(!url);
    });
  }, [item.id, item.kind]);

  useEffect(() => {
    for (const a of attachments) {
      void api.attachmentImageData(a.id).then((url) => {
        if (url) setThumbUrls((m) => new Map(m).set(a.id, url));
      });
    }
  }, [attachments.map((a) => a.id).join(",")]);

  useEffect(() => {
    setDraft(item.content);
  }, [item.content]);

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
      className={`item-card${selected ? " selected" : ""}${active ? " active" : ""}${done ? " done" : ""}`}
      role="option"
      tabIndex={0}
      aria-selected={selected}
      aria-label={item.kind === "image" ? "Captured image" : item.content.slice(0, 160)}
      data-item-id={item.id}
      data-item-kind={item.kind}
      aria-current={active ? "true" : undefined}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onActivate();
          return;
        }
        if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
          event.preventDefault();
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
          <div className="item-body" onDoubleClick={item.kind === "text" ? onEdit : undefined}>
            {renderInline(item.content, search)}
          </div>
        )}

        {attachments.length > 0 ? (
          <div className="thumb-row">
            {attachments.map((a) => {
              const url = thumbUrls.get(a.id);
              return (
                <span key={a.id} className="thumb">
                  {url ? <img src={url} alt="Attachment" onClick={(e) => {
                    e.stopPropagation();
                    void api.copyAttachment(a.id);
                  }} /> : null}
                  <button
                    type="button"
                    className="thumb-x"
                    aria-label="Remove attachment"
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
        aria-label={`Copy ${item.kind === "image" ? "image" : "text"}`}
        title="Copy"
        onClick={(e) => {
          e.stopPropagation();
          onCopyImage();
        }}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <rect x="5.5" y="5.5" width="8" height="8" rx="2" />
          <path d="M10.5 3.5v-.25A1.75 1.75 0 0 0 8.75 1.5h-5.5A1.75 1.75 0 0 0 1.5 3.25v5.5A1.75 1.75 0 0 0 3.25 10.5h.25" />
        </svg>
      </button>
    </div>
  );
}
