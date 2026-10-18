import { Ellipsis, Image as ImageIcon, Maximize2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { colorName } from "../color/names";
import { linkParts } from "../content";
import {
  colorChannels,
  itemKind,
  itemPreview,
  PACK_NO_TEXT,
  packText,
  toHex,
  type ShelfKind,
} from "../shelf";
import { api } from "../store";
import type { Item, PackSummary } from "../types";

/*
  One pack in the Packs zone: a collage of its leading members, its name and
  how many items it holds. The whole card is the paste — the primary action,
  as on an item card — with Open and More beside the name for the rest. The
  keyboard lands on the card itself (Enter pastes, Delete asks), which is why
  the card is a focusable option rather than a button: a paste surface that
  contains controls cannot be a button.

  A pack whose members carry no text cannot be pasted, and says so on its own
  line rather than failing silently at the clipboard.
*/

/** How much of an item's text a preview tile shows. */
const TILE_CHARS = 32;

export default function PackCard({
  pack,
  previewItems,
  editing,
  onPaste,
  onOpen,
  onMore,
  onContextMenu,
  onRename,
  onEditDone,
}: {
  pack: PackSummary;
  /** The leading members, in pack order, resolved against the shelf's items. */
  previewItems: Item[];
  /** The pack's name is being edited in place. */
  editing: boolean;
  /** The primary action: write the pack's text and paste it. */
  onPaste: () => void;
  /** Open the pack view. */
  onOpen: () => void;
  /** Open the card's menu at a point, from the More control or the key. */
  onMore: (at: { x: number; y: number }) => void;
  onContextMenu: (e: ReactMouseEvent) => void;
  onRename: (name: string) => void;
  /** The inline rename finished — committed or cancelled. */
  onEditDone: () => void;
}) {
  const [images, setImages] = useState<Map<number, string | null>>(new Map());
  // Whether the pack has anything pasteable, known once its members are read.
  // Until then the card does not claim either way.
  const [noText, setNoText] = useState(false);
  const [draft, setDraft] = useState(pack.name);
  const cardRef = useRef<HTMLDivElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const wasEditing = useRef(false);

  const count = pack.itemCount === 1 ? "1 item" : `${pack.itemCount} items`;

  // The preview tiles paint from the summary's leading ids, resolved against
  // the shelf's items; a tile's image comes through the item-image data path.
  // The id key is the dependency, not the array: the items behind the ids are
  // re-read on every shelf refresh, and re-fetching every thumbnail with them
  // would be work with no visible effect.
  const previewKey = pack.previewItemIds.join(",");
  const imageIds = useMemo(
    () =>
      previewKey === ""
        ? []
        : previewItems.filter((item) => item.kind === "image").map((item) => item.id),
    [pack.id, previewKey],
  );

  // The full membership is read once per membership change, to know whether
  // there is text to paste at all.
  useEffect(() => {
    let alive = true;
    void api
      .getPack(pack.id)
      .then((detail) => {
        if (alive) setNoText(packText(detail.items) === "");
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [pack.id, pack.itemCount, previewKey]);

  useEffect(() => {
    let alive = true;
    setImages(new Map());
    for (const id of imageIds) {
      void api.itemImageData(id).then((url) => {
        if (alive && url) setImages((current) => new Map(current).set(id, url));
      });
    }
    return () => {
      alive = false;
    };
  }, [pack.id, imageIds]);

  // Focus follows the rename: the input when it opens, the card when it ends.
  // A passive effect, so it lands after the menu that asked for the rename has
  // handed focus back. `pack.name` is read, not watched: the effect runs when
  // edit begins, and that render carries the current name.
  useEffect(() => {
    if (editing) {
      setDraft(pack.name);
      nameRef.current?.focus();
      nameRef.current?.select();
    } else if (wasEditing.current) {
      cardRef.current?.focus();
    }
    wasEditing.current = editing;
  }, [editing]);

  const commitName = () => {
    const name = draft.trim();
    // An empty name is Rust's refusal to speak; only a real change is sent.
    if (name && name !== pack.name) onRename(name);
    onEditDone();
  };

  return (
    <div
      ref={cardRef}
      className={`pack-card${editing ? " editing" : ""}`}
      role="option"
      tabIndex={0}
      aria-label={
        noText ? `${pack.name} — ${count} — ${PACK_NO_TEXT}` : `Paste ${pack.name} — ${count}`
      }
      aria-disabled={noText || undefined}
      data-pack-id={pack.id}
      onClick={() => onPaste()}
      onContextMenu={onContextMenu}
      onKeyDown={(event) => {
        // Only the context-menu key belongs to the card; Enter and Delete are
        // the panel's, and are read against the focused card's own id.
        if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
          event.preventDefault();
          event.stopPropagation();
          const rect = event.currentTarget.getBoundingClientRect();
          onMore({
            x: Math.max(8, Math.min(rect.left + rect.width / 2, window.innerWidth - 200)),
            y: rect.top + rect.height / 2,
          });
        }
      }}
    >
      <div className="pack-collage" aria-hidden="true">
        {previewItems.slice(0, 4).map((item) => (
          <PackTile key={item.id} item={item} image={images.get(item.id) ?? null} />
        ))}
      </div>

      <div className="pack-head">
        {editing ? (
          <input
            ref={nameRef}
            className="pack-name-input"
            value={draft}
            aria-label="Pack name"
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              // The editor owns Escape and Enter while it is open.
              if (e.key === "Enter") {
                e.preventDefault();
                e.stopPropagation();
                commitName();
              }
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setDraft(pack.name);
                onEditDone();
              }
            }}
            onBlur={commitName}
          />
        ) : (
          <span className="pack-name">{pack.name}</span>
        )}
        <span className={`pack-meta${noText ? " no-text" : ""}`} title={noText ? PACK_NO_TEXT : undefined}>
          {noText ? "No text to paste" : count}
        </span>
        <span className="pack-actions">
          <button
            type="button"
            className="pack-tool"
            aria-label={`Open ${pack.name}`}
            title="Open pack"
            onClick={(e) => {
              e.stopPropagation();
              onOpen();
            }}
          >
            <Maximize2 size={13} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="pack-tool"
            aria-label={`More actions for ${pack.name}`}
            aria-haspopup="menu"
            title="More"
            onClick={(e) => {
              e.stopPropagation();
              const rect = e.currentTarget.getBoundingClientRect();
              // The packs zone sits against the window's right edge, so the
              // menu opens with its right edge under the button.
              onMore({ x: Math.max(8, rect.right - 200), y: rect.bottom + 4 });
            }}
          >
            <Ellipsis size={13} strokeWidth={1.8} aria-hidden="true" />
          </button>
        </span>
      </div>
    </div>
  );
}

/** One tile of the collage: the item's picture, its colour, or its words. */
function PackTile({ item, image }: { item: Item; image: string | null }) {
  const kind = itemKind(item);
  if (item.kind === "image") {
    return image ? (
      <span className="pack-tile media">
        <img src={image} alt="" />
      </span>
    ) : (
      <span className="pack-tile media empty">
        <ImageIcon size={12} strokeWidth={1.8} aria-hidden="true" />
      </span>
    );
  }
  if (kind === "color") {
    const channels = colorChannels(item.content);
    return (
      <span
        className="pack-tile color"
        style={{ background: item.content.trim() }}
        title={channels ? colorName(toHex(channels)) : undefined}
      />
    );
  }
  return (
    <span className={`pack-tile ${kind}`}>
      <span className="pack-tile-text">{tileText(item, kind)}</span>
    </span>
  );
}

/** What a text tile shows: a link reads as its host, everything else as itself. */
function tileText(item: Item, kind: ShelfKind): string {
  if (kind === "link") {
    const link = linkParts(item.content);
    if (link) return link.host;
  }
  return itemPreview(item, TILE_CHARS);
}
