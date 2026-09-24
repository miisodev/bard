import { api } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Pane, Row, Segmented, Toggle } from "../ui";

export default function GeneralPane({ state, refresh }: PaneProps) {
  return (
    <Pane title="General" description="How Bard behaves when you are not looking at it.">
      <Group title="Appearance">
        <Row label="Theme" hint="System follows your Windows appearance setting.">
          <Segmented
            label="Theme"
            value={state.theme}
            onChange={(t) => void api.setTheme(t).then(refresh)}
            options={[
              { value: "system", label: "System" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
          />
        </Row>
      </Group>

      <Group title="Startup">
        {state.channel === "store" ? (
          // The Store copy starts at login through its declared startup task,
          // which Windows owns and switches in Settings › Apps › Startup.
          <Row label="Start at login" hint="Windows manages this for the Microsoft Store edition.">
            <Button onClick={() => void api.openStartupSettings()}>Open</Button>
          </Row>
        ) : (
          <Row label="Start at login" hint="Runs quietly in the tray.">
            <Toggle
              label="Start at login"
              checked={state.autostartEnabled}
              onChange={(next) => void api.setAutostart(next).then(refresh)}
            />
          </Row>
        )}
      </Group>
    </Pane>
  );
}
