import { listen } from "@tauri-apps/api/event";
import {
  AlertCircle,
  Check,
  ClipboardCheck,
  ClipboardCopy,
  Eye,
  FileText,
  Info,
  LoaderCircle,
  Mic,
  Pause,
  Play,
  type LucideIcon,
} from "lucide-react";
import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { boot } from "../dev/boot";
import type { ClipboardPreview, PillNote } from "../types";
import { useTheme } from "../useTheme";
import "../styles.css";
import "./pill.css";

/*
  The status pill. Rust decides when it shows and for how long; this draws the
  note it is handed and animates it in and out. The window is click-through,
  so nothing here is interactive: it is a sign, not a control.
*/

const ICONS: Record<PillNote["kind"], LucideIcon> = {
  copy: ClipboardCheck,
  select: ClipboardCopy,
  preview: Eye,
  listen: Mic,
  work: LoaderCircle,
  done: Check,
  pause: Pause,
  resume: Play,
  error: AlertCircle,
  info: Info,
};

const LEVEL_BARS = 9;

function previewMeta(preview: ClipboardPreview): string {
  switch (preview.kind) {
    case "text": {
      const length = preview.length ?? preview.text?.length ?? 0;
      return `Text · ${length.toLocaleString()} ${length === 1 ? "character" : "characters"}`;
    }
    case "image":
      return preview.width && preview.height ? `Image · ${preview.width} × ${preview.height}` : "Image";
    case "files":
      return preview.files.length === 1 ? "1 file" : `${preview.files.length} files`;
    default:
      return "Empty";
  }
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function PreviewCard({ preview }: { preview: ClipboardPreview }) {
  return (
    <div className="pill-preview">
      <div className="pill-preview-meta">{previewMeta(preview)}</div>
      {preview.kind === "text" && <p className="pill-preview-text">{preview.text}</p>}
      {preview.kind === "image" && preview.image && (
        <img className="pill-preview-image" src={preview.image} alt="The image on the clipboard" />
      )}
      {preview.kind === "files" && (
        <ul className="pill-preview-files">
          {preview.files.slice(0, 5).map((path) => (
            <li key={path}>
              <FileText size={14} aria-hidden="true" />
              <span>{fileName(path)}</span>
            </li>
          ))}
          {preview.files.length > 5 && <li className="more">+{preview.files.length - 5} more</li>}
        </ul>
      )}
      {preview.kind === "empty" && <p className="pill-preview-empty">Nothing is on the clipboard.</p>}
    </div>
  );
}

function Levels() {
  const [levels, setLevels] = useState<number[]>(() => new Array(LEVEL_BARS).fill(0.1));
  useEffect(() => {
    const off = listen<[number, number]>("mic-level", (event) => {
      const [level] = event.payload;
      setLevels((current) => [...current.slice(1), Math.max(0.1, Math.min(1, level * 3.2))]);
    });
    return () => {
      void off.then((fn) => fn());
    };
  }, []);
  return (
    <span className="pill-levels" aria-hidden="true">
      {levels.map((value, index) => (
        <i key={index} style={{ "--level": value } as React.CSSProperties} />
      ))}
    </span>
  );
}

function Pill() {
  useTheme({ background: false });
  const [note, setNote] = useState<PillNote | null>(null);
  const [leaving, setLeaving] = useState(false);
  // Bumped per note so the same note twice in a row still replays its entry.
  const [serial, setSerial] = useState(0);
  const live = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const offs = [
      listen<PillNote>("pill", (event) => {
        setNote(event.payload);
        setLeaving(false);
        setSerial((n) => n + 1);
      }),
      listen("pill-out", () => setLeaving(true)),
    ];
    return () => {
      for (const off of offs) void off.then((fn) => fn());
    };
  }, []);

  if (!note) return <div className="pill-stage" aria-live="polite" />;

  const Icon = ICONS[note.kind] ?? Info;
  const isPreview = note.kind === "preview" && note.preview;
  return (
    <div className="pill-stage" aria-live="polite" ref={live}>
      <div
        key={serial}
        className={`pill${isPreview ? " is-preview" : ""}${leaving ? " is-leaving" : ""}`}
        data-kind={note.kind}
        role="status"
      >
        <div className="pill-row">
          <span className="pill-icon">
            {note.kind === "listen" ? <span className="pill-rec" /> : <Icon size={15} strokeWidth={2.2} aria-hidden="true" />}
          </span>
          <span className="pill-text">{note.text}</span>
          {note.kind === "listen" && <Levels />}
          {note.detail && <span className="pill-detail">{note.detail}</span>}
          {note.slot != null && note.slots != null && (
            <span className="pill-slots" role="img" aria-label={`Item ${note.slot} of ${note.slots}`}>
              {Array.from({ length: Math.max(note.slots, 1) }, (_, index) => (
                <i key={index} className={index + 1 === note.slot ? "on" : undefined} />
              ))}
            </span>
          )}
        </div>
        {isPreview && note.preview && <PreviewCard preview={note.preview} />}
      </div>
    </div>
  );
}

void boot("pill").then(() => {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <Pill />
    </StrictMode>,
  );
});
