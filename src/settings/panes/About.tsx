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
          label="Local by default"
          hint="Audio capture, speech recognition, captures, and history stay on this computer. Bard has no account, sync, or telemetry."
        />
        <Row
          label="Optional cloud post-processing"
          hint="If you explicitly enable Post-processing, Bard sends only the finished transcript text to OpenRouter. Audio, captures, history, and unrelated items are not sent. Bard falls back to the untouched local transcript whenever cloud processing is off or fails."
        />
      </Group>
    </Pane>
  );
}
