import { ArrowUpRight, Paperclip, Settings, Star } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { DictationState } from "../App";
import { api } from "../store";
import type { AppState, ModelProgress } from "../types";
import AskResult from "./AskResult";
import ItemCard from "./ItemCard";
import SettingsSheet from "./SettingsSheet";

export default function Panel({
  state,
  modelProgress,
  dictation,
  onState,
}: {
  state: AppState;
  modelProgress: ModelProgress | null;
  dictation: DictationState;
  onState: (s: AppState) => void;
}) {
  const [query, setQuery] = useState("");
  const [onlyFavorites, setOnlyFavorites] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [composer, setComposer] = useState("");
  const [askBusy, setAskBusy] = useState(false);
  const [ask, setAsk] = useState<{ prompt: string; result: string } | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [attachSelection, setAttachSelection] = useState(true);
  const [toast, setToast] = useState<string | null>(null);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const timelineRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onToast = (e: Event) => {
      setToast((e as CustomEvent).detail as string);
      window.setTimeout(() => setToast(null), 2600);
    };
    const onResult = (e: Event) => {
      const text = (e as CustomEvent).detail as string;
      setComposer((c) => (c + (c ? " " : "") + text));
      setToast("Dictation transcribed");
      inputRef.current?.focus();
    };
    window.addEventListener("bard-toast", onToast);
    window.addEventListener("bard-dictation", onResult);
    return () => {
      window.removeEventListener("bard-toast", onToast);
      window.removeEventListener("bard-dictation", onResult);
    };
  }, []);

  useEffect(() => {
    void api.getState().then(onState);
  }, []);

  const refresh = () => void api.getState().then(onState);

  const items = state.items;
  const filtered = items.filter((it) => {
    if (onlyFavorites && !it.favorite) return false;
    if (!query) return true;
    if (it.kind === "image") return false;
    return it.content.toLowerCase().includes(query.toLowerCase());
  });
  const flatIds = filtered.map((i) => i.id);
  void flatIds;

  const sendAsk = async () => {
    const prompt = composer.trim();
    if (!prompt || askBusy) return;
    const ctx = attachSelection && selectedId !== null ? [selectedId] : [];
    setAskBusy(true);
    setAsk(null);
    try {
      const result = await api.ask(prompt, ctx);
      setAsk({ prompt, result });
      setComposer("");
    } catch (err) {
      setToast(String(err));
    } finally {
      setAskBusy(false);
    }
  };

  return (
    <>
      <header className="header" data-tauri-drag-region>
        <span className="wordmark">
          b<span className="dot">a</span>rd
        </span>
        <input
          className="search"
          placeholder="Search clipboard…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className={`icon-btn${onlyFavorites ? " active" : ""}`}
          title="Favorites only"
          onClick={() => setOnlyFavorites(!onlyFavorites)}
        >
          <Star size={15} strokeWidth={1.8} />
        </button>
        <button className="icon-btn" title="Settings" onClick={() => setShowSettings(true)}>
          <Settings size={15} strokeWidth={1.8} />
        </button>
      </header>

      <div className="timeline" ref={timelineRef}>
        {items.length === 0 ? (
          <EmptyState onInstall={() => setShowSettings(true)} />
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <div className="title">No matches</div>
            <div className="hint">Nothing here matches “{query}”.</div>
          </div>
        ) : (
          filtered.map((it) => (
            <ItemCard
              key={it.id}
              item={it}
              selected={it.id === selectedId}
              search={query}
              onSelect={() => setSelectedId(it.id)}
              onCopy={() => void api.copyItem(it.id).then(() => setToast("Copied to clipboard"))}
              onFavorite={() => {
                void api.setFavorite(it.id, !it.favorite).then(refresh);
              }}
              onDelete={() => {
                void api.deleteItem(it.id).then(() => {
                  setSelectedId(null);
                  refresh();
                });
              }}
            />
          ))
        )}
      </div>

      <div className="composer">
        {dictation.listening || dictation.transcribing ? (
          <div className="dictate-indicator">
            <span className="pulse-dot" />
            {dictation.transcribing ? "Transcribing…" : "Listening — speak now"}
          </div>
        ) : null}
        {attachSelection && selectedId !== null ? (
          <div className="context-chip">
            <Paperclip size={11} />
            <span>Attached to selected item</span>
            <button onClick={() => setAttachSelection(false)}>×</button>
          </div>
        ) : null}
        {ask ? (
          <AskResult
            prompt={ask.prompt}
            result={ask.result}
            busy={askBusy}
            onDismiss={() => setAsk(null)}
          />
        ) : null}
        <div className="composer-row">
          <textarea
            ref={inputRef}
            className="composer-input"
            rows={1}
            placeholder="Ask anything — or dictate with Ctrl+Space…"
            value={composer}
            onChange={(e) => setComposer(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void sendAsk();
              }
            }}
          />
          <button
            className={`attach-btn${attachSelection && selectedId !== null ? " active" : ""}`}
            title="Attach selected item as context"
            onClick={() => setAttachSelection(!attachSelection)}
          >
            <Paperclip size={14} />
          </button>
          <button
            className={`send-btn${askBusy ? " busy" : ""}`}
            disabled={!composer.trim() || askBusy}
            title="Send (Enter)"
            onClick={() => void sendAsk()}
          >
            <ArrowUpRight size={15} />
          </button>
        </div>
      </div>

      {showSettings ? (
        <SettingsSheet
          state={state}
          modelProgress={modelProgress}
          onClose={() => setShowSettings(false)}
          onState={onState}
        />
      ) : null}
      {toast ? <div className="toast">{toast}</div> : null}
    </>
  );
}

function EmptyState({ onInstall }: { onInstall: () => void }) {
  return (
    <div className="empty-state">
      <div className="title">Nothing captured yet</div>
      <div className="hint">
        Copy anything, anywhere — it lands here. Or tap{" "}
        <kbd>Shift</kbd> <kbd>Shift</kbd> to capture a selection without copying it.
      </div>
      <button className="btn primary" onClick={onInstall}>
        Set up Bard
      </button>
    </div>
  );
}