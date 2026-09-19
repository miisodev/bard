import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../store";
import { useTheme } from "../useTheme";
import type { AppState, ModelProgress } from "../types";

import GeneralPane from "./panes/General";
import CapturePane from "./panes/Capture";
import DictationPane from "./panes/Dictation";
import ModelsPane from "./panes/Models";
import ShortcutsPane from "./panes/Shortcuts";
import DataPane from "./panes/Data";
import AboutPane from "./panes/About";

/*
  Settings is its own window.

  It used to be a sheet inside a 380px panel, which was already cramped at nine
  settings — and bard owes the surface of two apps, so it is heading for many
  times that. Handy puts its sixty behind a sidebar in a window of its own, and
  a Handy user reaches for exactly that.

  The sections are ordered the way the product is used rather than the way it
  is built: what the app does at rest, then each of its two halves, then the
  models they need, then keys, data and provenance.
*/

const SECTIONS = [
  { id: "general", label: "General" },
  { id: "capture", label: "Capture" },
  { id: "dictation", label: "Dictation" },
  { id: "models", label: "Models" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "data", label: "Data" },
  { id: "about", label: "About" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

export default function SettingsWindow() {
  useTheme();

  const [state, setState] = useState<AppState | null>(null);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const [section, setSection] = useState<SectionId>("general");

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];
    let alive = true;
    const refresh = () => {
      if (alive) void api.getState().then(setState);
    };
    refresh();
    unlisteners.push(listen("refresh", refresh));
    unlisteners.push(listen("model-progress", (e) => setProgress(e.payload as ModelProgress)));
    unlisteners.push(
      listen("model-ready", () => {
        setProgress(null);
        refresh();
      })
    );
    return () => {
      alive = false;
      for (const u of unlisteners) void u.then((fn) => fn());
    };
  }, []);

  const refresh = () => void api.getState().then(setState);

  if (!state) return <div className="settings-root" />;

  const panes: Record<SectionId, JSX.Element> = {
    general: <GeneralPane state={state} refresh={refresh} />,
    capture: <CapturePane state={state} refresh={refresh} />,
    dictation: <DictationPane state={state} refresh={refresh} />,
    models: <ModelsPane state={state} refresh={refresh} progress={progress} />,
    shortcuts: <ShortcutsPane state={state} refresh={refresh} />,
    data: <DataPane state={state} refresh={refresh} />,
    about: <AboutPane state={state} refresh={refresh} />,
  };

  return (
    <div className="settings-root">
      <nav className="settings-nav" aria-label="Settings sections">
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            type="button"
            className={section === s.id ? "on" : ""}
            aria-current={section === s.id ? "page" : undefined}
            onClick={() => setSection(s.id)}
          >
            {s.label}
          </button>
        ))}
      </nav>
      <main className="settings-content">{panes[section]}</main>
    </div>
  );
}

export interface PaneProps {
  state: AppState;
  refresh: () => void;
}
