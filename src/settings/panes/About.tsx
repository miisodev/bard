import { useEffect, useRef, useState } from "react";
import { RefreshCw, Download } from "lucide-react";
import type { PaneProps } from "../SettingsWindow";
import { api } from "../../store";
import { updateDelivery } from "../../distribution";
import appIcon from "../../../src-tauri/icons/128x128.png";
import { Button, Group, Pane, Row } from "../ui";

export default function AboutPane({ state }: PaneProps) {
  const [checking, setChecking] = useState(false);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<{ version: string; notes: string | null; publishedAt: string | null } | null>(null);
  const [installing, setInstalling] = useState(false);
  const delivery = updateDelivery(state);
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
      <header className="about-hero">
        <img src={appIcon} alt="" width={64} height={64} />
        <div>
          <p className="about-name">Bard</p>
          <p className="about-line">Clipboard, Draw and Dictate</p>
          <p className="about-version">
            Version {state.appVersion}
            {` · ${delivery.label}`}
          </p>
        </div>
      </header>

      <Group title="Updates">
        {!delivery.inApp ? (
          <Row label="Updates" hint={delivery.hint} />
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

      <Group title="Privacy">
        <Row
          label="Everything stays on this PC"
          hint="Speech recognition, captures and history never leave it. Bard has no account, sync or analytics, and its source is open."
        />
      </Group>

      <nav className="about-links" aria-label="Legal and support">
        <button type="button" className="link-button" onClick={() => void api.openLink("https://miiso.dev/terms")}>
          Terms
        </button>
        <button type="button" className="link-button" onClick={() => void api.openLink("https://miiso.dev/privacy")}>
          Privacy
        </button>
        <button type="button" className="link-button" onClick={() => void api.openLink("https://miiso.dev/bard#support")}>
          Support
        </button>
        <span className="about-copyright">© 2026 miiso.dev</span>
      </nav>
    </Pane>
  );
}
