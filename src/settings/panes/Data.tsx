import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, Copy, FileWarning, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "../../store";
import type { CrashReport, RestorePreview } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, InlineConfirm, Note, Pane, Row, Select } from "../ui";

type Notice = { tone: "success" | "danger"; text: string };

function failureText(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

function itemCount(count: number): string {
  return `${count.toLocaleString()} item${count === 1 ? "" : "s"}`;
}

const RETENTION_OPTIONS = [
  { value: "1", label: "1 day" },
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "180", label: "180 days" },
  { value: "forever", label: "Forever" },
];

export default function DataPane({ refresh }: PaneProps) {
  const [exportDir, setExportDir] = useState("");
  const [crashReport, setCrashReport] = useState<CrashReport | null>(null);
  const [crashLoading, setCrashLoading] = useState(false);
  const [crashError, setCrashError] = useState<string | null>(null);

  // "" means "not read yet", and it stays "" if the read fails. It used to
  // start at "30", so a failed read left the pane presenting a hardcoded
  // thirty days as though it had been read out of the database.
  const [clipboardDays, setClipboardDays] = useState("");
  const [dictationDays, setDictationDays] = useState("");
  const [retentionLoading, setRetentionLoading] = useState(true);
  const [retentionReadError, setRetentionReadError] = useState<string | null>(null);
  const [retentionAttempt, setRetentionAttempt] = useState(0);
  const [retentionError, setRetentionError] = useState<string | null>(null);
  const [pruneLoading, setPruneLoading] = useState(false);
  const [pruned, setPruned] = useState<number | null>(null);
  const [pruneError, setPruneError] = useState<string | null>(null);
  const [exportNotice, setExportNotice] = useState<Notice | null>(null);
  const [dangerError, setDangerError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<"captures" | "history" | "delete" | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);
  const [backupNotice, setBackupNotice] = useState<Notice | null>(null);
  const [restorePreview, setRestorePreview] = useState<RestorePreview | null>(null);
  const [restoreBusy, setRestoreBusy] = useState(false);
  const [restoreNotice, setRestoreNotice] = useState<Notice | null>(null);
  const [copyingDiagnostics, setCopyingDiagnostics] = useState(false);
  const [copiedDiagnostics, setCopiedDiagnostics] = useState(false);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void api.getExportDir().then((value) => {
      if (alive) setExportDir(value);
    });
    setRetentionLoading(true);
    setRetentionReadError(null);
    void Promise.all([
      api.getRetentionDays("clipboard"),
      api.getRetentionDays("dictation"),
    ])
      .then(([clipboard, dictation]) => {
        if (!alive) return;
        setClipboardDays(clipboard === null ? "forever" : String(clipboard));
        setDictationDays(dictation === null ? "forever" : String(dictation));
      })
      .catch((error: unknown) => {
        // A surface that cannot tell "there is nothing" from "I could not find
        // out" has to say so. Swallowing this left a plausible-looking value on
        // screen that no database had ever confirmed.
        if (alive) {
          setRetentionReadError(
            error instanceof Error && error.message
              ? error.message
              : String(error),
          );
        }
      })
      .finally(() => {
        if (alive) setRetentionLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [retentionAttempt]);

  const loadCrashReport = async () => {
    if (crashLoading) return;
    setCrashLoading(true);
    setCrashError(null);
    try {
      setCrashReport(await api.getCrashReport());
    } catch (error) {
      setCrashError(error instanceof Error ? error.message : String(error));
    } finally {
      setCrashLoading(false);
    }
  };

  const setRetention = async (origin: "clipboard" | "dictation", value: string) => {
    const previous = origin === "clipboard" ? clipboardDays : dictationDays;
    if (origin === "clipboard") setClipboardDays(value);
    else setDictationDays(value);
    setRetentionError(null);
    try {
      await api.setRetentionDays(origin, value === "forever" ? null : Number(value));
    } catch (error) {
      if (origin === "clipboard") setClipboardDays(previous);
      else setDictationDays(previous);
      setRetentionError(error instanceof Error ? error.message : String(error));
    }
  };

  const exportMarkdown = async () => {
    if (exportNotice) setExportNotice(null);
    try {
      await api.exportMarkdown();
      setExportNotice({ tone: "success", text: "Exported" });
    } catch {
      // No catch at all before: a failed export was an unhandled rejection and
      // no message. The path is not announced either — it truncates in a
      // 380px panel, and Settings has no toast host of its own.
      setExportNotice({ tone: "danger", text: "Export failed" });
    }
  };

  const backUpNow = async () => {
    if (backupBusy) return;
    setBackupNotice(null);
    const dir = await open({ directory: true });
    if (typeof dir !== "string") return;
    setBackupBusy(true);
    try {
      const summary = await api.backupNow(dir);
      setBackupNotice({
        tone: "success",
        text: `Backed up ${itemCount(summary.counts.items)} and ${summary.counts.images.toLocaleString()} image${summary.counts.images === 1 ? "" : "s"} to ${summary.path}`,
      });
    } catch (error) {
      setBackupNotice({ tone: "danger", text: failureText(error) });
    } finally {
      setBackupBusy(false);
    }
  };

  const chooseRestore = async () => {
    if (restoreBusy) return;
    setRestoreNotice(null);
    setRestorePreview(null);
    const dir = await open({ directory: true });
    if (typeof dir !== "string") return;
    setRestoreBusy(true);
    try {
      // The first call changes nothing: it validates the folder and reports
      // what the backup holds so the confirmation below can state it.
      const outcome = await api.restoreFromBackup(dir, false);
      if (outcome.outcome === "needsConfirmation") {
        setRestorePreview(outcome.preview);
      }
    } catch (error) {
      setRestoreNotice({ tone: "danger", text: failureText(error) });
    } finally {
      setRestoreBusy(false);
    }
  };

  const runRestore = async () => {
    if (restoreBusy || !restorePreview) return;
    setRestoreBusy(true);
    setRestoreNotice(null);
    try {
      const outcome = await api.restoreFromBackup(restorePreview.path, true);
      setRestorePreview(null);
      if (outcome.outcome === "restored") {
        setRestoreNotice({
          tone: "success",
          text:
            `Restored ${itemCount(outcome.counts.items)} and ` +
            `${outcome.counts.images.toLocaleString()} image${outcome.counts.images === 1 ? "" : "s"}. ` +
            `Restart Bard to see everything. The data that was here is kept at ${outcome.previous}.`,
        });
        refresh();
      }
    } catch (error) {
      setRestoreNotice({ tone: "danger", text: failureText(error) });
    } finally {
      setRestoreBusy(false);
    }
  };

  const copyDiagnostics = async () => {
    if (copyingDiagnostics) return;
    setDiagnosticsError(null);
    setCopiedDiagnostics(false);
    setCopyingDiagnostics(true);
    try {
      await api.copyDiagnosticReport();
      setCopiedDiagnostics(true);
    } catch (error) {
      setDiagnosticsError(failureText(error));
    } finally {
      setCopyingDiagnostics(false);
    }
  };

  const pruneNow = async () => {
    if (pruneLoading) return;
    setPruneLoading(true);
    setPruneError(null);
    setPruned(null);
    try {
      // The command reports how many rows it removed. Discarding that leaves a
      // bare "History pruned." on a destructive action, and a number is what lets
      // someone trust it actually did anything.
      const removed = await api.pruneHistoryNow();
      setPruned(removed);
      refresh();
    } catch {
      // Not the backend's words: they are a debug string at best and empty at
      // worst, and "Prune failed" with no next step is not a message.
      setPruneError("Couldn't prune history. Try again.");
    } finally {
      setPruneLoading(false);
    }
  };

  const runDanger = async (action: () => Promise<unknown>, failure: string) => {
    setConfirm(null);
    setDangerError(null);
    try {
      await action();
      refresh();
    } catch {
      setDangerError(failure);
    }
  };

  return (
    <Pane title="Data" description="Captures, settings, and models live in Bard's local application-data directory.">
      <Group title="Export">
        <Row label="Export folder" hint={exportDir || "Documents"}>
          <Button
            onClick={() => {
              void open({ directory: true }).then((dir) => {
                if (typeof dir === "string") {
                  setExportDir(dir);
                  void api.setExportDir(dir);
                }
              });
            }}
          >
            Browse…
          </Button>
        </Row>
        <Row label="Export to Markdown" hint="Writes bard-export.md plus images/ and files/ to the folder above.">
          <Button onClick={() => void exportMarkdown()}>
            Export
          </Button>
        </Row>
        {exportNotice ? <Note tone={exportNotice.tone}>{exportNotice.text}</Note> : null}
      </Group>

      <Group
        title="Backup"
        note="A backup folder holds the database, the images and a manifest. Keep it on another drive; restore replaces Bard's local data with it."
      >
        <Row
          label="Back up now"
          hint="Writes a new Bard-backup-… folder into a folder you choose."
        >
          <Button onClick={() => void backUpNow()} disabled={backupBusy} ariaBusy={backupBusy}>
            {backupBusy ? <LoaderCircle className="spinning" aria-hidden="true" /> : null}
            {backupBusy ? "Backing up…" : "Back up…"}
          </Button>
        </Row>
        {backupNotice ? <Note tone={backupNotice.tone}>{backupNotice.text}</Note> : null}
        <Row
          label="Restore from backup"
          hint="Replaces the database and images from a backup folder. Nothing is written until you confirm."
        >
          <Button onClick={() => void chooseRestore()} disabled={restoreBusy} ariaBusy={restoreBusy}>
            {restoreBusy ? <LoaderCircle className="spinning" aria-hidden="true" /> : null}
            {restoreBusy ? "Reading…" : "Restore…"}
          </Button>
        </Row>
        {restorePreview ? (
          <InlineConfirm
            onCancel={() => setRestorePreview(null)}
            onConfirm={() => void runRestore()}
            confirming={restoreBusy}
            confirmLabel="Replace with backup"
          >
            Replace Bard's local data with the backup at {restorePreview.path}? It holds{" "}
            {itemCount(restorePreview.manifest.counts.items)} and{" "}
            {restorePreview.manifest.counts.images.toLocaleString()} image
            {restorePreview.manifest.counts.images === 1 ? "" : "s"}
            {restorePreview.fresh
              ? "; this Bard holds nothing yet."
              : `; this Bard holds ${itemCount(restorePreview.liveItems)} right now.`}{" "}
            The data that is here is kept beside it, not deleted.
          </InlineConfirm>
        ) : null}
        {restoreNotice ? <Note tone={restoreNotice.tone}>{restoreNotice.text}</Note> : null}
      </Group>

      <Group
        title="Retention"
        note="Automatic captures and dictations expire independently. Favorites and manual notes are never removed by age."
      >
        {retentionReadError ? (
          <div className="error-empty" role="alert">
            <AlertCircle aria-hidden="true" />
            <div>
              <strong>Retention settings could not be read.</strong>
              <span>
                {retentionReadError} Bard has not guessed at them, and nothing here has
                changed.
              </span>
            </div>
            <Button
              onClick={() => setRetentionAttempt((attempt) => attempt + 1)}
              disabled={retentionLoading}
            >
              {retentionLoading ? (
                <LoaderCircle className="spinning" aria-hidden="true" />
              ) : (
                <RotateCcw aria-hidden="true" />
              )}
              Retry
            </Button>
          </div>
        ) : (
          <>
            <Row
              label="Clipboard lifespan"
              hint={retentionLoading ? "Reading local retention…" : "Text and images copied from other apps."}
            >
              <Select
                label="Clipboard lifespan"
                value={clipboardDays}
                options={RETENTION_OPTIONS}
                onChange={(value) => void setRetention("clipboard", value)}
                disabled={retentionLoading}
              />
            </Row>
            <Row
              label="Dictation lifespan"
              hint="Final transcripts are stored locally; raw audio is never stored."
            >
              <Select
                label="Dictation lifespan"
                value={dictationDays}
                options={RETENTION_OPTIONS}
                onChange={(value) => void setRetention("dictation", value)}
                disabled={retentionLoading}
              />
            </Row>
          </>
        )}
        {retentionError ? <Note tone="danger">{retentionError}</Note> : null}
        <Row
          label="Prune now"
          hint="Removes everything past the lifespans above, right now. It does not change them."
        >
          <Button onClick={() => void pruneNow()} disabled={pruneLoading} ariaBusy={pruneLoading}>
            {pruneLoading ? <LoaderCircle className="spinning" aria-hidden="true" /> : null}
            {pruneLoading ? "Pruning…" : "Prune now"}
          </Button>
        </Row>
        {pruned !== null && !pruneError ? (
          <Note tone="success">
            {pruned === 0
              ? "Nothing had expired. History pruned."
              : `Pruned ${pruned.toLocaleString()} item${pruned === 1 ? "" : "s"}.`}
          </Note>
        ) : null}
        {pruneError ? <Note tone="danger">{pruneError}</Note> : null}
      </Group>

      <Group title="Diagnostics">
        <Row
          label="Latest crash report"
          hint="Crash evidence is written locally before Bard exits. This never includes audio, captures, or API keys."
        >
          <Button onClick={() => void loadCrashReport()} disabled={crashLoading} ariaBusy={crashLoading}>
            {crashLoading ? <LoaderCircle className="spinning" aria-hidden="true" /> : <FileWarning aria-hidden="true" />}
            {crashLoading ? "Loading…" : "Load report"}
          </Button>
        </Row>
        {crashError ? <Note tone="danger">{crashError}</Note> : null}
        <Row
          label="Copy diagnostic report"
          hint="Bard's version, storage state, the latest crash report and recent lifecycle lines, on the clipboard for a support message. Nothing is sent anywhere."
        >
          <Button onClick={() => void copyDiagnostics()} disabled={copyingDiagnostics} ariaBusy={copyingDiagnostics}>
            {copyingDiagnostics ? <LoaderCircle className="spinning" aria-hidden="true" /> : <Copy aria-hidden="true" />}
            {copyingDiagnostics ? "Copying…" : "Copy report"}
          </Button>
        </Row>
        {copiedDiagnostics ? <Note tone="success">Copied. Paste it into your support message.</Note> : null}
        {diagnosticsError ? <Note tone="danger">{diagnosticsError}</Note> : null}
        {crashReport ? (
          <div className="crash-report">
            <div className="ellipsis" title={crashReport.path}>{crashReport.path}</div>
            <pre>{crashReport.contents}</pre>
          </div>
        ) : crashError ? null : (
          <Note>No crash report has been recorded on this machine.</Note>
        )}
      </Group>

      <Group
        title="Danger"
        note="All three are permanent. Clear captures is the narrow one; the other two remove work you cannot get back."
      >
        <Row
          label="Clear captures"
          hint="Removes automatic clipboard and dictation history only. Your notes, drawings and favourites stay."
        >
          <Button onClick={() => setConfirm("captures")}>Clear captures</Button>
        </Row>
        <Row
          label="Clear history"
          hint="Removes every item in the shelf, including your notes, drawings and favourites."
        >
          <Button variant="danger" onClick={() => setConfirm("history")}>
            Clear history
          </Button>
        </Row>
        <Row
          label="Delete local data"
          hint="Removes Bard's data, models, logs, settings and saved licence from this Windows account, including everything above."
        >
          <Button variant="danger" onClick={() => setConfirm("delete")}>
            Delete everything
          </Button>
        </Row>

        {confirm === "captures" ? (
          <InlineConfirm
            onCancel={() => setConfirm(null)}
            onConfirm={() => void runDanger(api.clearCaptures, "Couldn't clear captures. Try again.")}
            confirmLabel="Clear captures"
          >
            Remove automatic clipboard and dictation history? Your notes and favourites stay.
          </InlineConfirm>
        ) : null}
        {confirm === "history" ? (
          <InlineConfirm
            onCancel={() => setConfirm(null)}
            onConfirm={() => void runDanger(api.clearHistory, "Couldn't clear history. Try again.")}
            confirmLabel="Clear history"
          >
            Clear the whole shelf? Notes, drawings and favourites go too, and cannot come back.
          </InlineConfirm>
        ) : null}
        {confirm === "delete" ? (
          <InlineConfirm
            onCancel={() => setConfirm(null)}
            onConfirm={() => void runDanger(api.deleteAllData, "Couldn't delete Bard's data. Try again.")}
            confirmLabel="Delete everything"
          >
            Delete all Bard data from this computer? This cannot be undone. The 14-day
            trial clock and this PC's installation identity stay, so the trial does not
            restart and a paid licence does not lose its seat.
          </InlineConfirm>
        ) : null}
        {dangerError ? <Note tone="danger">{dangerError}</Note> : null}
      </Group>
    </Pane>
  );
}
