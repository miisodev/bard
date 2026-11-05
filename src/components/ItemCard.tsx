import {
  Copy,
  Ellipsis,
  Eye,
  Star,
} from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { colorName } from "../color/names";
import { linkParts } from "../content";
import { renderInline } from "../markdown";
import { itemKind, itemPreview, KIND_LABEL, colorChannels, toHex } from "../shelf";
import { api } from "../store";
import type { Attachment, Item } from "../types";
import KindGlyph from "./KindGlyph";

/*
  One shelf card: a checkbox, its content and one Copy.

  The head strip carries the kind — written, glyphed and tinted, in that order
  of authority — with the quick-paste badge on every card whose digit is bound.
  The body is the content. The foot answers the one question the content
  cannot: where a link points, how long a note is, how large an image is. No
  timestamps and no chrome beyond the card's own commands.

  The card is one tab stop in the zone, not one per card: the listbox owns the
  cursor, `tabIndex` roves, and the buttons inside stay in the tab order so a
  card's own commands are one Tab away. The row therefore handles no key
  itself — every command on it is the panel's.
*/

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
}: {
  item: Item;
  attachments: Attachment[];
  selected: boolean;
  /** The roving tab stop and the keyboard cursor. Exactly one card at a time. */
  active?: boolean;
  /** A capture that has not been on screen before; it alone animates in. */
  fresh?: boolean;
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
}) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [imageMissing, setImageMissing] = useState(false);
  const [dimensions, setDimensions] = useState<string | null>(null);
  const [thumbUrls, setThumbUrls] = useState<Map<number, string>>(new Map());
  const [clamped, setClamped] = useState(false);
  const [onScreen, setOnScreen] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);
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

  /**
   * A card asks for its picture only when the card can be seen.
   *
   * The panel renders one page at a time, so on a normal shelf this fires in
   * the first frame and changes nothing. It is the off-screen cases that made
   * the read worth deferring: a page wider than the zone, a card scrolled past,
   * and — the one that matters — the panel mounting while its window is still
   * hidden, which is every launch until the shelf is summoned. Nothing is
   * fetched, decoded or held for pixels nobody is looking at.
   */
  useEffect(() => {
    const card = cardRef.current;
    if (!card) return;
    if (typeof IntersectionObserver !== "function") {
      setOnScreen(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        setOnScreen(true);
        observer.disconnect();
      },
      // A card just off the edge is fetched before it is needed, so scrolling
      // to it does not show a placeholder.
      { rootMargin: "200px" },
    );
    observer.observe(card);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (item.kind !== "image" || !onScreen) return;
    let alive = true;
    // The asset URL, not the file: the card was asking for a base64 copy of a
    // multi-megabyte screenshot per mount, decoded again by the compositor at
    // full size. Rust has already answered whether the file is there.
    void api.itemCoverUrl(item.id).then((url) => {
      if (!alive) return;
      setImageUrl(url);
      setImageMissing(!url);
    });
    return () => {
      alive = false;
    };
  }, [item.id, item.kind, onScreen]);

  useEffect(() => {
    // Rebuilt, not accumulated. Detach an attachment and its file is unlinked, but
    // the base64 data URL for the dead id would otherwise stay in state for the
    // life of the card — real memory held for a thumbnail of a file that no longer
    // exists, across every card in the timeline.
    let alive = true;
    setThumbUrls(new Map());
    if (!onScreen) return;
    for (const a of attachments) {
      void api.attachmentCoverUrl(a.id).then((url) => {
        if (!alive || !url) return;
        setThumbUrls((m) => new Map(m).set(a.id, url));
      });
    }
    return () => {
      alive = false;
    };
  }, [attachments.map((a) => a.id).join(","), onScreen]);

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

  return (
    <div
      ref={cardRef}
      className={`item-card${selected ? " selected" : ""}${active ? " active" : ""}${done ? " done" : ""}${
        fresh ? " fresh" : ""
      }`}
      role="option"
      // Roving: one card is in the tab order, the rest are reached by arrow keys.
      tabIndex={active ? 0 : -1}
      aria-selected={selected}
      aria-label={color ? `${colorLabel} ${hex ?? color}` : `${label}: ${itemPreview(item)}`}
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
          <span className="card-glyph"><KindGlyph kind={kind} /></span>
          {index < 9 ? (
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
          aria-label={`Copy ${itemPreview(item)}`}
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
