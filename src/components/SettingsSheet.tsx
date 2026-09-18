import { useEffect, useState } from "react";
import { api, getProvider } from "../store";
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
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [aiModel, setAiModel] = useState("");
  const [recording, setRecording] = useState<string | null>(null);
  const [keybinds, setKeybinds] = useState<Record<string, string>>({});
  const [autostart, setAutostart] = useState(state.autostartEnabled);
  const [doubleShift, setDoubleShift] = useState(state.captureDoubleShift);

  useEffect(() => {
    void api.getModels().then(setModels);
    void getProvider().then((p) => {
      setBaseUrl(p.baseUrl);
      setApiKey(p.apiKey);
      setAiModel(p.model);
    });
    try {
      setKeybinds(JSON.parse(state.keybinds) as Record<string, string>);
    } catch {
      setKeybinds({});
    }
  }, []);

  const saveKeybinds = (kb: Record<string, string>) => {
    setKeybinds(kb);
    void api.setKeybinds(JSON.stringify(kb));
  };

  const model = models.find((m) => m.id === state.modelId) ?? models[0];
  const progress = modelProgress;

  return (
    <div className="sheet" onClick={onClose}>
      <div className="sheet-card" onClick={(e) => e.stopPropagation()}>
        <h2>Settings</h2>

        <div className="section-title">Voice</div>
        <div className="model-card">
          <div className="name">{model?.name ?? "Whisper Small"}</div>
          <div className="meta">
            {model?.downloaded ? "Installed" : `${model?.sizeMb ?? 465} MB — offline dictation`}
          </div>
          {progress ? (
            <div className="progress">
              <div
                className="fill"
                style={{ width: `${Math.min(100, progress.percent)}%` }}
              />
            </div>
          ) : model && !model.downloaded ? (
            <button
              className="btn primary"
              onClick={() => void api.downloadModel(model.id)}
            >
              Download model
            </button>
          ) : null}
        </div>

        <div className="section-title">AI provider</div>
        <input
          className="field"
          placeholder="Base URL — e.g. https://api.openai.com/v1"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
        />
        <input
          className="field"
          type="password"
          placeholder="API key"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
        />
        <input
          className="field"
          placeholder="Model — e.g. gpt-4o-mini"
          value={aiModel}
          onChange={(e) => setAiModel(e.target.value)}
        />
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button
            className="btn primary"
            disabled={!baseUrl.trim() && !aiModel.trim()}
            onClick={() => {
              void api.setProvider(baseUrl.trim(), apiKey.trim(), aiModel.trim()).then(() => {
                void api.getState().then(onState);
              });
            }}
          >
            Save provider
          </button>
          {state.providerConfigured ? (
            <button
              className="btn"
              onClick={() => {
                void api.clearProvider().then(() => void api.getState().then(onState));
              }}
            >
              Clear
            </button>
          ) : null}
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
              void api.setAutostart(next).then(() => void api.getState().then(onState));
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
                  void api.setTheme(t).then(() => void api.getState().then(onState));
                }}
              >
                {t}
              </button>
            ))}
          </div>
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
              onClick={() => {
                setRecording(recording === key ? null : key);
                return;
              }}
              onKeyDown={(e) => {
                if (recording !== key) return;
                e.preventDefault();
                const mods: string[] = [];
                if (e.ctrlKey) mods.push("Ctrl");
                if (e.shiftKey) mods.push("Shift");
                if (e.altKey) mods.push("Alt");
                if (e.metaKey) mods.push("Super");
                const name = e.key.length === 1 ? e.key.toUpperCase() : e.key === " " ? "Space" : e.key;
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