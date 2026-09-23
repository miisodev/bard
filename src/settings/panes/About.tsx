import { useState } from "react";
import { RefreshCw, Download, X } from "lucide-react";
import type { PaneProps } from "../SettingsWindow";
import LicenseCard from "../../components/LicenseCard";
import { api } from "../../store";
import { Button, Group, Note, Pane, Row } from "../ui";

export default function AboutPane({ state, refresh }: PaneProps) {
  const [checking, setChecking] = useState(false);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<{ version: string; notes: string | null; publishedAt: string | null } | null>(null);
  const [installing, setInstalling] = useState(false);

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
        <Row label="bard" hint="Capture. Dictate. Keep." />
        <Row label="Version" hint={state.appVersion} />
      </Group>

      <LicenseCard status={state.license} onStatus={refresh} />

      <Group title="Updates">
        <Row label="Bard updates" hint="Updates are checked only when you ask. Nothing installs silently.">
          <Button onClick={() => void checkForUpdate()} disabled={checking}>
            <RefreshCw size={14} aria-hidden="true" />
            {checking ? "Checking…" : "Check for updates"}
          </Button>
        </Row>
        {updateMessage ? <Note>{updateMessage}</Note> : null}
        {availableUpdate ? (
          <div className="update-confirm" role="alertdialog" aria-label="Confirm Bard update">
            <div>
              <strong>Install Bard {availableUpdate.version}?</strong>
              <p>Bard will close while it installs. Your local captures and settings stay in place.</p>
            </div>
            <div className="row-actions">
              <Button onClick={() => setAvailableUpdate(null)} aria-label="Cancel update">
                <X size={14} aria-hidden="true" />
                Cancel
              </Button>
              <Button onClick={() => void installUpdate()} disabled={installing}>
                <Download size={14} aria-hidden="true" />
                {installing ? "Installing…" : "Install and restart"}
              </Button>
            </div>
          </div>
        ) : null}
      </Group>

      <Group title="Legal">
        <Row label="Terms & licence" hint="Annual term, three activations, offline lease and renewal terms.">
          <a className="btn" href="https://miiso.dev/terms" target="_blank" rel="noreferrer">Read terms</a>
        </Row>
        <Row label="Privacy" hint="Local-data, activation and optional OpenRouter disclosures.">
          <a className="btn" href="https://miiso.dev/privacy" target="_blank" rel="noreferrer">Read privacy</a>
        </Row>
      </Group>

      <Group title="Privacy">
        <Row
          label="Local by default"
          hint="Audio capture, speech recognition, captures, and history stay on this computer. Purchasing uses miiso.dev; Bard has no sync service or telemetry."
        />
        <Row
          label="Optional cloud post-processing"
          hint="If you explicitly enable Post-processing, Bard sends only the finished transcript text to OpenRouter. Audio, captures, history, and unrelated items are not sent. Bard falls back to the untouched local transcript whenever cloud processing is off or fails."
        />
      </Group>
    </Pane>
  );
}
