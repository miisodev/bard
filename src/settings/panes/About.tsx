import { useEffect, useRef, useState } from "react";
import { RefreshCw, Download } from "lucide-react";
import type { PaneProps } from "../SettingsWindow";
import LicenseCard from "../../components/LicenseCard";
import { api } from "../../store";
import { Button, Group, Pane, Row } from "../ui";

export default function AboutPane({ state, refresh }: PaneProps) {
  const [checking, setChecking] = useState(false);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<{ version: string; notes: string | null; publishedAt: string | null } | null>(null);
  const [installing, setInstalling] = useState(false);
  const store = state.channel === "store";
  const confirm = useRef<HTMLDivElement>(null);

  const checkForUpdate = async () => {
    if (checking) return;
    setChecking(true);
    setUpdateMessage(null);
    setAvailableUpdate(null);
    try {
      const update = await api.checkForUpdate();
      setAvailableUpdate(update);
      setUpdateMessage(update ? `Bard ${update.version} is available.` : "Bard is up to date.");
    } catch (error) {
      setUpdateMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setChecking(false);
    }
  };

  const installUpdate = async () => {
    if (!availableUpdate || installing) return;
    setInstalling(true);
    try {
      await api.installUpdate();
      setUpdateMessage("Installing Bard. The app will close and reopen when the update is ready.");
    } catch (error) {
      setUpdateMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setInstalling(false);
    }
  };

  /**
   * Focus lands here when an update is found. It was `role="alertdialog"`
   * inside a group card: it announced an interruption it never made — no
   * scrim, no focus moved, no Escape — and Escape closed the whole window.
   */
  useEffect(() => {
    if (availableUpdate) confirm.current?.focus();
  }, [availableUpdate]);

  return (
    <Pane title="About">
      <Group>
        <Row label="Bard" hint="Capture. Dictate. Keep." />
        <Row label="Version" hint={store ? `${state.appVersion} · Microsoft Store` : state.appVersion} />
      </Group>

      <LicenseCard status={state.license} onStatus={refresh} />

      <Group title="Updates">
        {store ? (
          <Row label="Updates" hint="The Microsoft Store keeps Bard up to date." />
        ) : (
          <Row label="Updates" hint="Checked only when you ask. Nothing installs silently.">
            <Button onClick={() => void checkForUpdate()} disabled={checking} ariaBusy={checking}>
              <RefreshCw size={14} aria-hidden="true" className={checking ? "spin" : undefined} />
              {checking ? "Checking" : "Check"}
            </Button>
          </Row>
        )}
        {/*
          The most actionable string in the pane was a bare 12px note outside
          every group, unannounced, and the notes the backend fetched were
          never rendered at all — so a user could confirm an install with no
          changelog. It is a group row now, and the notes are part of it.
        */}
        {updateMessage ? (
          <Row
            label={updateMessage}
            hint={availableUpdate?.publishedAt ? `Released ${availableUpdate.publishedAt}.` : undefined}
          />
        ) : null}
        {availableUpdate ? (
          <div
            ref={confirm}
            className="update-confirm"
            role="group"
            tabIndex={-1}
            aria-label={`Install Bard ${availableUpdate.version}`}
            onKeyDown={(event) => {
              if (event.key !== "Escape") return;
              // The window closes itself on Escape; the Escape that dismisses
              // this has to say so first.
              event.preventDefault();
              event.stopPropagation();
              setAvailableUpdate(null);
            }}
          >
            <div>
              <strong>Install Bard {availableUpdate.version}?</strong>
              <p>Bard closes while it installs. Your captures and settings stay in place.</p>
              {availableUpdate.notes ? (
                <pre className="update-notes">{availableUpdate.notes}</pre>
              ) : (
                <p>No release notes were published for this version.</p>
              )}
            </div>
            <div className="row-actions">
              <Button onClick={() => setAvailableUpdate(null)}>
                Cancel
              </Button>
              <Button variant="primary" onClick={() => void installUpdate()} disabled={installing} ariaBusy={installing}>
                <Download size={14} aria-hidden="true" />
                {installing ? "Installing" : "Install"}
              </Button>
            </div>
          </div>
        ) : null}
      </Group>

      <Group title="Legal">
        {/* Two buttons both reading "Read" left the first two rows
            indistinguishable; the label is the only place a legal document is
            named. */}
        <Row label="Terms" hint="One licence: 365 days from activation, three computers, an offline lease.">
          <Button onClick={() => void api.openLink("https://miiso.dev/terms")}>Read terms</Button>
        </Row>
        <Row label="Privacy" hint="Local data, activation and the optional OpenRouter disclosures.">
          <Button onClick={() => void api.openLink("https://miiso.dev/privacy")}>Read privacy</Button>
        </Row>
        <Row label="Support" hint="Report a problem, including anything Refine returned.">
          <Button onClick={() => void api.openLink("https://miiso.dev/tools/bard#support")}>Contact</Button>
        </Row>
      </Group>

      <Group title="Privacy">
        <Row
          label="Local by default"
          hint="Audio capture, speech recognition, captures, and history stay on this computer. Purchasing uses miiso.dev; Bard has no account, sync, or telemetry."
        />
        <Row
          label="Refine, only if you turn it on"
          hint="Bard then sends the finished transcript text — and nothing else — to OpenRouter. Audio, captures, history, and unrelated items are never sent, and the untouched local transcript is used whenever Refine is off or fails."
        />
      </Group>
    </Pane>
  );
}
