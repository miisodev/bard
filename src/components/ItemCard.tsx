import { Copy, Star, Trash2 } from "lucide-react";
import { Fragment, useEffect, useState } from "react";
import { api } from "../store";
import type { Item } from "../types";

export default function ItemCard({
  item,
  selected,
  search,
  onSelect,
  onCopy,
  onFavorite,
  onDelete,
}: {
  item: Item;
  selected: boolean;
  search: string;
  onSelect: () => void;
  onCopy: () => void;
  onFavorite: () => void;
  onDelete: () => void;
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);

  useEffect(() => {
    if (item.kind !== "image" || !item.imagePath) return;
    void api.imageData(item.imagePath).then(setImageUrl);
  }, [item.id, item.imagePath]);

  const when = formatTime(item.createdAt);

  return (
    <div
      className={`item-card${selected ? " selected" : ""}`}
      onClick={onSelect}
      onDoubleClick={onCopy}
    >
      {item.kind === "image" && imageUrl ? (
        <img src={imageUrl} alt="" onClick={onCopy} />
      ) : (
        <div className="item-body">
          <RichText text={item.content} search={search} />
        </div>
      )}
      <div className="row">
        <span className="item-meta">{when}</span>
        <span className="item-actions">
          <button className={`mini-btn${item.favorite ? " on" : ""}`} title="Favorite" onClick={onFavorite}>
            <Star size={13} strokeWidth={1.8} className={item.favorite ? "star" : undefined} />
          </button>
          <button className="mini-btn" title="Copy" onClick={onCopy}>
            <Copy size={13} strokeWidth={1.8} />
          </button>
          <button className="mini-btn danger" title="Delete" onClick={onDelete}>
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
  const sameDay = d.toDateString() === now.toDateString();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (sameDay) return `${hh}:${mm}`;
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
  while (true) {
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