import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../store";
import { CANVAS, isHexColor, resolvedTheme, applyAppearance } from "../../appearance";
import type { PaneProps } from "../SettingsWindow";
import type { FeedbackChannels } from "../../types";
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
      applyAppearance({ ...state, drawBackground: "theme", drawBackgroundColor: "" });
      refresh();
    } finally { busy.current = false; setSaving(false); }
  }, [refresh, state]);

  useEffect(() => setColor(current), [current]);

  const [feedback, setFeedback] = useState<FeedbackChannels>({ pill: true, sound: true });
  const [feedbackSaving, setFeedbackSaving] = useState(false);
  useEffect(() => {
    void api.getFeedback().then(setFeedback).catch(() => undefined);
  }, []);
  const changeFeedback = async (next: FeedbackChannels) => {
    setFeedback(next);
    setFeedbackSaving(true);
    try {
      setFeedback(await api.setFeedback(next.pill, next.sound));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setFeedbackSaving(false);
    }
  };

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
    <Pane title="General" description="How Bard looks, where it sits and how it tells you things.">
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

      <Group title="Main window">
        <Row label="Position" hint="Float it anywhere, or dock it to the side of the screen. Docked, it tucks away when you click elsewhere.">
          <Segmented
            label="Main window position"
            value={state.dock}
            disabled={saving}
            onChange={(dock) => void save(() => api.setDock(dock))}
            options={[
              { value: "float", label: "Float" },
              { value: "left", label: "Left" },
              { value: "right", label: "Right" },
            ]}
          />
        </Row>
      </Group>

      <Group
        title="Feedback"
        note="Every copy, shortcut and error is told to you this way. A live microphone always shows its pill."
      >
        <Row label="Status pill" hint="A small pill at the top middle of your screen.">
          <Toggle
            label="Status pill"
            checked={feedback.pill}
            disabled={feedbackSaving}
            onChange={(pill) => void changeFeedback({ ...feedback, pill })}
          />
        </Row>
        <Row label="Sounds" hint="Soft pops and clicks.">
          <Toggle
            label="Sounds"
            checked={feedback.sound}
            disabled={feedbackSaving}
            onChange={(sound) => void changeFeedback({ ...feedback, sound })}
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
