import { useEffect, useRef, useState } from "react";
import { api, DEFAULT_KEYBINDS, setShortcutRecording } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row } from "../ui";

const BINDABLE: { key: string; label: string; hint: string }[] = [
  { key: "capture", label: "Capture selection", hint: "Grab the selected text from any app." },
  { key: "panel", label: "Toggle panel", hint: "Show or hide bard." },
  { key: "dictate", label: "Dictate", hint: "Hold to record, release to transcribe." },
];

const RECORD_SECONDS = 3;

export default function ShortcutsPane({ state }: PaneProps) {
  const [keybinds, setKeybinds] = useState<Record<string, string>>({});
  const [recording, setRecording] = useState<string | null>(null);
  const [countdown, setCountdown] = useState(RECORD_SECONDS);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    try {
      setKeybinds(JSON.parse(state.keybinds) as Record<string, string>);
    } catch {
      setKeybinds({});
    }
  }, [state.keybinds]);

  // Recording must survive this component unmounting — the global shortcut
  // layer stays suppressed until it is explicitly turned back on, and leaving
  // it suppressed would silently disable every shortcut in the app.
  useEffect(() => stopRecording, []);

  function stopRecording() {
    if (timer.current) window.clearInterval(timer.current);
    timer.current = undefined;
    setShortcutRecording(false);
  }

  function startRecording(key: string) {
    if (recording) stopRecording();
    setRecording(key);
    setCountdown(RECORD_SECONDS);
    setShortcutRecording(true);
    timer.current = window.setInterval(() => {
      setCountdown((c) => {
        if (c <= 1) {
          stopRecording();
          setRecording(null);
          return 0;
        }
        return c - 1;
      });
    }, 1000);
  }

  /** The printable name of a key, or null if it cannot anchor a shortcut. */
  function keyLabel(e: { key: string }): string | null {
    if (e.key === " ") return "Space";
    if (/^F\d{1,2}$/.test(e.key)) return e.key;
    if (e.key.length === 1) return e.key.toUpperCase();
    const named = [
      "Escape", "Enter", "Tab", "Backspace", "Delete",
      "Insert", "Home", "End", "PageUp", "PageDown",
    ];
    return named.includes(e.key) ? e.key : null;
  }

  function finish(
    key: string,
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
    setKeybinds((kb) => {
      const next = { ...kb, [key]: combo };
      void api.setKeybinds(JSON.stringify(next));
      return next;
    });
    stopRecording();
    setRecording(null);
  }

  return (
    <Pane title="Shortcuts" description="Global keys, active whatever you are using.">
      <Group>
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

      <Group title="Fixed">
        <Row label="Left Shift, Left Shift" hint="Capture the current selection." />
        <Row label="Right Shift, Right Shift" hint="Show or hide the panel." />
      </Group>

      <Group>
        <Row label="Reset shortcuts" hint="Puts every binding back to its default.">
          <Button
            onClick={() => {
              setKeybinds({ ...DEFAULT_KEYBINDS });
              void api.setKeybinds(JSON.stringify(DEFAULT_KEYBINDS));
            }}
          >
            Defaults
          </Button>
        </Row>
      </Group>

      <Note>
        Click a shortcut and press the combination within {RECORD_SECONDS} seconds. A
        shortcut needs at least one modifier, or a function key.
      </Note>
    </Pane>
  );
}
