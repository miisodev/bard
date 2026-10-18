import {
  Copy,
  Droplet,
  Ellipsis,
  Eye,
  Image as ImageIcon,
  Link as LinkIcon,
  Mic,
  PenTool,
  Star,
  Type,
} from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import { colorName } from "../color/names";
import { linkParts } from "../content";
import { renderInline } from "../markdown";
import { KIND_LABEL, colorChannels, itemKind, toHex, type ShelfKind } from "../shelf";
import { api } from "../store";
import type { Attachment, Item } from "../types";

/*
  One shelf card: a checkbox, its content and one Copy.

  The head strip carries the kind — written, glyphed and tinted, in that order
  of authority — with the quick-paste badge on the first five cards of a page.
  The body is the content. The foot answers the one question the content
  cannot: where a link points, how long a note is, how large an image is. No
  timestamps and no chrome beyond the card's own commands.

  The card is one tab stop in the zone, not one per card: the listbox owns the
  cursor, `tabIndex` roves, and the buttons inside stay in the tab order so a
  card's own commands are one Tab away. The row therefore handles no key
  itself — every command on it is the panel's.
*/

/** Long enough to identify a card, short enough to be read out loud. */
const PREVIEW_CHARS = 48;

/** The kind's glyph, always beside the written label — colour is never alone. */
const GLYPHS: Record<ShelfKind, ReactNode> = {
  text: <Type size={13} strokeWidth={2} />,
  image: <ImageIcon size={13} strokeWidth={2} />,
  link: <LinkIcon size={13} strokeWidth={2} />,
  color: <Droplet size={13} strokeWidth={2} />,
  dictation: <Mic size={13} strokeWidth={2} />,
  draw: <PenTool size={13} strokeWidth={2} />,
};

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

function countCharacters(text: string): string {
  const length = [...text].length;
  return length === 1 ? "1 character" : `${length.toLocaleString()} characters`;
}

