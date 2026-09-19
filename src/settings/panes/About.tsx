import type { PaneProps } from "../SettingsWindow";
import { Group, Note, Pane, Row } from "../ui";

export default function AboutPane({ state }: PaneProps) {
  return (
    <Pane title="About">
      <Group>
        <Row label="bard" hint={`Version ${state.appVersion}`} />
        <Row label="Capture. Dictate. Keep." hint="A native Windows workspace for the AI era." />
      </Group>

      <Group title="Credits">
        <Row
          label="Copper"
          hint="bard's capture panel follows the design of Copper by shadcn. bard is an independent product and is not affiliated with or endorsed by shadcn."
        />
        <Row
          label="Handy"
          hint="bard's dictation follows the shape of Handy by CJ Pais, used under the MIT licence."
        />
        <Row
          label="cooper"
          hint="Consulted as a reference implementation. Apache-2.0, TouchMyBar."
        />
      </Group>

      <Note>
        Everything bard transcribes stays on this machine. There is no account, no
        sync and no telemetry.
      </Note>
    </Pane>
  );
}
