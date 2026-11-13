import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../../store";
import type { ShortcutConflict, ShortcutState } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row } from "../ui";

/** A stored chord (`Alt+Shift+Comma`) the way it is read (`Alt+Shift+,`). */
function shown(chord: string): string {
  const signs: Record<string, string> = {
    Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[",
    BracketRight: "]", Backslash: "\\", Backquote: "`", Minus: "-", Equal: "=",
  };
  return chord.split("+").map((key) => signs[key] ?? key).join("+");
}

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

/*
  The shortcuts are edited as one table and saved as one table.

  Each change is a draft until Save. Save first asks whether the table can
  stand: two actions on one chord, a key Bard cannot use, or a chord the
  desktop or another app already holds. Anything in the way is marked on its
  row and nothing is written, so the person fixes the table once rather than
  finding one dead shortcut at a time. It is the same on Windows, X11 and
  Wayland; on Wayland the desktop then shows the new keys once to confirm.
*/
export default function ShortcutsPane({ state }: PaneProps) {
  const [shortcuts, setShortcuts] = useState<ShortcutState | null>(null);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [conflicts, setConflicts] = useState<ShortcutConflict[]>([]);
  const [recording, setRecording] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const checking = useRef(0);

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

  const rows = shortcuts?.rows ?? [];
  const portal = shortcuts?.backend === "portal";
  const connected = !portal || Boolean(state.desktop?.shortcutsReady);
  const changed = useMemo(
    () => Object.fromEntries(Object.entries(draft).filter(([id, chord]) => rows.find((row) => row.id === id)?.chord !== chord)),
    [draft, rows],
  );
  const dirty = Object.keys(changed).length > 0;

  // Mark what stands in the way as the table is edited, not only at Save.
  useEffect(() => {
    if (!dirty) {
      setConflicts([]);
      return;
    }
    const ticket = ++checking.current;
    void api
      .checkShortcuts(changed)
      .then((found) => {
        if (ticket === checking.current) setConflicts(found);
      })
      .catch(() => undefined);
  }, [changed, dirty]);

  const stage = (id: string, chord: string) => {
    setSaved(false);
    setError(null);
    setDraft((current) => ({ ...current, [id]: chord }));
    setRecording(null);
  };

  const onRecordKey = (id: string) => (event: KeyboardEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(null);
      return;
    }
    if (event.key === "Backspace" && !event.ctrlKey && !event.altKey && !event.metaKey) {
      stage(id, "");
      return;
    }
    const chord = chordFromEvent(event);
    if (chord) stage(id, chord);
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const outcome = await api.saveShortcuts(changed);
      setShortcuts(outcome.state);
      setConflicts(outcome.conflicts);
      if (outcome.conflicts.length === 0) {
        setDraft({});
        setSaved(true);
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  };

  const restoreDefaults = () => {
    setSaved(false);
    setDraft(Object.fromEntries(rows.map((row) => [row.id, row.defaultChord])));
  };

  const connect = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.configureDesktop("shortcuts");
      load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setSaving(false);
    }
  };

  const atDefaults = rows.every((row) => (draft[row.id] ?? row.chord) === row.defaultChord);

  return (
    <Pane title="Shortcuts">
      {error ? <Note tone="danger">{error}</Note> : null}
      {!connected ? (
        <Group>
          <Row label="Your desktop has not been asked yet" hint="It confirms these keys once.">
            <Button variant="primary" disabled={saving} ariaBusy={saving} onClick={() => void connect()}>
              {saving ? "Waiting…" : "Connect"}
            </Button>
          </Row>
        </Group>
      ) : null}

      <Group>
        {rows.map((row) => {
          const staged = row.id in changed;
          const chord = draft[row.id] ?? row.chord;
          // An unedited row shows what is really bound: on Wayland that is the
          // desktop's answer, which may not be what Bard asked for.
          const display = staged ? shown(chord) : row.display || shown(chord);
          const conflict = conflicts.find((found) => found.id === row.id);
          const problem = conflict
            ? conflict.kind === "duplicate"
              ? `Same as “${conflict.with}”`
              : conflict.kind === "system"
                ? `Used by ${conflict.with}`
                : conflict.with
            : !staged && row.conflict
              ? `Used by your desktop’s “${row.conflict}”`
              : !staged && row.status === "taken"
                ? "Used by another app"
                : null;
          return (
            <Row key={row.id} label={row.label}>
              <span className="shortcut-cell">
                {problem ? <span className="shortcut-status taken">{problem}</span> : null}
                <button
                  type="button"
                  className={`shortcut-record${recording === row.id ? " recording" : ""}${staged ? " staged" : ""}${problem ? " clash" : ""}`}
                  aria-label={`${row.label}: ${display || "off"}${problem ? `. ${problem}` : ""}. Press to change.`}
                  aria-invalid={problem ? true : undefined}
                  onClick={() => setRecording(recording === row.id ? null : row.id)}
                  onKeyDown={recording === row.id ? onRecordKey(row.id) : undefined}
                  onBlur={() => setRecording((current) => (current === row.id ? null : current))}
                >
                  {recording === row.id ? <span className="recording-hint">Press keys…</span> : <Keys display={display} />}
                </button>
              </span>
            </Row>
          );
        })}
      </Group>

      <div className="shortcut-bar">
        <Button disabled={saving || atDefaults} onClick={restoreDefaults}>
          Restore defaults
        </Button>
        <span className="shortcut-bar-note" role="status">
          {conflicts.length > 0
            ? conflicts.length === 1
              ? "Fix the marked shortcut to save."
              : `Fix the ${conflicts.length} marked shortcuts to save.`
            : saved
              ? "Saved."
              : dirty && portal
                ? "Your desktop will ask you to confirm."
                : recording
                  ? "Backspace turns it off. Escape cancels."
                  : ""}
        </span>
        {dirty ? (
          <Button disabled={saving} onClick={() => { setDraft({}); setConflicts([]); }}>
            Discard
          </Button>
        ) : null}
        <Button
          variant="primary"
          disabled={!dirty || saving || conflicts.length > 0}
          ariaBusy={saving}
          onClick={() => void save()}
        >
          {saving ? "Saving…" : "Save"}
        </Button>
      </div>
    </Pane>
  );
}
