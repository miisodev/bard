import { Group, Pane, Row } from "../ui";

export default function ShortcutsPane() {
  return (
    <Pane
      title="Shortcuts"
      description="Fixed gestures and panel keys. These are intentionally not customizable."
    >
      <Group title="From any app">
        <Row label="Capture" hint="Tap Left Shift twice to grab the current selection.">
          <Keycaps keys={["Left Shift", "Left Shift"]} />
        </Row>
        <Row
          label="Paste item"
          hint="Tap Right Shift twice, then quickly press 1, 2, or 3 to paste that timeline item into the focused app."
        >
          <Keycaps keys={["Right Shift", "Right Shift", "1 / 2 / 3"]} />
        </Row>
        <Row label="Panel" hint="Tap Right Shift twice to show or hide Bard.">
          <Keycaps keys={["Right Shift", "Right Shift"]} />
        </Row>
        <Row label="Settings" hint="Tap Right Alt twice to open Settings as a single window.">
          <Keycaps keys={["Right Alt", "Right Alt"]} />
        </Row>
        <Row label="Draw" hint="Tap Left Alt twice to open Draw as a single window.">
          <Keycaps keys={["Left Alt", "Left Alt"]} />
        </Row>
        <Row label="Fullscreen Draw" hint="While Draw is focused, toggle its native fullscreen state.">
          <Keycaps keys={["F11"]} />
        </Row>
        <Row
          label="Dictate"
          hint="Hold the same Ctrl key to talk, or tap it twice to start and twice to stop. Esc cancels."
        >
          <Keycaps keys={["Ctrl", "Ctrl"]} />
        </Row>
      </Group>

      <Group title="Inside the panel">
        <Row label="Copy" hint="Copy the selected item, or the active item.">
          <Keycaps keys={["Enter"]} />
        </Row>
        <Row label="Copy as list" hint="Copy the selected items as a Markdown list.">
          <Keycaps keys={["Ctrl", "Shift", "M"]} />
        </Row>
        <Row label="Sections" hint="Open the section switcher.">
          <Keycaps keys={["Ctrl", "K"]} />
        </Row>
        <Row label="Shortcut guide" hint="Show this list without leaving the panel.">
          <Keycaps keys={["Ctrl", "/"]} />
        </Row>
        <Row label="Done" hint="Mark the selected item complete.">
          <Keycaps keys={["Space"]} />
        </Row>
        <Row label="Close" hint="Clear search, then selection, then close the panel.">
          <Keycaps keys={["Esc"]} />
        </Row>
      </Group>
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
