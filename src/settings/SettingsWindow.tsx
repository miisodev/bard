import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertCircle, LoaderCircle, RotateCcw, X, SlidersHorizontal, Clipboard, Mic, Cpu, Keyboard, HardDrive, Info } from "lucide-react";
import ResizeHandles from "../components/ResizeHandles";
import { api } from "../store";
import { useTheme } from "../useTheme";
import type { AppState, ModelProgress } from "../types";

import GeneralPane from "./panes/General";
import ClipboardPane from "./panes/Clipboard";
import DictationPane from "./panes/Dictation";
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
  is built: how it looks and talks, then the clipboard, dictation and its
  models (with optional Refine), then keys, data and provenance.
*/

const SECTIONS = [
  { id: "general", icon: SlidersHorizontal, label: "General" },
  { id: "clipboard", icon: Clipboard, label: "Clipboard" },
  { id: "dictation", icon: Mic, label: "Dictation" },
  { id: "models", icon: Cpu, label: "Models" },
  { id: "shortcuts", icon: Keyboard, label: "Shortcuts" },
  { id: "data", icon: HardDrive, label: "Data" },
  { id: "about", icon: Info, label: "About" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

const SECTION_STEP: Record<string, number> = {
  ArrowDown: 1,
  ArrowRight: 1,
  ArrowUp: -1,
  ArrowLeft: -1,
};

/** The section to open on: `?pane=` in the browser preview, General otherwise. */
function initialSection(): SectionId {
  const requested = new URLSearchParams(location.search).get("pane");
  return SECTIONS.find((item) => item.id === requested)?.id ?? "general";
}

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
  const [section, setSection] = useState<SectionId>(initialSection);
  const nav = useRef<HTMLDivElement>(null);

  /**
   * A tablist: one Tab stop into the sidebar, arrow keys between sections, and
   * the current one marked as selected rather than merely styled. It was eight
   * buttons that each took a Tab press to reach pane content.
   */
  const moveSection = (next: number) => {
    const index = (next + SECTIONS.length) % SECTIONS.length;
    const id = SECTIONS[index].id;
    setSection(id);
    nav.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[index]?.focus();
  };

  const onSectionKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, index: number) => {
    const step = SECTION_STEP[event.key];
    if (step) {
      event.preventDefault();
      moveSection(index + step);
    } else if (event.key === "Home") {
      event.preventDefault();
      moveSection(0);
    } else if (event.key === "End") {
      event.preventDefault();
      moveSection(SECTIONS.length - 1);
    }
  };

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
    clipboard: <ClipboardPane state={state} refresh={refresh} />,
    dictation: <DictationPane />,
    models: (
      <ModelsPane
        state={state}
        refresh={refresh}
        progress={progress}
        inventoryVersion={inventoryVersion}
      />
    ),
    shortcuts: <ShortcutsPane state={state} refresh={refresh} />,
    data: <DataPane state={state} refresh={refresh} />,
    about: <AboutPane state={state} refresh={refresh} />,
  };

  return (
    <div className="settings-root companion">
      <ResizeHandles />
      {/* The top strip moves the window from anywhere along it. */}
      <div className="companion-drag" data-tauri-drag-region aria-hidden="true" />
      {close}
      {/* The sidebar is the handle: opened on its own, Settings moves by it. */}
      <nav className="settings-nav" aria-label="Settings" data-tauri-drag-region>
        <p className="companion-title settings-nav-title" data-tauri-drag-region>Settings</p>
        <div role="tablist" aria-orientation="vertical" aria-label="Settings sections" ref={nav}>
          {SECTIONS.map((item, index) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              id={`settings-tab-${item.id}`}
              aria-controls={`settings-panel-${item.id}`}
              aria-selected={section === item.id}
              tabIndex={section === item.id ? 0 : -1}
              className={section === item.id ? "on" : ""}
              onClick={() => setSection(item.id)}
              onKeyDown={(event) => onSectionKeyDown(event, index)}
            >
              <item.icon size={16} strokeWidth={1.8} aria-hidden="true" />
              {item.label}
            </button>
          ))}
        </div>
      </nav>
      <main className="settings-content" key={section}>
        {/* The pane is the tabpanel, and it holds the scroll so it stays
            reachable by keyboard. `<main>` keeps its landmark: a tabpanel on
            it would replace it. */}
        <div
          role="tabpanel"
          id={`settings-panel-${section}`}
          aria-labelledby={`settings-tab-${section}`}
          tabIndex={0}
        >
          {panes[section]}
        </div>
      </main>
    </div>
  );
}

export interface PaneProps {
  state: AppState;
  refresh: () => void;
}