export default function ItemCard({
  item,
  attachments,
  selected,
  active = false,
  fresh = false,
  editing,
  search,
  index,
  onClick,
  onFocus,
  onContextMenu,
  onMore,
  onToggleDone,
  onCopy,
  onFavorite,
  onPreview,
  onDelete,
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
  /** The card's position on its page, for the quick-paste badge. */
  index: number;
  onClick: (e: ReactMouseEvent) => void;
  /** The row — or anything inside it — took DOM focus. */
  onFocus: () => void;
  onContextMenu: (e: ReactMouseEvent) => void;
  /** Open the context menu at a point, from the More control or the key. */
  onMore: (at: { x: number; y: number }) => void;
  onToggleDone: () => void;
  onCopy: () => void;
  onFavorite: () => void;
  onPreview: () => void;
  onDelete: () => void;
  /** The inline edit finished — committed, cancelled or blurred away. */
  onEditDone: () => void;
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [imageMissing, setImageMissing] = useState(false);
  const [dimensions, setDimensions] = useState<string | null>(null);
  const [draft, setDraft] = useState(item.content);
  const [thumbUrls, setThumbUrls] = useState<Map<number, string>>(new Map());
  const [clamped, setClamped] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  const kind = itemKind(item);
  const label = KIND_LABEL[kind];
  const done = item.done !== 0;
  const color = kind === "color" ? item.content.trim() : null;
  const channels = color ? colorChannels(color) : null;
  const hex = channels ? toHex(channels) : null;
  const colorLabel = channels && hex ? colorName(hex) : "Color";
  const link = kind === "link" ? linkParts(item.content) : null;
  const meta =
    kind === "link" && link
      ? link.host
      : kind === "image" || kind === "draw"
        ? dimensions
        : kind === "color"
          ? null
          : countCharacters(item.content);

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

  // The clamp hides the rest of a note with nothing to say so. CSS cannot tell
  // a clamped box from a short one, so the fade is measured: once per content
  // change, and again only when the box itself changes size.
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
      aria-label={color ? `${colorLabel} ${hex ?? color}` : `${label}: ${preview(item)}`}
      data-item-id={item.id}
      data-item-kind={item.kind}
      data-shelf-kind={kind}
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
          const rect = event.currentTarget.getBoundingClientRect();
          onMore({ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 });
        }
      }}
    >
      <div className="card-head" data-kind={kind}>
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
        <span className="card-label">{label}</span>
        <span className="card-signs">
          <span className="card-glyph" aria-hidden="true">{GLYPHS[kind]}</span>
          {index < 5 ? (
            <kbd className="card-key" title={`Ctrl+${index + 1} pastes this card`} aria-hidden="true">
              {index + 1}
            </kbd>
          ) : null}
        </span>
      </div>

      <div className="card-body">
        {item.kind === "image" && !imageUrl ? (
          // An image row must never render as an empty card: while it loads it
          // holds its place, and a file that is gone says so.
          <div className={imageMissing ? "item-missing" : "item-image-pending"}>
            {imageMissing ? "Image unavailable" : null}
          </div>
        ) : item.kind === "image" && imageUrl ? (
          <img
            src={imageUrl}
            alt=""
            className="item-image"
            onLoad={(event) => {
              const image = event.currentTarget;
              if (image.naturalWidth > 0) {
                setDimensions(`${image.naturalWidth} × ${image.naturalHeight}`);
              }
            }}
          />
        ) : editing && item.kind === "text" ? (
          <textarea
            className="inline-edit"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              // The editor owns Escape and Enter while it is open. Without
              // this, Escape would fall through to the shelf's own order and
              // could clear the selection or retract the window under the
              // edit being cancelled.
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                e.stopPropagation();
                commitEdit();
              }
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                onEditDone();
              }
            }}
            onBlur={commitEdit}
          />
        ) : color ? (
          <div className="card-color">
            <span className="color-chip" style={{ background: color }} aria-hidden="true" />
            <span className="color-text">
              <span className="color-name">{colorLabel}</span>
              <span className="color-value">{hex ?? color}</span>
            </span>
          </div>
        ) : (
          <div
            ref={bodyRef}
            className={`item-body${clamped ? " clamped" : ""}`}
            // Only where text is actually hidden: a tooltip that repeats what
            // the card already shows is noise on every card in a row.
            title={clamped ? item.content : undefined}
          >
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

      <div className="card-foot">
        {kind === "color" ? (
          <span className="card-tools">
            <button
              type="button"
              className={`card-tool${item.favorite !== 0 ? " on" : ""}`}
              aria-pressed={item.favorite !== 0}
              aria-label={item.favorite !== 0 ? "Remove favorite" : "Add favorite"}
              title="Favorite"
              onClick={(e) => {
                e.stopPropagation();
                onFavorite();
              }}
            >
              <Star size={13} strokeWidth={1.8} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="card-tool"
              aria-label={`Preview ${colorLabel}`}
              title="Preview"
              onClick={(e) => {
                e.stopPropagation();
                onPreview();
              }}
            >
              <Eye size={13} strokeWidth={1.8} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="card-tool"
              aria-label="More actions"
              aria-haspopup="menu"
              title="More"
              onClick={(e) => {
                e.stopPropagation();
                const rect = e.currentTarget.getBoundingClientRect();
                onMore({ x: rect.left, y: rect.bottom + 4 });
              }}
            >
              <Ellipsis size={13} strokeWidth={1.8} aria-hidden="true" />
            </button>
          </span>
        ) : meta ? (
          <span className="card-meta" title={meta}>{meta}</span>
        ) : null}
        <span className="card-spring" />
        <button
          type="button"
          className="item-copy"
          aria-label={`Copy ${preview(item)}`}
          title="Copy"
          onClick={(e) => {
            e.stopPropagation();
            onCopy();
          }}
        >
          {/* Lucide, like every other icon in the product. */}
          <Copy size={15} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
