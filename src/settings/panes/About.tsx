import { useState } from "react";
import { RefreshCw, Download } from "lucide-react";
import type { PaneProps } from "../SettingsWindow";
import LicenseCard from "../../components/LicenseCard";
import { api } from "../../store";
import { Button, Group, Note, Pane, Row } from "../ui";

export default function AboutPane({ state, refresh }: PaneProps) {
  const [checking, setChecking] = useState(false);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<{ version: string; notes: string | null; publishedAt: string | null } | null>(null);
  const [installing, setInstalling] = useState(false);
  const store = state.channel === "store";

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
            <Button onClick={() => void checkForUpdate()} disabled={checking}>
              <RefreshCw size={14} aria-hidden="true" className={checking ? "spin" : undefined} />
              {checking ? "Checking" : "Check"}
            </Button>
          </Row>
        )}
        {availableUpdate ? (
          <div className="update-confirm" role="alertdialog" aria-label="Confirm Bard update">
            <div>
              <strong>Install Bard {availableUpdate.version}?</strong>
              <p>Bard closes while it installs. Your captures and settings stay in place.</p>
            </div>
            <div className="row-actions">
              <Button onClick={() => setAvailableUpdate(null)}>Cancel</Button>
              <Button variant="primary" onClick={() => void installUpdate()} disabled={installing}>
                <Download size={14} aria-hidden="true" />
                {installing ? "Installing" : "Install"}
              </Button>
            </div>
          </div>
        ) : null}
      </Group>
      {updateMessage ? <Note>{updateMessage}</Note> : null}

      <Group title="Legal">
        <Row label="Terms" hint="Annual term, three activations, offline lease and renewal.">
          <Button onClick={() => void api.openLink("https://miiso.dev/terms")}>Read</Button>
        </Row>
        <Row label="Privacy" hint="Local data, activation and the optional OpenRouter disclosures.">
          <Button onClick={() => void api.openLink("https://miiso.dev/privacy")}>Read</Button>
        </Row>
      </Group>

      <Group title="Privacy">
        <Row
          label="Local by default"
          hint="Audio capture, speech recognition, captures, and history stay on this computer. Purchasing uses miiso.dev; Bard has no sync service or telemetry."
        />
        <Row
          label="Refine, only if you turn it on"
          hint="Bard then sends the finished transcript text — and nothing else — to OpenRouter. Audio, captures, history, and unrelated items are never sent, and the untouched local transcript is used whenever Refine is off or fails."
        />
      </Group>
    </Pane>
  );
}
