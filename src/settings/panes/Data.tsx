import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileWarning, LoaderCircle } from "lucide-react";
import { api } from "../../store";
import type { CrashReport } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row, Segmented } from "../ui";

export default function DataPane({ refresh }: PaneProps) {
  const [exportDir, setExportDir] = useState("");
  const [crashReport, setCrashReport] = useState<CrashReport | null>(null);
  const [crashLoading, setCrashLoading] = useState(false);
  const [crashError, setCrashError] = useState<string | null>(null);
  const [retentionLoading, setRetentionLoading] = useState(true);
  const [clipboardDays, setClipboardDays] = useState("30");
  const [dictationDays, setDictationDays] = useState("30");
  const [retentionError, setRetentionError] = useState<string | null>(null);
  const [dataError, setDataError] = useState<string | null>(null);
  const [pruneLoading, setPruneLoading] = useState(false);

  useEffect(() => {
    let alive = true;
    void api.getExportDir().then((value) => {
      if (alive) setExportDir(value);
    });
    void Promise.all([
      api.getRetentionDays("clipboard"),
      api.getRetentionDays("dictation"),
    ])
      .then(([clipboard, dictation]) => {
        if (!alive) return;
        setClipboardDays(clipboard === null ? "forever" : String(clipboard));
        setDictationDays(dictation === null ? "forever" : String(dictation));
      })
      .catch(() => undefined)
      .finally(() => {
        if (alive) setRetentionLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

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

  const retentionOptions = [
    { value: "1", label: "1 day" },
    { value: "7", label: "7 days" },
    { value: "30", label: "30 days" },
    { value: "90", label: "90 days" },
    { value: "180", label: "180 days" },
    { value: "forever", label: "Forever" },
  ];

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

  const pruneNow = async () => {
    if (pruneLoading) return;
    setPruneLoading(true);
    setDataError(null);
    try {
      await api.pruneHistoryNow();
      refresh();
    } catch (error) {
      setDataError(error instanceof Error ? error.message : String(error));
    } finally {
      setPruneLoading(false);
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
          <Button
            onClick={() => {
              void api.exportMarkdown().then((p) => {
                window.dispatchEvent(new CustomEvent("bard-toast", { detail: `Exported → ${p}` }));
              });
            }}
          >
            Export
          </Button>
        </Row>
      </Group>

      <Group
        title="Retention"
        note="Automatic captures and dictations expire independently. Favorites and manual notes are never removed by age."
      >
        <Row
          label="Clipboard lifespan"
          hint={retentionLoading ? "Reading local retention…" : "Text and images copied from other apps."}
        >
          <Segmented
            label="Clipboard lifespan"
            value={clipboardDays}
            options={retentionOptions}
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
            options={retentionOptions}
            onChange={(value) => void setRetention("dictation", value)}
            disabled={retentionLoading}
          />
        </Row>
        {retentionError ? <Note tone="danger">{retentionError}</Note> : null}
        <Row label="Prune now" hint="Apply the selected lifespans immediately without changing them.">
          <Button onClick={() => void pruneNow()} disabled={pruneLoading} ariaBusy={pruneLoading}>
            {pruneLoading ? <LoaderCircle className="spinning" aria-hidden="true" /> : null}
            {pruneLoading ? "Pruning…" : "Prune expired items"}
          </Button>
        </Row>
        {dataError ? <Note tone="danger">{dataError}</Note> : null}
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

      <Group title="Danger">
        <Row label="Clear captures" hint="Removes automatic clipboard and dictation history. Manual notes and favorites stay.">
          <Button
            variant="danger"
            onClick={() => {
              setDataError(null);
              void api.clearCaptures().then(refresh).catch((error) => {
                setDataError(error instanceof Error ? error.message : String(error));
              });
            }}
          >
            Clear captures
          </Button>
        </Row>
        <Row label="Clear history" hint="Removes every captured item. This cannot be undone.">
          <Button
            variant="danger"
            onClick={() => {
              setDataError(null);
              void api.clearHistory().then(refresh).catch((error) => {
                setDataError(error instanceof Error ? error.message : String(error));
              });
            }}
          >
            Clear all
          </Button>
        </Row>
        <Row label="Delete local data" hint="Removes captures, models, logs, settings, and stored credentials from this Windows account.">
          <Button
            variant="danger"
            onClick={() => {
              if (window.confirm("Delete all Bard data and credentials from this computer? This cannot be undone.")) {
                void api.deleteAllData().then(refresh).catch((error) => {
                  setDataError(error instanceof Error ? error.message : String(error));
                });
              }
            }}
          >
            Delete everything
          </Button>
        </Row>
      </Group>
    </Pane>
  );
}
