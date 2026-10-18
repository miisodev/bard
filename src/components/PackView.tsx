import { ClipboardPaste, Copy, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { playCue } from "../audio";
import { InlineConfirm } from "../settings/ui";
import { itemKind, itemPreview, PACK_NO_TEXT, packText } from "../shelf";
import { api } from "../store";
import type { Item, PackSummary } from "../types";
import KindGlyph from "./KindGlyph";

/*
  The pack view: an in-shelf dialog over the shelf, listing the pack's items
  in pack order with the per-item commands, and the pack's own — paste,
  rename, delete — in the foot. It reads the membership fresh on open and is
  the only surface that changes it while it is up, so its list is the truth
  for as long as it is open.

  On the shortcut sheet's terms: a scrim, focus that starts inside and stays
  inside, Escape to leave, and focus returned to wherever it came from.
  Escape answers the confirmation first, then the rename, then the dialog.
*/

type Notify = (text: string, tone?: "info" | "error") => void;

/** Rust's refusals are written for a person; an invoke failure is not. */
function reasonText(reason: unknown, fallback: string): string {
  const detail = reason instanceof Error ? reason.message : String(reason ?? "");
  return detail.trim() || fallback;
}

export default function PackView({
  pack,
  onClose,
  onChanged,
  onDelete,
  notify,
}: {
  pack: PackSummary;
  onClose: () => void;
  /** Membership or the name changed; the shelf reloads its packs. */
  onChanged: () => void;
  /** The confirmed delete; the shelf closes the dialog and does the work. */
  onDelete: () => void;
  notify: Notify;
}) {
  const [items, setItems] = useState<Item[] | null>(null);
  const [name, setName] = useState(pack.name);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(pack.name);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);
  const restoreFocusTo = useRef<HTMLElement | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const renameRef = useRef<HTMLButtonElement>(null);
  const deleteRef = useRef<HTMLButtonElement>(null);

  const text = items ? packText(items) : "";

  // The dialog's own keys are set up once and kept away from `onClose`'s
  // identity; the states they consult are mirrored into a ref so the listener
  // never acts on a stale render.
  const phase = useRef({ renaming: false, confirming: false, name });
  useEffect(() => {
    phase.current = { renaming, confirming: confirmingDelete, name };
  });

  useEffect(() => {
    let alive = true;
    void api
      .getPack(pack.id)
      .then((detail) => {
        if (alive) setItems(detail.items);
      })
      .catch((reason: unknown) => {
        if (!alive) return;
        notify(reasonText(reason, "Couldn't open that pack."), "error");
        onClose();
      });
    return () => {
      alive = false;
    };
  }, [pack.id, notify, onClose]);

  useEffect(() => {
    restoreFocusTo.current = document.activeElement as HTMLElement | null;
    cardRef.current?.querySelector<HTMLElement>("button")?.focus();

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (phase.current.confirming) {
          setConfirmingDelete(false);
          deleteRef.current?.focus();
        } else if (phase.current.renaming) {
          setRenaming(false);
          setDraft(phase.current.name);
          renameRef.current?.focus();
        } else {
          onClose();
        }
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

  // The rename field is the one thing in the dialog that appears after it has
  // opened, so it takes focus when it does.
  useEffect(() => {
    if (!renaming) return;
    setDraft(name);
    nameRef.current?.focus();
    nameRef.current?.select();
  }, [renaming, name]);

  const paste = (item: Item, back: boolean) => {
    const write = item.kind === "image" ? api.copyItem(item.id) : api.copyText(item.content);
    void write
      .then(() => {
        playCue("copy");
        if (back) return api.pasteBack();
        notify("Copied");
      })
      .catch((reason: unknown) => {
        playCue("error");
        notify(reasonText(reason, "Couldn't copy."), "error");
      });
  };

  const pastePack = () => {
    // Nothing to assemble until the members arrive; the button follows them.
    if (items === null) return;
    if (!text) {
      notify(PACK_NO_TEXT);
      return;
    }
    void api
      .copyText(text)
      .then(() => {
        playCue("copy");
        return api.pasteBack();
      })
      .catch((reason: unknown) => {
        playCue("error");
        notify(reasonText(reason, "Couldn't paste the pack."), "error");
      });
  };

  const removeItem = (item: Item) => {
    void api
      .packRemoveItems(pack.id, [item.id])
      .then(() => {
        setItems((current) => (current ? current.filter((entry) => entry.id !== item.id) : current));
        onChanged();
        notify("Removed from pack");
      })
      .catch((reason: unknown) => {
        playCue("error");
        notify(reasonText(reason, "Couldn't remove that item."), "error");
      });
  };

  const commitRename = () => {
    const next = draft.trim();
    if (!next) {
      notify("Give the pack a name.", "error");
      return;
    }
    if (next === name) {
      setRenaming(false);
      return;
    }
    void api
      .renamePack(pack.id, next)
      .then(() => {
        setName(next);
        setRenaming(false);
        onChanged();
        notify("Renamed pack");
      })
      .catch((reason: unknown) => {
        playCue("error");
        notify(reasonText(reason, "Couldn't rename the pack."), "error");
      });
  };

  return (
    <div className="sheet" onClick={onClose}>
      <div
        ref={cardRef}
        className="sheet-card pack-view"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pack-view-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="pack-view-head">
          <h2 id="pack-view-title">{name}</h2>
          <button
            type="button"
            className="icon-btn sheet-head-close"
            aria-label="Close pack"
            onClick={onClose}
          >
            <X aria-hidden="true" />
          </button>
        </div>

        <div className="pack-view-list">
          {items === null ? null : items.length === 0 ? (
            <p className="pack-view-empty">This pack is empty.</p>
          ) : (
            items.map((item) => {
              const kind = itemKind(item);
              const preview = itemPreview(item, 64);
              return (
                <div key={item.id} className="pack-view-row">
                  <span className="pack-view-glyph">
                    <KindGlyph kind={kind} />
                  </span>
                  <span className="pack-view-text" title={itemPreview(item, 160)}>
                    {preview}
                  </span>
                  <span className="pack-view-row-actions">
                    <button
                      type="button"
                      className="pack-tool"
                      aria-label={`Paste ${preview}`}
                      title="Paste"
                      onClick={() => paste(item, true)}
                    >
                      <ClipboardPaste size={13} strokeWidth={1.8} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className="pack-tool"
                      aria-label={`Copy ${preview}`}
                      title="Copy"
                      onClick={() => paste(item, false)}
                    >
                      <Copy size={13} strokeWidth={1.8} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className="pack-tool"
                      aria-label={`Remove ${preview} from the pack`}
                      title="Remove from pack"
                      onClick={() => removeItem(item)}
                    >
                      <X size={13} strokeWidth={1.8} aria-hidden="true" />
                    </button>
                  </span>
                </div>
              );
            })
          )}
        </div>

        {confirmingDelete ? (
          <InlineConfirm
            confirmLabel="Delete pack"
            onCancel={() => {
              setConfirmingDelete(false);
              deleteRef.current?.focus();
            }}
            onConfirm={onDelete}
          >
            Delete the pack “{name}”? Its items stay in Bard.
          </InlineConfirm>
        ) : null}

        <div className="pack-view-foot">
          {renaming ? (
            <>
              <input
                ref={nameRef}
                className="field pack-view-name"
                value={draft}
                aria-label="Pack name"
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key !== "Enter") return;
                  event.preventDefault();
                  event.stopPropagation();
                  commitRename();
                }}
              />
              <button type="button" className="btn primary" onClick={commitRename}>
                Save
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setRenaming(false);
                  setDraft(name);
                }}
              >
                Cancel
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                className="btn primary"
                disabled={items !== null && text === ""}
                title={
                  items !== null && text === "" ? PACK_NO_TEXT : "Paste the pack into your app"
                }
                onClick={pastePack}
              >
                Paste pack
              </button>
              {items !== null && text === "" ? (
                <span className="pack-view-note">{PACK_NO_TEXT}</span>
              ) : null}
              <span className="pack-view-spring" />
              <button
                ref={renameRef}
                type="button"
                className="btn"
                onClick={() => setRenaming(true)}
              >
                Rename
              </button>
              <button
                ref={deleteRef}
                type="button"
                className="btn danger"
                onClick={() => setConfirmingDelete(true)}
              >
                Delete
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
