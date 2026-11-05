import { useEffect, useRef, useState } from "react";
import { Copy, X } from "lucide-react";
import { itemKind, KIND_LABEL } from "../shelf";
import { api } from "../store";
import type { Item } from "../types";

/** Read the whole capture before committing it to another application. */
export default function ItemPreview({ item, onClose }: { item: Item; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const content = useRef<HTMLPreElement>(null);
  const [image, setImage] = useState<string | null>(null);
  const [imageError, setImageError] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const element = dialog.current;
    element?.showModal();
    // Context-menu cleanup restores its trigger first; the reader then owns
    // focus, with the native dialog providing Tab containment and Escape.
    const frame = requestAnimationFrame(() => content.current?.focus());
    return () => {
      cancelAnimationFrame(frame);
      element?.close();
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  useEffect(() => {
    if (item.kind !== "image") return;
    let alive = true;
    void api.itemCoverUrl(item.id).then((url) => {
      if (!alive) return;
      setImage(url);
      setImageError(!url);
    }).catch(() => { if (alive) setImageError(true); });
    return () => { alive = false; };
  }, [item.id, item.kind]);

  const copy = async () => {
    if (busy) return;
    setBusy(true);
    setStatus(null);
    try { await api.copyItem(item.id); setStatus({ text: "Copied to clipboard", error: false }); }
    catch { setStatus({ text: "Couldn’t copy. Your capture is still here. Try again.", error: true }); }
    finally { setBusy(false); }
  };

  return (
    <dialog ref={dialog} className="item-preview" aria-labelledby="item-preview-title"
      onCancel={(event) => { event.preventDefault(); onClose(); }}>
      <header className="item-preview-head">
        <h2 id="item-preview-title">{KIND_LABEL[itemKind(item)]}</h2>
        <button type="button" className="icon-btn" aria-label="Close preview" onClick={onClose}><X size={16} aria-hidden="true" /></button>
      </header>
      {item.kind === "image" ? (
        <div className="item-preview-content" tabIndex={0}>
          {image ? <img src={image} alt="Captured image" onError={() => { setImage(null); setImageError(true); }} />
            : <p role="status">{imageError ? "This image is no longer available." : "Loading image…"}</p>}
        </div>
      ) : <pre ref={content} className="item-preview-content" tabIndex={0} aria-label="Full captured text">{item.content}</pre>}
      <footer className="item-preview-foot">
        <span role="status" className={status?.error ? "note-edit-error" : "card-meta"}>{status?.text ?? "Read, select or copy the full capture."}</span>
        <button type="button" className="btn primary" disabled={busy || imageError} aria-busy={busy} onClick={() => void copy()}>
          <Copy size={14} aria-hidden="true" />{busy ? "Copying…" : "Copy"}
        </button>
      </footer>
    </dialog>
  );
}
