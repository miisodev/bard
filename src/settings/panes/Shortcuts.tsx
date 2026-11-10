import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState, type KeyboardEvent } from "react";
import { api } from "../../store";
import type { ShortcutRow, ShortcutState } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row } from "../ui";

/** Browser key names → the names Bard's shortcut table uses. */
const KEY_NAMES: Record<string, string> = {
  " ": "Space",
  ",": "Comma",
  ".": "Period",
  "/": "Slash",
  ";": "Semicolon",
  "'": "Quote",
  "[": "BracketLeft",
  "]": "BracketRight",
  "\\": "Backslash",
  "`": "Backquote",
  "-": "Minus",
  "=": "Equal",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

/** The physical key, so Shift+1 records as 1 rather than "!". */
function keyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  const byCode: Record<string, string> = {
    Space: "Space",
    Comma: "Comma",
    Period: "Period",
    Slash: "Slash",
    Semicolon: "Semicolon",
    Quote: "Quote",
    BracketLeft: "BracketLeft",
    BracketRight: "BracketRight",
    Backslash: "Backslash",
    Backquote: "Backquote",
    Minus: "Minus",
    Equal: "Equal",
  };
  return byCode[code] ?? null;
}

export function chordFromEvent(event: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string | null {
  if (["Control", "Alt", "Shift", "Meta", "OS", "AltGraph"].includes(event.key)) return null;
  const key = keyFromCode(event.code) ?? KEY_NAMES[event.key] ?? (/^F([1-9]|1[0-2])$/.test(event.key) ? event.key : null) ??
    (["Enter", "Tab", "Insert", "Delete", "Home", "End", "PageUp", "PageDown"].includes(event.key) ? event.key : null);
  if (!key) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  parts.push(key);
  return parts.join("+");
}

function Keys({ display }: { display: string }) {
  if (!display) return <span className="keys-off">Off</span>;
  return (
    <span className="keys static">
      {display.split("+").map((key, index) => (
        <kbd key={`${key}-${index}`}>{key}</kbd>
      ))}
    </span>
  );
}

function StatusNote({ row }: { row: ShortcutRow }) {
  if (row.status === "taken") return <span className="shortcut-status taken">Used by another app</span>;
  if (row.status === "pending") return <span className="shortcut-status">Waiting for your desktop</span>;
  return null;
}

export default function ShortcutsPane({ state }: PaneProps) {
  const [shortcuts, setShortcuts] = useState<ShortcutState | null>(null);
  const [recording, setRecording] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);

  const load = useCallback(() => {
    void api.getShortcuts().then(setShortcuts).catch((reason) => setError(String(reason)));
  }, []);

  useEffect(() => {
    load();
    const off = listen("shortcuts-changed", load);
    return () => {
      void off.then((fn) => fn());
    };
  }, [load]);

  const save = async (id: string, chord: string) => {
    setError(null);
    try {
      setShortcuts(await api.setShortcut(id, chord));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRecording(null);
    }
  };

  const onRecordKey = (id: string) => (event: KeyboardEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(null);
      return;
    }
    if (event.key === "Backspace" && !event.ctrlKey && !event.altKey && !event.metaKey) {
      void save(id, "");
      return;
    }
    const chord = chordFromEvent(event);
    if (chord) void save(id, chord);
  };

  const portal = shortcuts?.backend === "portal";
  const connectDesktop = async (feature: "shortcuts" | "edit-shortcuts") => {
    setWorking(true);
    setError(null);
    try {
      await api.configureDesktop(feature);
      load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setWorking(false);
    }
  };

  return (
    <Pane
      title="Shortcuts"
      description="Work with Bard from any app. Every shortcut puts things on your clipboard; you paste."
    >
      {error ? <Note tone="danger">{error}</Note> : null}
      {portal ? (
        <Group
          title="Your desktop"
          note="On Wayland your desktop owns global shortcuts: it asks once, and you change them in its own dialog."
        >
          <Row
            label="Global shortcuts"
            hint={
              state.desktop?.shortcutsReady
                ? "Connected. The keys below are what your desktop assigned."
                : "Not connected yet. Bard proposes the keys below; your desktop confirms them."
            }
          >
            <Button
              disabled={working}
              ariaBusy={working}
              onClick={() => void connectDesktop(state.desktop?.shortcutsReady ? "edit-shortcuts" : "shortcuts")}
            >
              {working ? "Waiting for desktop…" : state.desktop?.shortcutsReady ? "Change keys" : "Connect"}
            </Button>
          </Row>
        </Group>
      ) : null}

      <Group
        title="From any app"
        note={
          portal
            ? undefined
            : "Click a shortcut, then press the keys you want. Backspace turns it off, Escape keeps it as it is."
        }
      >
        {(shortcuts?.rows ?? []).map((row) => (
          <Row key={row.id} label={row.label} hint={undefined}>
            <span className="shortcut-cell">
              <StatusNote row={row} />
              {shortcuts?.editable ? (
                <button
                  type="button"
                  className={`shortcut-record${recording === row.id ? " recording" : ""}`}
                  aria-label={`${row.label}: ${row.display || "off"}. Press to change.`}
                  onClick={() => setRecording(recording === row.id ? null : row.id)}
                  onKeyDown={recording === row.id ? onRecordKey(row.id) : undefined}
                  onBlur={() => setRecording((current) => (current === row.id ? null : current))}
                >
                  {recording === row.id ? <span className="recording-hint">Press keys…</span> : <Keys display={row.display} />}
                </button>
              ) : (
                <Keys display={row.display} />
              )}
            </span>
          </Row>
        ))}
        {shortcuts?.editable ? (
          <div className="group-actions">
            <Button onClick={() => void api.resetShortcuts().then(setShortcuts)}>Restore defaults</Button>
          </div>
        ) : null}
      </Group>

      <Group title="In the main window">
        <Row label="Copy" hint="Enter copies and closes a docked window; Shift+Enter copies and stays.">
          <Keys display="Enter" />
        </Row>
        <Row label="Pin to slots 1–3" hint="Pinned items lead the list and answer the slot shortcuts.">
          <Keys display="Ctrl+P" />
        </Row>
        <Row label="Keep forever" hint="Kept items are never removed by the history lifespan.">
          <Keys display="Ctrl+S" />
        </Row>
        <Row label="Delete">
          <Keys display="Delete" />
        </Row>
        <Row label="Search" hint="Or just start typing.">
          <Keys display="Ctrl+F" />
        </Row>
        <Row label="Guide" hint="The shortcut guide, over the list.">
          <Keys display="?" />
        </Row>
      </Group>
    </Pane>
  );
}
