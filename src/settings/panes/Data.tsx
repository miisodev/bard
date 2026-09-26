import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, FileWarning, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "../../store";
import type { CrashReport } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, InlineConfirm, Note, Pane, Row, Segmented } from "../ui";

type Notice = { tone: "success" | "danger"; text: string };

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
  const [pruned, setPruned] = useState(false);
  const [pruneError, setPruneError] = useState<string | null>(null);
  const [exportNotice, setExportNotice] = useState<Notice | null>(null);
  const [dangerError, setDangerError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<"captures" | "history" | "delete" | null>(null);

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

  const pruneNow = async () => {
    if (pruneLoading) return;
    setPruneLoading(true);
    setPruneError(null);
    setPruned(false);
    try {
      await api.pruneHistoryNow();
      setPruned(true);
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
        <Row label="Export to Markdown" hint="Writes bard-export.md to the folder above.">
          <Button onClick={() => void exportMarkdown()}>
            Export
          </Button>
        </Row>
        {exportNotice ? <Note tone={exportNotice.tone}>{exportNotice.text}</Note> : null}
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
              <Segmented
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
              <Segmented
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
        {pruned && !pruneError ? <Note tone="success">History pruned.</Note> : null}
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
          hint="Removes every item in the timeline, including your notes, drawings and favourites."
        >
          <Button variant="danger" onClick={() => setConfirm("history")}>
            Clear history
          </Button>
        </Row>
        <Row
          label="Delete local data"
          hint="Removes Bard's data, models, logs, settings and stored credentials from this Windows account, including everything above."
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
            Clear the whole timeline? Notes, drawings and favourites go too, and cannot come back.
          </InlineConfirm>
        ) : null}
        {confirm === "delete" ? (
          <InlineConfirm
            onCancel={() => setConfirm(null)}
            onConfirm={() => void runDanger(api.deleteAllData, "Couldn't delete Bard's data. Try again.")}
            confirmLabel="Delete everything"
          >
            Delete all Bard data and credentials from this computer? This cannot be undone.
          </InlineConfirm>
        ) : null}
        {dangerError ? <Note tone="danger">{dangerError}</Note> : null}
      </Group>
    </Pane>
  );
}
