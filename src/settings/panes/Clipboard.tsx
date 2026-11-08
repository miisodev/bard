import { api } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Group, Note, Pane, Row, Toggle } from "../ui";

export default function ClipboardPane({ state, refresh }: PaneProps) {
  return (
    <Pane
      title="Clipboard"
      description="While Bard runs, everything you copy or cut is kept here, newest first."
    >
      <Group title="Capture">
        <Row
          label="Keep what I copy"
          hint={
            state.capturePaused
              ? "Paused. Nothing you copy is kept until you turn this back on."
              : "Text, links, images and files you copy are kept in Bard's history."
          }
        >
          <Toggle
            label="Keep what I copy"
            checked={!state.capturePaused}
            onChange={(on) => void api.setCapturePaused(!on).then(refresh)}
          />
        </Row>
        <Row
          label="Copying from Bard"
          hint="Selecting an item, a slot shortcut or a finished dictation puts it on your clipboard. Paste it yourself with Ctrl+V."
        />
      </Group>

      <Group title="What Bard leaves alone">
        <Row
          label="Passwords"
          hint="Copies that a password manager marks as secret are never kept."
        />
        <Row
          label="Your files"
          hint="A copied file is kept as a reference to where it is. Bard never moves, copies or deletes it."
        />
      </Group>

      <Note>
        History stays on this PC. Choose how long it is kept, export it or clear it in Settings › Data.
      </Note>
    </Pane>
  );
}
