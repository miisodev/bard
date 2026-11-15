import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { X } from "lucide-react";
import { StrictMode, useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { boot } from "../dev/boot";
import { api } from "../store";
import type { ShortcutState } from "../types";
import { useTheme } from "../useTheme";
import { guideGroups } from "./groups";
import "../styles.css";
import "./guide.css";

/*
  The shortcut guide: one small window that lists what Bard answers to.

  It is a window of its own, not a sheet over the history, so it can be read
  beside whatever the person is doing and summoned without bringing the main
  window along. Like every Bard window it leaves on Escape or a click away.
*/

function Guide() {
  useTheme();
  const [shortcuts, setShortcuts] = useState<ShortcutState | null>(null);

  const load = useCallback(() => {
    void api.getShortcuts().then(setShortcuts).catch(() => undefined);
  }, []);

  useEffect(() => {
    load();
    const off = listen("shortcuts-changed", load);
    // The window is kept for a while after it is put away; what it shows when
    // it comes back must be what is bound now.
    window.addEventListener("focus", load);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void getCurrentWindow().close();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      void off.then((fn) => fn());
      window.removeEventListener("focus", load);
      window.removeEventListener("keydown", onKey);
    };
  }, [load]);

  return (
    <div className="app guide">
      <header
        className="guide-head"
        onMouseDown={(event) => {
          if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
          void getCurrentWindow().startDragging();
        }}
      >
        <h1 id="guide-title">Shortcuts</h1>
        <button
          type="button"
          className="icon-btn guide-close"
          aria-label="Close"
          title="Close"
          onClick={() => void getCurrentWindow().close()}
        >
          <X aria-hidden="true" />
        </button>
      </header>
      <main className="guide-body" aria-labelledby="guide-title">
        {guideGroups(shortcuts).map((group) => (
          <section key={group.name} className="guide-group" aria-label={group.name}>
            <h2>{group.name}</h2>
            <dl>
              {group.rows.map((row) => (
                <div key={row.what} className="guide-row">
                  <dt>{row.what}</dt>
                  <dd className="shortcut-keys">
                    {row.keys.map((key, index) => (
                      <kbd key={`${key}-${index}`}>{key}</kbd>
                    ))}
                  </dd>
                </div>
              ))}
            </dl>
            {group.note ? <p className="guide-note">{group.note}</p> : null}
          </section>
        ))}
      </main>
    </div>
  );
}

void boot("guide").then(() => {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <Guide />
    </StrictMode>,
  );
});
