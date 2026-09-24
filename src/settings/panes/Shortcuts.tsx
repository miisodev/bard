import { useEffect, useRef, useState } from "react";
import { api, DEFAULT_KEYBINDS, setShortcutRecording } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row } from "../ui";

const BINDABLE: { key: "capture" | "panel"; label: string; hint: string }[] = [
  { key: "capture", label: "Capture", hint: "Grab the selected text from any app." },
  { key: "panel", label: "Panel", hint: "Show or hide Bard." },
];

const RECORD_SECONDS = 3;

export default function ShortcutsPane({ state }: PaneProps) {
  const [keybinds, setKeybinds] = useState<Record<string, string>>({});
  const [recording, setRecording] = useState<string | null>(null);
  const [countdown, setCountdown] = useState(RECORD_SECONDS);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    try {
      const parsed = JSON.parse(state.keybinds) as Record<string, string>;
      setKeybinds({ ...DEFAULT_KEYBINDS, ...parsed });
    } catch {
      setKeybinds({ ...DEFAULT_KEYBINDS });
    }
  }, [state.keybinds]);

  // Recording must survive this component unmounting — the global shortcut
  // layer stays suppressed until it is explicitly turned back on, and leaving
  // it suppressed would silently disable every shortcut in the app.
  useEffect(() => {
    const endRecordingOnFocusLoss = () => stopRecording();
    window.addEventListener("blur", endRecordingOnFocusLoss);
    document.addEventListener("visibilitychange", endRecordingOnFocusLoss);
    return () => {
      window.removeEventListener("blur", endRecordingOnFocusLoss);
      document.removeEventListener("visibilitychange", endRecordingOnFocusLoss);
      stopRecording();
    };
  }, []);

  function stopRecording() {
    if (timer.current) window.clearInterval(timer.current);
    timer.current = undefined;
    setShortcutRecording(false);
    void api.setShortcutRecording(false).catch((reason: unknown) => {
      setError(errorText(reason));
    });
  }

  function startRecording(key: string) {
    if (recording) stopRecording();
    setRecording(key);
    setCountdown(RECORD_SECONDS);
    setError(null);
    setShortcutRecording(true);
    void api.setShortcutRecording(true).catch((reason: unknown) => {
      setError(errorText(reason));
    });
    timer.current = window.setInterval(() => {
      setCountdown((current) => {
        if (current <= 1) {
          stopRecording();
          setRecording(null);
          return 0;
        }
        return current - 1;
      });
    }, 1000);
  }

  /** The printable name of a key, or null if it cannot anchor a shortcut. */
  function keyLabel(e: { key: string }): string | null {
    if (e.key === " ") return "Space";
    if (/^F\d{1,2}$/.test(e.key)) return e.key;
    if (e.key.length === 1) return e.key.toUpperCase();
    const named = [
      "Escape", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End",
      "PageUp", "PageDown", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
    ];
    return named.includes(e.key) ? e.key : null;
  }

  function finish(
    key: "capture" | "panel",
    e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }
  ) {
    const label = keyLabel(e);
    if (!label) return;
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.shiftKey) mods.push("Shift");
    if (e.altKey) mods.push("Alt");
    if (e.metaKey) mods.push("Super");
    // A bare letter would swallow that key everywhere on the machine; function
    // keys are the one safe exception.
    if (!mods.length && !/^F\d{1,2}$/.test(label)) return;

    const combo = `${mods.join("+")}${mods.length ? "+" : ""}${label}`;
    save({ ...keybinds, [key]: combo });
    stopRecording();
    setRecording(null);
  }

  function save(next: Record<string, string>) {
    const previous = keybinds;
    setKeybinds(next);
    setError(null);
    void api.setKeybinds(JSON.stringify(next)).catch((reason: unknown) => {
      setKeybinds(previous);
      setError(errorText(reason));
    });
  }

  return (
    <Pane title="Shortcuts" description="Two taps of a modifier, in any app. Shift captures, Ctrl dictates.">
      <Group title="Gestures">
        <Row label="Capture" hint="Tap Left Shift twice to capture the current selection.">
          <Keycaps keys={["Left Shift", "Left Shift"]} />
        </Row>
        <Row label="Panel" hint="Tap Right Shift twice to show or hide Bard.">
          <Keycaps keys={["Right Shift", "Right Shift"]} />
        </Row>
        <Row
          label="Dictate"
          hint="Hold the same Ctrl key to talk, or tap it twice to start and twice to stop. Esc cancels."
        >
          <Keycaps keys={["Ctrl", "Ctrl"]} />
        </Row>
      </Group>

      <Group title="Chords" note="Optional. A chord takes its key combination away from every other app.">
        {BINDABLE.map(({ key, label, hint }) => {
          const combo = (keybinds[key] ?? "").trim();
          return (
            <Row key={key} label={label} hint={hint}>
              <div className="chord-control">
                <button
                  type="button"
                  className={`keys${recording === key ? " recording" : ""}${combo ? "" : " unset"}`}
                  aria-label={combo ? `${label}: ${combo}. Record a new chord` : `Record a chord for ${label}`}
                  onClick={(e) => {
                    (e.currentTarget as HTMLButtonElement).focus();
                    startRecording(key);
                  }}
                  onKeyDown={(e) => {
                    if (recording !== key) return;
                    e.preventDefault();
                    e.stopPropagation();
                    finish(key, e);
                  }}
                >
                  {recording === key ? (
                    `Press keys · ${countdown}`
                  ) : combo ? (
                    combo.split("+").map((part, index) => <kbd key={`${part}-${index}`}>{part}</kbd>)
                  ) : (
                    "Record"
                  )}
                </button>
                {combo && recording !== key ? (
                  <button
                    type="button"
                    className="chord-clear"
                    aria-label={`Clear the ${label.toLowerCase()} chord`}
                    title="Clear"
                    onClick={() => save({ ...keybinds, [key]: "" })}
                  >
                    ×
                  </button>
                ) : null}
              </div>
            </Row>
          );
        })}
      </Group>

      <Group>
        <Row label="Reset" hint="Clears every chord. The gestures always stay.">
          <Button onClick={() => save({ ...DEFAULT_KEYBINDS })}>Reset</Button>
        </Row>
      </Group>

      {error ? <Note tone="danger">{error}</Note> : null}
      <Note>
        Click Record and press the combination within {RECORD_SECONDS} seconds. A chord needs at
        least one modifier, or a function key.
      </Note>
    </Pane>
  );
}

function Keycaps({ keys }: { keys: string[] }) {
  return (
    <span className="keys static" aria-label={keys.join(" then ")}>
      {keys.map((key, index) => (
        <kbd key={`${key}-${index}`}>{key}</kbd>
      ))}
    </span>
  );
}

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "The shortcut could not be registered.";
}
