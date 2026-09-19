import type { PaneProps } from "../SettingsWindow";
import { Group, Pane, Row } from "../ui";

export default function AboutPane({ state }: PaneProps) {
  return (
    <Pane title="About">
      <Group>
        <Row label="bard" hint="Capture. Dictate. Keep." />
        <Row label="Version" hint={state.appVersion} />
      </Group>

      <Group title="Privacy">
        <Row
          label="Everything stays on this machine"
          hint="Transcription runs on a local model. There is no account, no sync and no telemetry — your captures and recordings never leave the device."
        />
      </Group>
    </Pane>
  );
}
