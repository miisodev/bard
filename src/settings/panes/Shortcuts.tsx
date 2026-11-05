import { useState } from "react";
import { Group, Pane, Row, Button } from "../ui";
import type { PaneProps } from "../SettingsWindow";
import { api } from "../../store";

export default function ShortcutsPane({ state, refresh }: PaneProps) {
  const wayland = state.desktop?.session === "wayland";
  const [working, setWorking] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const configure = async (feature: "shortcuts" | "input") => {
    setWorking(feature);
    setError(null);
    try { await api.configureDesktop(feature); refresh(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setWorking(null); }
  };
  return (
    <Pane
      title="Shortcuts"
      description={wayland ? "Your desktop controls global shortcuts and permission to type into other apps." : "A few taps of keys you already rest on. Available from any app."}
    >
      {wayland ? (
        <Group title="Desktop integration" note="Wayland protects other apps from global input hooks. Enable each integration through your desktop’s permission dialog.">
          <Row label="Global shortcuts" hint={state.desktop?.shortcutsReady ? "Connected. Your desktop owns the assigned shortcuts." : "Choose shortcuts for the shelf, capture, dictation, Draw and Settings. Double-tap modifier gestures are Windows and X11 shortcuts."}>
            <Button disabled={!!working || state.desktop?.shortcutsReady} ariaBusy={working === "shortcuts"} onClick={() => void configure("shortcuts")}>
              {working === "shortcuts" ? "Waiting for desktop…" : state.desktop?.shortcutsReady ? "Connected" : "Set up shortcuts"}
            </Button>
          </Row>
          <Row label="Capture and paste-back" hint={state.desktop?.inputReady ? "Keyboard access is enabled for this session. You can revoke it from your desktop." : "Allow Bard to send Copy and Paste to your focused app. Only keyboard access is requested; no screen or pointer is shared. Without it, copy items and paste them yourself."}>
            <Button disabled={!!working || state.desktop?.inputReady} ariaBusy={working === "input"} onClick={() => void configure("input")}>
              {working === "input" ? "Waiting for desktop…" : state.desktop?.inputReady ? "Connected" : "Enable keyboard access"}
            </Button>
          </Row>
          {error ? <div className="row"><p className="form-error" role="alert">{error}</p></div> : null}
        </Group>
      ) : <Group title="From any app">
        <Row label="Capture" hint="Tap Left Shift twice to grab the current selection.">
          <Keycaps keys={["Left Shift"]} twice />
        </Row>
        <Row
          label="Paste item"
          hint="Tap Right Shift twice, then quickly press 1, 2 or 3 to paste one of the first three items into the focused app."
        >
          <Keycaps keys={["Right Shift"]} twice then="1 2 3" />
        </Row>
        <Row label="Shelf" hint="Tap Right Shift twice to show or hide Bard.">
          <Keycaps keys={["Right Shift"]} twice />
        </Row>
        <Row label="Settings" hint="Tap Right Alt twice to open Settings as a single window.">
          <Keycaps keys={["Right Alt"]} twice />
        </Row>
        <Row label="Draw" hint="Tap Left Alt twice to open Draw as a single window.">
          <Keycaps keys={["Left Alt"]} twice />
        </Row>
        <Row label="Fullscreen Draw" hint="While Draw is focused, toggle its native fullscreen state.">
          <Keycaps keys={["F11"]} />
        </Row>
        <Row
          label="Dictate"
          hint="Hold the same Ctrl key to talk, or tap it twice to start and twice to stop. Esc cancels."
        >
          <Keycaps keys={["Ctrl"]} twice />
        </Row>
      </Group>}

      <Group title="Inside the shelf">
        <Row label="Paste" hint="Paste the selected item into the app you opened Bard over.">
          <Keycaps keys={["Enter"]} />
        </Row>
        <Row
          label="Paste by position"
          hint="Paste a card by where it sits on the page. The digits reach as many cards as the page holds."
        >
          <Keycaps keys={["Ctrl", "1–9"]} />
        </Row>
        <Row label="Preview" hint="Read the full capture before copying or pasting it.">
          <Keycaps keys={["Alt", "Enter"]} />
        </Row>
        <Row label="Copy" hint="Copy the selected item without leaving Bard.">
          <Keycaps keys={["Ctrl", "C"]} />
        </Row>
        <Row label="Copy as list" hint="Copy the selected items as a Markdown list.">
          <Keycaps keys={["Ctrl", "Shift", "M"]} />
        </Row>
        <Row label="Sections" hint="Open the section switcher.">
          <Keycaps keys={["Ctrl", "K"]} />
        </Row>
        <Row label="Shortcut guide" hint="Show this list without leaving the shelf.">
          <Keycaps keys={["Ctrl", "/"]} />
        </Row>
        <Row label="Done" hint="Mark the selected item complete.">
          <Keycaps keys={["Space"]} />
        </Row>
        <Row label="Close" hint="Clear search, then selection, then close the shelf.">
          <Keycaps keys={["Esc"]} />
        </Row>
      </Group>
    </Pane>
  );
}

/**
 * A chord, or a key tapped twice. A doubled gesture reads as one keycap and
 * "×2" rather than the same cap drawn twice, which looked like a two-key chord.
 */
function Keycaps({ keys, twice = false, then }: { keys: string[]; twice?: boolean; then?: string }) {
  const spoken = `${keys.join(" plus ")}${twice ? ", tapped twice" : ""}${then ? `, then ${then.split(" ").join(" or ")}` : ""}`;
  return (
    <span className="keys static" aria-label={spoken}>
      {keys.map((key, index) => (
        <kbd key={`${key}-${index}`}>{key}</kbd>
      ))}
      {twice ? <span className="keys-times" aria-hidden="true">×2</span> : null}
      {then ? (
        <>
          <span className="keys-then" aria-hidden="true">then</span>
          {then.split(" ").map((key) => (
            <kbd key={key} aria-hidden="true">{key}</kbd>
          ))}
        </>
      ) : null}
    </span>
  );
}
