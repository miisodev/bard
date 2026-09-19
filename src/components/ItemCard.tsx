import { Copy, Star, Trash2 } from "lucide-react";
import { Fragment, useEffect, useState, type MouseEvent as ReactMouseEvent } from "react";
import { api } from "../store";
import type { Attachment, Item } from "../types";

export default function ItemCard({
  item,
  attachments,
  selected,
  editing,
  search,
  onClick,
  onContextMenu,
  onCopy,
  onFavorite,
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
  onCopy: () => void;
  onFavorite: () => void;
  onDelete: () => void;
  onEdit: () => void;
  onEditDone: () => void;
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [draft, setDraft] = useState(item.content);
  const [thumbUrls, setThumbUrls] = useState<Map<number, string>>(new Map());

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
      className={`item-card${selected ? " selected" : ""}`}
      data-item-id={item.id}
      data-item-kind={item.kind}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      {item.kind === "image" && imageUrl ? (
        <img src={imageUrl} alt="" />
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
          <RichText text={item.content} search={search} />
        </div>
      )}
      {attachments.length > 0 ? (
        <div className="thumb-row">
          {attachments.map((a) => {
            const url = thumbUrls.get(a.id);
            return (
              <span key={a.id} className="thumb">
                {url ? <img src={url} alt="" onClick={onCopy} /> : null}
                <button
                  className="thumb-x"
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
      <div className="row">
        <span className="item-meta">{formatTime(item.createdAt)}</span>
        <span className="item-actions">
          <button
            className={`mini-btn${item.favorite ? " on" : ""}`}
            title="Favorite (Space)"
            onClick={(e) => {
              e.stopPropagation();
              onFavorite();
            }}
          >
            <Star size={13} strokeWidth={1.8} className={item.favorite ? "star" : undefined} />
          </button>
          <button
            className="mini-btn"
            title="Copy (Enter)"
            onClick={(e) => {
              e.stopPropagation();
              onCopy();
            }}
          >
            <Copy size={13} strokeWidth={1.8} />
          </button>
          <button
            className="mini-btn danger"
            title="Delete (Del)"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
          >
            <Trash2 size={13} strokeWidth={1.8} />
          </button>
        </span>
      </div>
    </div>
  );
}

function formatTime(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (d.toDateString() === now.toDateString()) return `${hh}:${mm}`;
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (d.toDateString() === yesterday.toDateString()) return `Yesterday ${hh}:${mm}`;
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

function RichText({ text, search }: { text: string; search: string }) {
  if (!search) return <>{text}</>;
  const lower = text.toLowerCase();
  const q = search.toLowerCase();
  const parts: { hit: boolean; text: string }[] = [];
  let i = 0;
  for (;;) {
    const j = lower.indexOf(q, i);
    if (j < 0) {
      parts.push({ hit: false, text: text.slice(i) });
      break;
    }
    if (j > i) parts.push({ hit: false, text: text.slice(i, j) });
    parts.push({ hit: true, text: text.slice(j, j + q.length) });
    i = j + q.length;
  }
  return (
    <>
      {parts.map((p, k) =>
        p.hit ? <mark key={k}>{p.text}</mark> : <Fragment key={k}>{p.text}</Fragment>
      )}
    </>
  );
}