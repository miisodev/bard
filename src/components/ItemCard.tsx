import { useEffect, useState, type MouseEvent as ReactMouseEvent } from "react";
import { renderInline } from "../markdown";
import { api } from "../store";
import type { Attachment, Item } from "../types";

/*
  A card is a checkbox and its text. That is the whole of it.

  Copper puts no timestamp, no action row and no branding on a card: everything
  a card can do is reachable from the keyboard or the context menu, which keeps
  the list scannable at a glance and stops the chrome competing with the content.
  Resist adding affordances here — the empty space is the design.
*/
export default function ItemCard({
  item,
  attachments,
  selected,
  editing,
  search,
  onClick,
  onContextMenu,
  onToggleDone,
  onCopyImage,
  onDelete,
  onEdit,
  onEditDone,
}: {
  item: Item;
  attachments: Attachment[];
  selected: boolean;
  editing: boolean;
  search: string;
  onClick: (e: ReactMouseEvent) => void;
  onContextMenu: (e: ReactMouseEvent) => void;
  onToggleDone: () => void;
  onCopyImage: () => void;
  onDelete: () => void;
  onEdit: () => void;
  onEditDone: () => void;
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [draft, setDraft] = useState(item.content);
  const [thumbUrls, setThumbUrls] = useState<Map<number, string>>(new Map());

  const done = item.done !== 0;

  useEffect(() => {
    if (item.kind !== "image" || !item.imagePath) return;
    void api.imageData(item.imagePath).then(setImageUrl);
  }, [item.id, item.imagePath]);

  useEffect(() => {
    for (const a of attachments) {
      void api.imageData(a.path).then((url) => {
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
      className={`item-card${selected ? " selected" : ""}${done ? " done" : ""}`}
      data-item-id={item.id}
      data-item-kind={item.kind}
      onClick={onClick}
      onContextMenu={onContextMenu}
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
        {item.kind === "image" && imageUrl ? (
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
                  {url ? <img src={url} alt="" onClick={(e) => {
                    e.stopPropagation();
                    onCopyImage();
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
    </div>
  );
}
