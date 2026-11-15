import { listen } from "@tauri-apps/api/event";
import {
  AlertCircle,
  Check,
  ClipboardCheck,
  File,
  Image as ImageIcon,
  Info,
  Link2,
  LoaderCircle,
  Pause,
  Pin,
  Play,
  Type,
  type LucideIcon,
} from "lucide-react";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { boot } from "../dev/boot";
import type { ClipboardPreview, PillNote, PillSlot } from "../types";
import { useTheme } from "../useTheme";
import "../styles.css";
import "./pill.css";

/*
  The status pill: what Bard just did, said at the top middle of the screen.

  Rust decides when it shows and for how long; this draws the note it is
  handed. The window is click-through, so nothing here is a control.

  It has three sizes, by how much the moment deserves:
  - a badge, for what happens many times an hour (a copy, pause, an error);
  - the slots, when a shortcut moved between the three of them;
  - a card, when there is something to read back (the clipboard, a transcript).
*/

const ICONS: Record<PillNote["kind"], LucideIcon> = {
  copy: ClipboardCheck,
  select: ClipboardCheck,
  preview: ClipboardCheck,
  listen: Info,
  work: LoaderCircle,
  done: Check,
  pause: Pause,
  resume: Play,
  error: AlertCircle,
  info: Info,
};

const LEVEL_BARS = 14;
const LINK = /^(https?:\/\/|www\.)\S+$/i;

function slotIcon(slot: PillSlot): LucideIcon {
  if (slot.kind === "image") return ImageIcon;
  if (slot.kind === "file") return File;
  return LINK.test(slot.label.trim()) ? Link2 : Type;
}

function previewMeta(preview: ClipboardPreview): string {
  switch (preview.kind) {
    case "text": {
      const length = preview.length ?? preview.text?.length ?? 0;
      return `${length.toLocaleString()} ${length === 1 ? "character" : "characters"}`;
    }
    case "image":
      return preview.width && preview.height ? `Image · ${preview.width} × ${preview.height}` : "Image";
    case "files":
      return preview.files.length === 1 ? "1 file" : `${preview.files.length} files`;
    default:
      return "";
  }
}

