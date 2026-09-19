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
            Capture anything you copy. Dictate offline with your own voice. Everything
            you capture stays on this machine, in one minimal panel.
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
              {modelReady
                ? "Installed and ready"
                : `${model?.sizeMb ?? 465} MB · runs fully offline`}
            </div>
            {installing ? (
              <div className="progress">
                <div
                  className="fill"
                  style={{ width: `${Math.min(100, modelProgress.percent)}%` }}
                />
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
        </>
      )}

      <div className="steps">
        {[0, 1, 2].map((i) => (
          <div key={i} className={`step${i <= step ? " on" : ""}`} />
        ))}
      </div>

      <div className="actions">
        {step === 0 ? (
          <button className="btn primary" onClick={() => setStep(1)}>
            Continue
          </button>
        ) : step === 1 ? (
          installing ? null : (
            <>
              {modelReady ? (
                <button className="btn primary" onClick={() => setStep(2)}>
                  Continue
                </button>
              ) : null}
              <button className="btn" onClick={() => setStep(2)}>
                Skip for now
              </button>
            </>
          )
        ) : (
          <button
            className="btn primary"
            onClick={() => {
              if (autostart) void api.setAutostart(true);
              onDone();
            }}
          >
            Start using Bard
          </button>
        )}
      </div>
    </div>
  );
}