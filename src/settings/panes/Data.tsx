import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileWarning, LoaderCircle } from "lucide-react";
import { api } from "../../store";
import type { CrashReport } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row } from "../ui";

export default function DataPane({ refresh }: PaneProps) {
  const [exportDir, setExportDir] = useState("");
  const [crashReport, setCrashReport] = useState<CrashReport | null>(null);
  const [crashLoading, setCrashLoading] = useState(false);
  const [crashError, setCrashError] = useState<string | null>(null);

  useEffect(() => {
    void api.getExportDir().then(setExportDir);
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

  return (
    <Pane title="Data" description="Everything lives in one SQLite file on this machine.">
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
        <Row label="Clear history" hint="Removes every captured item. This cannot be undone.">
          <Button variant="danger" onClick={() => void api.clearHistory().then(refresh)}>
            Clear
          </Button>
        </Row>
      </Group>
    </Pane>
  );
}
