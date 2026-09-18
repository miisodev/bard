import { useState } from "react";
import { api } from "../store";
import type { AppState, ModelInfo, ModelProgress } from "../types";

const WAVE = (
  <svg viewBox="0 0 42 42">
    <rect x="4" y="16" width="7" height="10" rx="3.5" opacity="0.85" />
    <rect x="16" y="6" width="7" height="30" rx="3.5" />
    <rect x="28" y="12" width="7" height="18" rx="3.5" opacity="0.9" />
  </svg>
);

export default function Onboarding({
  state,
  modelProgress,
  onDone,
}: {
  state: AppState;
  modelProgress: ModelProgress | null;
  onDone: () => void;
}) {
  const [step, setStep] = useState(0);
  const [model, setModel] = useState<ModelInfo | null>(null);
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [apiKey, setApiKey] = useState("");
  const [aiModel, setAiModel] = useState("");
  const [autostart, setAutostart] = useState(true);

  useState(() => {
    void api.getModels().then((ms) => {
      const picked = ms.find((m) => m.id === state.modelId) ?? ms[0];
      setModel(picked);
    });
    return 0;
  });

  const modelReady = model?.downloaded ?? false;
  const installing = modelProgress !== null;

  return (
    <div className="onboarding">
      <div className="mark">{WAVE}</div>
      {step === 0 ? (
        <>
          <h1>Bard</h1>
          <p>
            Capture anything you copy. Dictate offline with your own voice. Ask an AI
            anything — all from one minimal panel on your desktop.
          </p>
        </>
      ) : step === 1 ? (
        <>
          <h1>Offline voice</h1>
          <p>
            Bard dictates with a local Whisper model — nothing leaves your machine.
            One download, works everywhere.
          </p>
          <div className="model-card">
            <div className="name">{model?.name ?? "Whisper Small"}</div>
            <div className="meta">
              {modelReady ? "Installed and ready" : `${model?.sizeMb ?? 465} MB · runs fully offline`}
            </div>
            {installing ? (
              <div className="progress">
                <div className="fill" style={{ width: `${Math.min(100, modelProgress.percent)}%` }} />
              </div>
            ) : !modelReady ? (
              <button
                className="btn primary"
                onClick={() => void api.downloadModel(model!.id)}
              >
                Download {model?.sizeMb ?? 465} MB
              </button>
            ) : null}
          </div>
          {!installing ? (
            <button className="btn" onClick={() => setStep(2)}>
              {modelReady ? "Continue" : "Skip for now"}
            </button>
          ) : null}
        </>
      ) : step === 2 ? (
        <>
          <h1>Your AI</h1>
          <p>
            Optional. Connect any OpenAI-compatible provider to ask questions with
            captured context. Your key stays on this machine.
          </p>
          <input
            className="field"
            placeholder="Base URL"
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
          <div className="actions">
            <button
              className="btn primary"
              disabled={!aiModel.trim() || !apiKey.trim()}
              onClick={() => {
                void api.setProvider(baseUrl.trim(), apiKey.trim(), aiModel.trim()).then(() =>
                  setStep(3)
                );
              }}
            >
              Connect
            </button>
            <button className="btn" onClick={() => setStep(3)}>
              Skip
            </button>
          </div>
        </>
      ) : (
        <>
          <h1>You're set</h1>
          <p>
            Tap <kbd>Shift</kbd> <kbd>Shift</kbd> to capture a selection, hold{" "}
            <kbd>Ctrl</kbd> <kbd>Space</kbd> to dictate, and open the panel with{" "}
            <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Space</kbd>.
          </p>
          <div className="setting-row">
            <span>
              <div className="label">Start Bard at login</div>
              <div className="sub">Runs quietly in the tray</div>
            </span>
            <button
              className={`toggle${autostart ? " on" : ""}`}
              onClick={() => setAutostart(!autostart)}
            >
              <span className="knob" />
            </button>
          </div>
          <div className="actions">
            <button
              className="btn primary"
              onClick={() => {
                if (autostart) void api.setAutostart(true);
                onDone();
              }}
            >
              Start using Bard
            </button>
          </div>
        </>
      )}
      <div className="steps">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className={`step${i <= step ? " on" : ""}`} />
        ))}
      </div>
    </div>
  );
}