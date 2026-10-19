import { Group, Pane, Row } from "../ui";

export default function ShortcutsPane() {
  return (
    <Pane
      title="Shortcuts"
      description="Bard is built around a few taps of keys you already rest on. They work in any app."
    >
      <Group title="From any app">
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
      </Group>

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
