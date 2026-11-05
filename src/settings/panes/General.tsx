import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../store";
import { CANVAS, isHexColor, resolvedTheme, applyAppearance } from "../../appearance";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row, Segmented, Toggle } from "../ui";

export default function GeneralPane({ state, refresh }: PaneProps) {
  const custom = state.background === "custom" && isHexColor(state.backgroundColor);
  const current = custom ? state.backgroundColor : CANVAS[resolvedTheme(state.theme)];
  const [color, setColor] = useState(current);
  const well = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const busy = useRef(false);
  const save = useCallback(async (action: () => Promise<unknown>) => {
    if (busy.current) return;
    busy.current = true;
    setSaving(true);
    setError(null);
    try { await action(); refresh(); }
    catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      // A rejected colour save must also undo its live preview.
      applyAppearance(state);
      refresh();
    } finally { busy.current = false; setSaving(false); }
  }, [refresh, state]);

  useEffect(() => setColor(current), [current]);

  // The picker previews live in this window as it moves, and is saved — and
  // sent to every other window — once, when the picker closes.
  useEffect(() => {
    const input = well.current;
    if (!input) return;
    const commit = () => void save(() => api.setBackground("custom", input.value));
    input.addEventListener("change", commit);
    return () => input.removeEventListener("change", commit);
  }, [save]);

  return (
    <Pane title="General" description="Appearance and how Bard fits into your day.">
      {error ? <Note tone="danger">{error}</Note> : null}
      <Group title="Appearance">
        <Row label="Theme" hint="System follows your desktop appearance.">
          <Segmented
            label="Theme"
            value={state.theme}
            disabled={saving}
            onChange={(t) => void save(() => api.setTheme(t))}
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
              <Button disabled={saving} onClick={() => void save(() => api.setBackground("theme"))}>Match theme</Button>
            ) : null}
            <input
              ref={well}
              className="color-well"
              type="color"
              disabled={saving}
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
          hint="Where the shelf appears on your screen."
        >
          <Segmented
            label="Dock edge"
            value={state.shelfEdge ?? "bottom"}
            disabled={saving}
            onChange={(edge) => void save(() => api.setShelfEdge(edge))}
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
            <Button disabled={saving} onClick={() => void save(() => api.openStartupSettings())}>Open</Button>
          </Row>
        ) : (
          <Row label="Start at login" hint="Runs quietly in the tray.">
            <Toggle
              label="Start at login"
              checked={state.autostartEnabled}
              disabled={saving}
              onChange={(next) => void save(() => api.setAutostart(next))}
            />
          </Row>
        )}
      </Group>
    </Pane>
  );
}
