import { Captions, Copy, FileText, Pencil, Star, Trash2 } from "lucide-react";
import { useEffect, useRef } from "react";
import type { Item } from "../types";

export type MenuAction = "copy" | "copyList" | "favorite" | "edit" | "caption" | "delete";

export default function ContextMenu({
  x,
  y,
  multi,
  item,
  onAction,
  onClose,
}: {
  x: number;
  y: number;
  multi: boolean;
  item?: Item;
  onAction: (a: MenuAction) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const winW = window.innerWidth;
  const winH = window.innerHeight;
  const left = Math.min(x, winW - 180);
  const top = Math.min(y, winH - (multi ? 200 : 260));

  const Row = ({
    icon,
    label,
    action,
    danger,
  }: {
    icon: React.ReactNode;
    label: string;
    action?: MenuAction;
    danger?: boolean;
  }) => (
    <button
      className={`ctx-row${danger ? " danger" : ""}`}
      onClick={() => action && onAction(action)}
    >
      {icon}
      <span>{label}</span>
    </button>
  );

  return (
    <div ref={ref} className="ctx-menu" style={{ left, top }}>
      {multi ? (
        <>
          <Row icon={<Copy size={13} strokeWidth={1.8} />} label="Copy selection" action="copy" />
          <Row
            icon={<FileText size={13} strokeWidth={1.8} />}
            label="Copy as list"
            action="copyList"
          />
          <Row icon={<Star size={13} strokeWidth={1.8} />} label="Toggle favorite" action="favorite" />
          <div className="ctx-sep" />
          <Row icon={<Trash2 size={13} strokeWidth={1.8} />} label="Delete selection" action="delete" danger />
        </>
      ) : item ? (
        <>
          <Row icon={<Copy size={13} strokeWidth={1.8} />} label="Copy" action="copy" />
          <Row icon={<Star size={13} strokeWidth={1.8} />} label={item.favorite ? "Unfavorite" : "Favorite"} action="favorite" />
          {item.kind === "text" ? (
            <Row icon={<Pencil size={13} strokeWidth={1.8} />} label="Edit" action="edit" />
          ) : (
            <Row icon={<Captions size={13} strokeWidth={1.8} />} label="Add note to image" action="caption" />
          )}
          <div className="ctx-sep" />
          <Row icon={<Trash2 size={13} strokeWidth={1.8} />} label="Delete" action="delete" danger />
        </>
      ) : null}
    </div>
  );
}