function wordCount(text: string): string {
  const words = text.trim().split(/\s+/).filter(Boolean).length;
  return `${words.toLocaleString()} ${words === 1 ? "word" : "words"}`;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function Body({ preview }: { preview: ClipboardPreview }) {
  if (preview.kind === "text") return <p className="pill-body-text">{preview.text}</p>;
  if (preview.kind === "image" && preview.image) {
    return <img className="pill-body-image" src={preview.image} alt="The image on the clipboard" />;
  }
  if (preview.kind === "files") {
    return (
      <ul className="pill-body-files">
        {preview.files.slice(0, 4).map((path) => (
          <li key={path}>
            <File size={13} aria-hidden="true" />
            <span>{fileName(path)}</span>
          </li>
        ))}
        {preview.files.length > 4 && <li className="more">+{preview.files.length - 4} more</li>}
      </ul>
    );
  }
  return <p className="pill-body-empty">Nothing is on the clipboard.</p>;
}

/** The three slots as rows: where the cycle is, and what the other keys hold. */
function Rail({ rail, active }: { rail: PillSlot[]; active: number | null }) {
  return (
    <ol className="pill-rail">
      {rail.map((slot, index) => {
        const Icon = slotIcon(slot);
        const on = index + 1 === active;
        return (
          <li key={index} className={on ? "on" : undefined} aria-current={on ? "true" : undefined}>
            <kbd>{index + 1}</kbd>
            <Icon size={13} aria-hidden="true" />
            <span className="pill-rail-label">{slot.label}</span>
            {slot.pinned && <Pin className="pill-rail-pin" size={11} aria-label="Pinned" />}
          </li>
        );
      })}
    </ol>
  );
}

/** Which of the slots holds what is on the clipboard, as three small marks. */
function Marks({ count, active }: { count: number; active: number | null }) {
  return (
    <span
      className="pill-marks"
      role="img"
      aria-label={active ? `Slot ${active} of ${count}` : "Not one of your three slots"}
    >
      {Array.from({ length: count }, (_, index) => (
        <i key={index} className={index + 1 === active ? "on" : undefined}>
          {index + 1}
        </i>
      ))}
    </span>
  );
}

function Levels() {
  const [levels, setLevels] = useState<number[]>(() => new Array(LEVEL_BARS).fill(0.08));
  useEffect(() => {
    const off = listen<[number, number]>("mic-level", (event) => {
      const [level] = event.payload;
      setLevels((current) => [...current.slice(1), Math.max(0.08, Math.min(1, level * 3.2))]);
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

/** How long the microphone has been open, so a long take is never a surprise. */
function Elapsed() {
  const [seconds, setSeconds] = useState(0);
  useEffect(() => {
    const started = Date.now();
    const timer = window.setInterval(() => setSeconds(Math.floor((Date.now() - started) / 1000)), 250);
    return () => window.clearInterval(timer);
  }, []);
  const minutes = Math.floor(seconds / 60);
  return (
    <span className="pill-time">
      {minutes}:{String(seconds % 60).padStart(2, "0")}
    </span>
  );
}

function Pill() {
  useTheme({ background: false });
  const [note, setNote] = useState<PillNote | null>(null);
  const [leaving, setLeaving] = useState(false);
  // The entry animation replays when the shape changes, not on every note: a
  // cycle moving from slot 1 to 2 should move the highlight, not the card.
  const [serial, setSerial] = useState(0);

  useEffect(() => {
    const offs = [
      listen<PillNote>("pill", (event) => {
        setNote((previous) => {
          if (!previous || shapeOf(previous) !== shapeOf(event.payload)) setSerial((n) => n + 1);
          return event.payload;
        });
        setLeaving(false);
      }),
      listen("pill-out", () => setLeaving(true)),
    ];
    return () => {
      for (const off of offs) void off.then((fn) => fn());
    };
  }, []);

  if (!note) return <div className="pill-stage" aria-live="polite" />;

  const Icon = ICONS[note.kind] ?? Info;
  const rail = note.rail ?? [];
  const shape = shapeOf(note);
  const transcript = note.kind === "done" && note.preview?.kind === "text";

  return (
    <div className="pill-stage" aria-live="polite">
      <div
        key={serial}
        className={`pill${leaving ? " is-leaving" : ""}`}
        data-kind={note.kind}
        data-shape={shape}
        role="status"
      >
        <div className="pill-row">
          <span className="pill-glyph">
            {note.kind === "listen" ? <span className="pill-rec" /> : <Icon size={15} strokeWidth={2.2} aria-hidden="true" />}
          </span>
          <span className="pill-text">{note.text}</span>
          {note.kind === "listen" && (
            <>
              <Levels />
              <Elapsed />
            </>
          )}
          {shape === "badge" && note.detail && <span className="pill-detail">{note.detail}</span>}
          {shape === "card" && note.preview && (
            <span className="pill-meta">
              {transcript ? wordCount(note.preview.text ?? "") : previewMeta(note.preview)}
            </span>
          )}
        </div>
        {shape === "slots" && <Rail rail={rail} active={note.slot ?? null} />}
        {shape === "card" && note.preview && (
          <div className="pill-body">
            <Body preview={note.preview} />
          </div>
        )}
        {shape === "card" && (
          <div className="pill-foot">
            <span>
              Paste with <kbd>Ctrl</kbd>
              <kbd>V</kbd>
            </span>
            {rail.length > 0 && <Marks count={rail.length} active={note.slot ?? null} />}
          </div>
        )}
      </div>
    </div>
  );
}

function shapeOf(note: PillNote): "badge" | "slots" | "card" {
  if (note.preview) return note.preview.kind === "empty" ? "badge" : "card";
  if (note.kind === "select" && note.rail && note.rail.length > 0) return "slots";
  return "badge";
}

void boot("pill").then(() => {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <Pill />
    </StrictMode>,
  );
});
