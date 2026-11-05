import { useEffect, useState } from "react";
import { api } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Group, Pane, Row, Toggle } from "../ui";

export default function CapturePane({ state, refresh }: PaneProps) {
  const [soundOnCapture, setSoundOnCapture] = useState(true);

  useEffect(() => {
    void api.getSoundOnCapture().then(setSoundOnCapture);
  }, []);

  return (
    <Pane title="Capture" description="Grabbing text and images from anywhere on the machine.">
      {state.desktop?.session === "wayland" ? <Group title="Shortcut"><Row label="Desktop shortcuts" hint="Set up global capture and keyboard access in Settings › Shortcuts. Your desktop manages these permissions." /></Group> : <Group title="Shortcut">
        <Row
          label="Double-Shift capture"
          hint="Tap Shift twice to grab the current selection without disturbing your clipboard."
        >
          <Toggle
            label="Double-Shift capture"
            checked={state.captureDoubleShift}
            onChange={(next) => {
              void api.setCaptureDoubleShift(next);
              refresh();
            }}
          />
        </Row>
      </Group>}

      <Group title="Feedback">
        <Row label="Capture sound" hint="A soft pop when something is captured.">
          <Toggle
            label="Capture sound"
            checked={soundOnCapture}
            onChange={(next) => {
              setSoundOnCapture(next);
              void api.setSoundOnCapture(next);
            }}
          />
        </Row>
      </Group>
    </Pane>
  );
}
