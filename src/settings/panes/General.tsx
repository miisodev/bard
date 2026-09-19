import { api } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Group, Pane, Row, Segmented, Toggle } from "../ui";

export default function GeneralPane({ state, refresh }: PaneProps) {
  return (
    <Pane title="General" description="How bard behaves when you are not looking at it.">
      <Group title="Appearance">
        <Row
          label="Theme"
          hint="Glass follows your system's light or dark setting and lets the desktop through."
        >
          <Segmented
            label="Theme"
            value={state.theme}
            onChange={(t) => void api.setTheme(t).then(refresh)}
            options={[
              { value: "system", label: "System" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
              { value: "glass", label: "Glass" },
            ]}
          />
        </Row>
      </Group>

      <Group title="Startup">
        <Row label="Start at login" hint="Runs quietly in the tray.">
          <Toggle
            label="Start at login"
            checked={state.autostartEnabled}
            onChange={(next) => void api.setAutostart(next).then(refresh)}
          />
        </Row>
      </Group>
    </Pane>
  );
}
