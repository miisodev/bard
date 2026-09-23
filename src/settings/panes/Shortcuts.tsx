import { useEffect, useRef, useState } from "react";
import { api, DEFAULT_KEYBINDS, setShortcutRecording } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row } from "../ui";

const BINDABLE: { key: "capture" | "panel"; label: string; hint: string }[] = [
  { key: "capture", label: "Capture selection", hint: "Grab the selected text from any app." },
  { key: "panel", label: "Toggle panel", hint: "Show or hide Bard." },
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
    const previous = keybinds;
    const next = { ...keybinds, [key]: combo };
    setKeybinds(next);
    setError(null);
    void api.setKeybinds(JSON.stringify(next)).catch((reason: unknown) => {
      setKeybinds(previous);
      setError(errorText(reason));
    });
    stopRecording();
    setRecording(null);
  }

  return (
    <Pane title="Shortcuts" description="Global keys, active whatever you are using.">
      <Group title="Rebindable">
        {BINDABLE.map(({ key, label, hint }) => (
          <Row key={key} label={label} hint={hint}>
            <button
              type="button"
              className={`keys${recording === key ? " recording" : ""}`}
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
              {recording === key
                ? `Press keys… (${countdown})`
                : (keybinds[key] ?? DEFAULT_KEYBINDS[key] ?? "—")}
            </button>
          </Row>
        ))}
      </Group>

      <Group title="Fixed gestures">
        <Row label="Left Shift × 2" hint="Capture the current selection." />
        <Row label="Right Shift × 2" hint="Show or hide the panel." />
        <Row
          label="Ctrl × 2"
          hint="Tap the same Control key twice to start dictation. Hold it for push-to-talk, or use a short tap and a second Ctrl+Ctrl to stop. Escape cancels."
        />
      </Group>

      <Group>
        <Row label="Reset shortcuts" hint="Puts every binding back to its default.">
          <Button
            onClick={() => {
              const next = { ...DEFAULT_KEYBINDS };
              setKeybinds(next);
              setError(null);
              void api.setKeybinds(JSON.stringify(next)).catch((reason: unknown) => {
                setError(errorText(reason));
              });
            }}
          >
            Defaults
          </Button>
        </Row>
      </Group>

      {error ? <Note tone="danger">{error}</Note> : null}
      <Note>
        Click a shortcut and press the combination within {RECORD_SECONDS} seconds. A
        shortcut needs at least one modifier, or a function key. Double-tap gestures are
        deliberately fixed so they remain available in every application.
      </Note>
    </Pane>
  );
}

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "The shortcut could not be registered.";
}
