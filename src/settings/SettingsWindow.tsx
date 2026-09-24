import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertCircle, LoaderCircle, RotateCcw, X } from "lucide-react";
import { api } from "../store";
import { useTheme } from "../useTheme";
import type { AppState, ModelProgress } from "../types";

import GeneralPane from "./panes/General";
import CapturePane from "./panes/Capture";
import DictationPane, { PostprocessingPane } from "./panes/Dictation";
import ModelsPane from "./panes/Models";
import ShortcutsPane from "./panes/Shortcuts";
import DataPane from "./panes/Data";
import AboutPane from "./panes/About";

/*
  Settings is a companion of the panel.

  It used to be a sheet inside a 380px panel, which was already cramped at nine
  settings — and Bard owes the surface of two apps. It now docks beside the
  panel as a frameless surface of the same material, at the panel's height.
  The sections are ordered the way the product is used rather than the way it
  is built: what the app does at rest, then each of its two halves, models,
  optional cloud refinement, then keys, data and provenance.
*/

const SECTIONS = [
  { id: "general", label: "General" },
  { id: "capture", label: "Capture" },
  { id: "dictation", label: "Dictation" },
  { id: "models", label: "Models" },
  { id: "postprocessing", label: "Refine" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "data", label: "Data" },
  { id: "about", label: "About" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Bard could not load Settings.";
}

export default function SettingsWindow() {
  useTheme();

  const [state, setState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const [inventoryVersion, setInventoryVersion] = useState(0);
  const [section, setSection] = useState<SectionId>("general");

  const loadState = useCallback(async () => {
    try {
      const next = await api.getState();
      setState(next);
      setLoadError(null);
    } catch (error) {
      setLoadError(errorText(error));
    }
  }, []);

  useEffect(() => {
    let alive = true;
    const pendingUnlisteners: Promise<() => void>[] = [];

    const refresh = () => {
      if (!alive) return;
      void api
        .getState()
        .then((next) => {
          if (!alive) return;
          setState(next);
          setLoadError(null);
        })
        .catch((error: unknown) => {
          if (alive) setLoadError(errorText(error));
        });
    };

    const subscribe = (subscription: Promise<() => void>) => {
      pendingUnlisteners.push(subscription.catch(() => () => undefined));
    };

    refresh();
    void api.refreshLicense().then((license) => {
      if (alive) setState((current) => (current ? { ...current, license } : current));
    }).catch(() => undefined);
    // `refresh` keeps shell state current. Dedicated model events drive the
    // inventory pane so a compatibility probe's state refresh cannot restart
    // the same probe. `model-ready` remains the terminal success signal.
    subscribe(listen("refresh", refresh));
    subscribe(
      listen("model-progress", (event) => {
        if (alive) setProgress(event.payload as ModelProgress);
      }),
    );
    subscribe(
      listen("model-download-failed", (event) => {
        if (!alive) return;
        const payload = event.payload as { id?: string; error?: string };
        setProgress({
          id: payload.id ?? "unknown",
          received: 0,
          total: 0,
          percent: 0,
          phase: "Download failed",
          error: payload.error ?? "The model download failed.",
        });
      }),
    );
    subscribe(
      listen("model-installed", () => {
        if (alive) setInventoryVersion((version) => version + 1);
      }),
    );
    subscribe(
      listen("model-activated", () => {
        if (alive) setInventoryVersion((version) => version + 1);
      }),
    );
    subscribe(
      listen("model-ready", () => {
        if (!alive) return;
        setProgress(null);
        refresh();
      }),
    );

    return () => {
      alive = false;
      for (const pending of pendingUnlisteners) void pending.then((unlisten) => unlisten());
    };
  }, []);

  const refresh = useCallback(() => {
    void loadState();
  }, [loadState]);

  // Esc closes the companion, exactly as it dismisses the panel.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) void getCurrentWindow().close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const close = (
    <button
      type="button"
      className="companion-close"
      aria-label="Close"
      title="Close"
      onClick={() => void getCurrentWindow().close()}
    >
      <X aria-hidden="true" strokeWidth={2} />
    </button>
  );

  if (!state) {
    return (
      <div className="settings-root companion">
        {close}
        <main className="settings-load-state">
          <div className="settings-load-content">
            {loadError ? (
              <>
                <AlertCircle className="settings-load-icon" aria-hidden="true" />
                <h1>Settings could not load</h1>
                <p role="alert">{loadError}</p>
                <button type="button" className="btn primary" onClick={() => void loadState()}>
                  <RotateCcw aria-hidden="true" />
                  Try again
                </button>
              </>
            ) : (
              <>
                <LoaderCircle className="settings-load-icon spinning" aria-hidden="true" />
                <h1>Loading Settings</h1>
                <p role="status">Reading Bard's local preferences…</p>
              </>
            )}
          </div>
        </main>
      </div>
    );
  }

  const panes: Record<SectionId, JSX.Element> = {
    general: <GeneralPane state={state} refresh={refresh} />,
    capture: <CapturePane state={state} refresh={refresh} />,
    dictation: <DictationPane state={state} refresh={refresh} />,
    models: (
      <ModelsPane
        state={state}
        refresh={refresh}
        progress={progress}
        inventoryVersion={inventoryVersion}
      />
    ),
    postprocessing: <PostprocessingPane state={state} refresh={refresh} />,
    shortcuts: <ShortcutsPane />,
    data: <DataPane state={state} refresh={refresh} />,
    about: <AboutPane state={state} refresh={refresh} />,
  };

  return (
    <div className="settings-root companion">
      {close}
      {/* The sidebar is the handle: opened on its own, Settings moves by it. */}
      <nav className="settings-nav" aria-label="Settings sections" data-tauri-drag-region>
        <p className="companion-title settings-nav-title" data-tauri-drag-region>Settings</p>
        {SECTIONS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={section === item.id ? "on" : ""}
            aria-current={section === item.id ? "page" : undefined}
            onClick={() => setSection(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>
      <main className="settings-content" key={section}>
        {panes[section]}
      </main>
    </div>
  );
}

export interface PaneProps {
  state: AppState;
  refresh: () => void;
}
