import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../store";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Pane, Row } from "../ui";

export default function DataPane({ refresh }: PaneProps) {
  const [exportDir, setExportDir] = useState("");

  useEffect(() => {
    void api.getExportDir().then(setExportDir);
  }, []);

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
