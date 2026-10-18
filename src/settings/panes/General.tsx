import { useEffect, useRef, useState } from "react";
import { api } from "../../store";
import { CANVAS, isHexColor, resolvedTheme, applyAppearance } from "../../appearance";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Pane, Row, Segmented, Toggle } from "../ui";

export default function GeneralPane({ state, refresh }: PaneProps) {
  const custom = state.background === "custom" && isHexColor(state.backgroundColor);
  const current = custom ? state.backgroundColor : CANVAS[resolvedTheme(state.theme)];
  const [color, setColor] = useState(current);
  const well = useRef<HTMLInputElement>(null);

  useEffect(() => setColor(current), [current]);

  // The picker previews live in this window as it moves, and is saved — and
  // sent to every other window — once, when the picker closes.
  useEffect(() => {
    const input = well.current;
    if (!input) return;
    const commit = () => void api.setBackground("custom", input.value).then(refresh);
    input.addEventListener("change", commit);
    return () => input.removeEventListener("change", commit);
  }, [refresh]);

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
        <Row
          label="Background"
          hint={custom ? "Your colour. Cards keep the theme." : "Follows the theme until you pick a colour."}
        >
          <div className="background-control">
            {custom ? (
              <Button onClick={() => void api.setBackground("theme").then(refresh)}>Match theme</Button>
            ) : null}
            <input
              ref={well}
              className="color-well"
              type="color"
              aria-label="Background colour"
              title="Pick a colour"
              value={color}
              onChange={(event) => {
                setColor(event.target.value);
                applyAppearance({
                  theme: state.theme,
                  background: "custom",
                  backgroundColor: event.target.value,
                  drawBackground: "theme",
                  drawBackgroundColor: "",
                });
              }}
            />
          </div>
        </Row>
      </Group>

      <Group title="Shelf">
        <Row
          label="Dock edge"
          hint="Where the shelf sits. Bottom keeps it under your app; the window follows the edge you pick."
        >
          <Segmented
            label="Dock edge"
            value={state.shelfEdge ?? "bottom"}
            onChange={(edge) => void api.setShelfEdge(edge).then(refresh)}
            options={[
              { value: "bottom", label: "Bottom" },
              { value: "top", label: "Top" },
              { value: "left", label: "Left" },
              { value: "right", label: "Right" },
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
