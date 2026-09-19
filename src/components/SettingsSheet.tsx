import { Download, Mic, RotateCcw, Search } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, DEFAULT_KEYBINDS, setShortcutRecording } from "../store";
import type { AppState, CachedHfModel, HfFile, ModelInfo, ModelProgress } from "../types";

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
  const [countdown, setCountdown] = useState(3);
  const countdownRef = useRef<number | undefined>(undefined);
  const [keybinds, setKeybinds] = useState<Record<string, string>>({});
  const [autostart, setAutostart] = useState(state.autostartEnabled);
  const [doubleShift, setDoubleShift] = useState(state.captureDoubleShift);
  const [soundOnCapture, setSoundOnCapture] = useState(true);
  const [exportDir, setExportDir] = useState("");
  const [hfRepo, setHfRepo] = useState("");
  const [hfFiles, setHfFiles] = useState<HfFile[] | null>(null);
  const [hfBusy, setHfBusy] = useState(false);
  const [hfError, setHfError] = useState<string | null>(null);
  const [hfCached, setHfCached] = useState<CachedHfModel[]>([]);

  const stopCounting = () => {
    if (countdownRef.current) window.clearInterval(countdownRef.current);
    countdownRef.current = undefined;
    setShortcutRecording(false);
  };

  useEffect(() => stopCounting, []);

  const startRecording = (key: string) => {
    if (recording) stopCounting();
    setRecording(key);
    setCountdown(3);
    setShortcutRecording(true);
    countdownRef.current = window.setInterval(() => {
      setCountdown((c) => {
        if (c <= 1) {
          stopCounting();
          setRecording(null);
          return 0;
        }
        return c - 1;
      });
    }, 1000);
  };

  const keyLabel = (e: { key: string }): string | null => {
    if (e.key === " ") return "Space";
    if (/^F\d{1,2}$/.test(e.key)) return e.key;
    if (e.key.length === 1) return e.key.toUpperCase();
    switch (e.key) {
      case "Escape":
      case "Enter":
      case "Tab":
      case "Backspace":
      case "Delete":
      case "Insert":
      case "Home":
      case "End":
      case "PageUp":
      case "PageDown":
        return e.key;
      default:
        return null;
    }
  };

  const finishRecording = (
    key: string,
    e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }
  ) => {
    const label = keyLabel(e);
    if (!label) return;
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.shiftKey) mods.push("Shift");
    if (e.altKey) mods.push("Alt");
    if (e.metaKey) mods.push("Super");
    const combos = `${mods.join("+")}${mods.length ? "+" : ""}${label}`;
    if (!mods.length && !/^F\d{1,2}$/.test(label)) return;
    setKeybinds((kb) => {
      const next = { ...kb, [key]: combos };
      void api.setKeybinds(JSON.stringify(next));
      return next;
    });
    stopCounting();
    setRecording(null);
  };

  useEffect(() => {
    void api.getModels().then(setModels);
    void api.getMicrophones().then(setMicrophones);
    void api.getSoundOnCapture().then(setSoundOnCapture);
    void api.getExportDir().then(setExportDir);
    void api.getSettingStr("selected_microphone").then(setSelectedMic);
    void api.hfCachedModels().then(setHfCached);
    try {
      setKeybinds(JSON.parse(state.keybinds) as Record<string, string>);
    } catch {
      setKeybinds({});
    }
  }, []);

  const refresh = () => void api.getState().then(onState);

  const progress = modelProgress;

  const browseRepo = async () => {
    const repo = hfRepo.trim();
    if (!repo || hfBusy) return;
    setHfBusy(true);
    setHfError(null);
    try {
      const files = await api.hfListRepo(repo);
      setHfFiles(files);
    } catch (err) {
      setHfError(String(err));
      setHfFiles(null);
    } finally {
      setHfBusy(false);
    }
 };

  const mb = (b: number) => `${Math.round((b / 1024 / 1024) * 10) / 10} MB`;

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

        <div className="section-title">Hugging Face models</div>
        {hfCached.length > 0 ? (
          <div className="sub" style={{ fontSize: 11, color: "var(--text-3)", marginTop: 4 }}>
            Found in the shared Hugging Face cache on this machine — no download needed.
          </div>
        ) : null}
        {hfCached.map((c, i) => (
          <div className="model-card" key={`${c.repo}-${i}`}>
            <div className="model-head">
              <div style={{ minWidth: 0 }}>
                <div className="name" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                  {c.file}
                </div>
                <div className="meta" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                  {c.repo} · {mb(c.size)}
                </div>
              </div>
              <button
                className="btn primary"
                onClick={() => {
                  void api
                    .hfUseCached(c.repo, c.file, c.localPath, c.size)
                    .then(() => {
                      void api.getState().then(onState);
                      void api.getModels().then(setModels);
                      window.dispatchEvent(
                        new CustomEvent("bard-toast", { detail: "Using cached model" })
                      );
                    })
                    .catch((err) => setHfError(String(err)));
                }}
              >
                Use
              </button>
            </div>
          </div>
        ))}
        <div className="hf-add">
          <input
            className="field"
            placeholder="Repo — e.g. unslothai/Qwen3-ASR-0.6B-GGUF"
            value={hfRepo}
            onChange={(e) => {
              setHfRepo(e.target.value);
              setHfFiles(null);
              setHfError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") void browseRepo();
            }}
          />
          <button className="btn" disabled={!hfRepo.trim() || hfBusy} onClick={() => void browseRepo()}>
            <Search size={12} /> Files
          </button>
        </div>
        {hfBusy ? (
          <div className="sub" style={{ fontSize: 11, color: "var(--text-3)" }}>
            Listing repo…
          </div>
        ) : null}
        {hfError ? (
          <div className="sub" style={{ fontSize: 11, color: "var(--danger)" }}>
            {hfError}
          </div>
        ) : null}
        {hfFiles && !hfBusy ? (
          <div className="hf-files">
            {hfFiles.length === 0 ? (
              <div className="sub" style={{ fontSize: 11, color: "var(--text-3)" }}>
                No transcription models (.gguf / ggml-*.bin) in this repo.
              </div>
            ) : (
              hfFiles.map((f) => (
                <div className="keybind-row" key={f.path}>
                  <span
                    className="label"
                    style={{
                      fontSize: 12,
                      color: "var(--text)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {f.path}
                    <span className="sub" style={{ marginLeft: 6 }}>{mb(f.size)}</span>
                  </span>
                  <button
                    className="btn primary"
                    onClick={() => {
                      void api
                        .hfInstall(hfRepo.trim(), f.path, f.size, f.sha256)
                        .then(() => {
                          setHfFiles(null);
                          void api.getState().then(onState);
                          void api.getModels().then(setModels);
                        })
                        .catch((err) => setHfError(String(err)));
                    }}
                  >
                    <Download size={12} /> Install
                  </button>
                </div>
              ))
            )}
          </div>
        ) : null}
        {progress && progress.id.startsWith("hf|") ? (
          <div className="progress">
            <div className="fill" style={{ width: `${Math.min(100, progress.percent)}%` }} />
          </div>
        ) : null}

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
              onClick={(e) => {
                (e.currentTarget as HTMLButtonElement).focus();
                startRecording(key);
              }}
              onKeyDown={(e) => {
                if (recording !== key) return;
                e.preventDefault();
                e.stopPropagation();
                finishRecording(key, e);
              }}
            >
              {recording === key
                ? `Press keys…`
                : (keybinds[key] ?? DEFAULT_KEYBINDS[key] ?? "—")}
              {recording === key ? ` (${countdown})` : null}
            </button>
          </div>
        ))}
        <div className="setting-row">
          <span className="sub" style={{ fontSize: 11 }}>
            Reset all shortcuts to defaults
          </span>
          <button
            className="btn"
            onClick={() => {
              setKeybinds({ ...DEFAULT_KEYBINDS });
              void api.setKeybinds(JSON.stringify(DEFAULT_KEYBINDS));
            }}
          >
            <RotateCcw size={12} /> Defaults
          </button>
        </div>
        <div className="sub" style={{ fontSize: 11, color: "var(--text-3)" }}>
          You have 3 seconds to press a key combination. Double-tap Left Shift
          captures, Right Shift toggles the panel — these are fixed.
        </div>

        <button className="btn" style={{ width: "100%" }} onClick={onClose}>
          Done
        </button>
      </div>
    </div>
  );
}