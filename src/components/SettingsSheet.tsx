import { Download, Mic } from "lucide-react";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../store";
import type { AppState, ModelInfo, ModelProgress } from "../types";

export default function SettingsSheet({
  state,
  modelProgress,
  onClose,
  onState,
}: {
  state: AppState;
  modelProgress: ModelProgress | null;
  onClose: () => void;
  onState: (s: AppState) => void;
}) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");
  const [recording, setRecording] = useState<string | null>(null);
  const [keybinds, setKeybinds] = useState<Record<string, string>>({});
  const [autostart, setAutostart] = useState(state.autostartEnabled);
  const [doubleShift, setDoubleShift] = useState(state.captureDoubleShift);
  const [soundOnCapture, setSoundOnCapture] = useState(true);
  const [exportDir, setExportDir] = useState("");

  useEffect(() => {
    void api.getModels().then(setModels);
    void api.getMicrophones().then(setMicrophones);
    void api.getSoundOnCapture().then(setSoundOnCapture);
    void api.getExportDir().then(setExportDir);
    void api.getSettingStr("selected_microphone").then(setSelectedMic);
    try {
      setKeybinds(JSON.parse(state.keybinds) as Record<string, string>);
    } catch {
      setKeybinds({});
    }
  }, []);

  const refresh = () => void api.getState().then(onState);
  const saveKeybinds = (kb: Record<string, string>) => {
    setKeybinds(kb);
    void api.setKeybinds(JSON.stringify(kb));
  };

  const progress = modelProgress;

  return (
    <div className="sheet" onClick={onClose}>
      <div className="sheet-card" onClick={(e) => e.stopPropagation()}>
        <h2>Settings</h2>

        <div className="section-title">Voice</div>
        {models.map((m) => (
          <div key={m.id} className="model-card">
            <div className="model-head">
              <div>
                <div className="name">{m.name}</div>
                <div className="meta">
                  {m.downloaded ? "Installed" : `${m.sizeMb} MB · offline`}
                </div>
              </div>
              {progress?.id === m.id && progress.percent < 100 ? (
                <button className="btn" onClick={() => void api.downloadModelCancel()}>
                  Cancel
                </button>
              ) : !m.downloaded ? (
                <button
                  className="btn primary"
                  onClick={() => void api.downloadModel(m.id)}
                >
                  <Download size={12} /> Download
                </button>
              ) : m.id !== state.modelId ? (
                <button
                  className="btn"
                  onClick={() => {
                    void api.setModel(m.id).then(refresh);
                  }}
                >
                  Use
                </button>
              ) : (
                <span className="meta" style={{ color: "var(--accent)" }}>
                  Active
                </span>
              )}
            </div>
            {progress?.id === m.id ? (
              <div className="progress">
                <div className="fill" style={{ width: `${Math.min(100, progress.percent)}%` }} />
              </div>
            ) : null}
          </div>
        ))}
        <div className="setting-row">
          <span>
            <div className="label">
              <Mic size={12} style={{ verticalAlign: -1, marginRight: 5 }} />
              Microphone
            </div>
            <div className="sub">Default input is used unless one is picked</div>
          </span>
          <select
            className="field select"
            value={selectedMic}
            onChange={(e) => {
              setSelectedMic(e.target.value);
              void api.setMicrophone(e.target.value);
            }}
          >
            <option value="">System default</option>
            {microphones.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
        </div>

        <div className="section-title">Sound</div>
        <div className="setting-row">
          <span>
            <div className="label">Capture sound</div>
            <div className="sub">A soft pop when something is captured</div>
          </span>
          <button
            className={`toggle${soundOnCapture ? " on" : ""}`}
            onClick={() => {
              const next = !soundOnCapture;
              setSoundOnCapture(next);
              void api.setSoundOnCapture(next);
            }}
          >
            <span className="knob" />
          </button>
        </div>

        <div className="section-title">General</div>
        <div className="setting-row">
          <span>
            <div className="label">Start at login</div>
            <div className="sub">Runs quietly in the tray</div>
          </span>
          <button
            className={`toggle${autostart ? " on" : ""}`}
            onClick={() => {
              const next = !autostart;
              setAutostart(next);
              void api.setAutostart(next).then(refresh);
            }}
          >
            <span className="knob" />
          </button>
        </div>
        <div className="setting-row">
          <span>
            <div className="label">Double-Shift capture</div>
            <div className="sub">Tap Shift twice to grab a selection</div>
          </span>
          <button
            className={`toggle${doubleShift ? " on" : ""}`}
            onClick={() => {
              const next = !doubleShift;
              setDoubleShift(next);
              void api.setCaptureDoubleShift(next);
            }}
          >
            <span className="knob" />
          </button>
        </div>
        <div className="setting-row">
          <span>
            <div className="label">Theme</div>
          </span>
          <div className="segmented">
            {["system", "light", "dark"].map((t) => (
              <button
                key={t}
                className={state.theme === t ? "on" : ""}
                onClick={() => {
                  void api.setTheme(t).then(refresh);
                }}
              >
                {t}
              </button>
            ))}
          </div>
        </div>

        <div className="section-title">Data</div>
        <div className="setting-row">
          <span>
            <div className="label">Export to Markdown</div>
            <div className="sub">{exportDir || "Documents"}</div>
          </span>
          <button
            className="btn"
            onClick={() => {
              void api.exportMarkdown().then((p) => {
                window.dispatchEvent(new CustomEvent("bard-toast", { detail: `Exported → ${p}` }));
              });
            }}
          >
            Export
          </button>
        </div>
        <div className="setting-row">
          <span>
            <div className="label">Export folder</div>
          </span>
          <button
            className="btn"
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
          </button>
        </div>
        <div className="setting-row">
          <span>
            <div className="label">Clear history</div>
            <div className="sub">Removes every captured item</div>
          </span>
          <button
            className="btn danger-btn"
            onClick={() => {
              void api.clearHistory().then(refresh);
            }}
          >
            Clear
          </button>
        </div>

        <div className="section-title">Shortcuts</div>
        {[
          ["capture", "Capture selection"],
          ["panel", "Toggle panel"],
          ["dictate", "Dictate"],
        ].map(([key, label]) => (
          <div className="keybind-row" key={key}>
            <span className="label" style={{ fontSize: 12.5, color: "var(--text)" }}>
              {label}
            </span>
            <button
              className={`keys${recording === key ? " recording" : ""}`}
              onClick={() => setRecording(recording === key ? null : key)}
              onKeyDown={(e) => {
                if (recording !== key) return;
                e.preventDefault();
                const mods: string[] = [];
                if (e.ctrlKey) mods.push("Ctrl");
                if (e.shiftKey) mods.push("Shift");
                if (e.altKey) mods.push("Alt");
                if (e.metaKey) mods.push("Super");
                const name =
                  e.key.length === 1
                    ? e.key.toUpperCase()
                    : e.key === " "
                      ? "Space"
                      : e.key;
                if (!mods.length && name !== "Space") return;
                const kb = { ...keybinds };
                kb[key] = `${mods.join("+")}+${name}`;
                saveKeybinds(kb);
                setRecording(null);
              }}

            >
              {recording === key ? "Press keys…" : (keybinds[key] ?? "—")}
            </button>
          </div>
        ))}
        <div className="sub" style={{ fontSize: 11, color: "var(--text-3)" }}>
          Double-tap Left Shift captures, Right Shift toggles the panel.
        </div>

        <button className="btn" style={{ width: "100%" }} onClick={onClose}>
          Done
        </button>
      </div>
    </div>
  );
}