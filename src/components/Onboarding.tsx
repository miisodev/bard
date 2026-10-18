import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { ArrowRight, ChevronLeft, Download, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "../store";
import LicenseCard from "./LicenseCard";
import type { AppState, ModelInfo, ModelProgress } from "../types";

const WAVE = (
  <svg viewBox="0 0 42 42" aria-hidden="true">
    <rect x="4" y="16" width="7" height="10" rx="3.5" opacity="0.85" />
    <rect x="16" y="6" width="7" height="30" rx="3.5" />
    <rect x="28" y="12" width="7" height="18" rx="3.5" opacity="0.9" />
  </svg>
);

const STEPS = 4;

/** What each step calls itself, for the announcement when it arrives. */
const STEP_TITLES = [
  "Clipboard, Draw and Dictate",
  "Offline voice",
  "Make it yours",
  "You're set",
];

/**
 * The step you are on, in a lighter tint; the steps behind you are solid. The
 * segments used to light for `index <= step`, so at the last step all four
 * were lit and the current one could not be picked out.
 */
const CURRENT_STEP: CSSProperties = { background: "var(--accent-soft)", width: 30 };

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

function preferredModel(models: ModelInfo[], activeId: string): ModelInfo | null {
  return (
    models.find((model) => model.id === activeId) ??
    models.find((model) => model.role?.toLowerCase().includes("fast")) ??
    models.find((model) => model.recommended) ??
    models[0] ??
    null
  );
}

export default function Onboarding({
  state,
  modelProgress,
  onDone,
}: {
  state: AppState;
  modelProgress: ModelProgress | null;
  onDone: () => void | Promise<void>;
}) {
  const [step, setStep] = useState(0);
  const [model, setModel] = useState<ModelInfo | null>(null);
  const [modelsLoading, setModelsLoading] = useState(true);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [modelsAttempt, setModelsAttempt] = useState(0);
  const [completedModelId, setCompletedModelId] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [activating, setActivating] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);
  const [autostart, setAutostart] = useState(state.autostartEnabled);
  const [finishing, setFinishing] = useState(false);
  const [finishError, setFinishError] = useState<string | null>(null);
  const [license, setLicense] = useState(state.license);
  const [announcement, setAnnouncement] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const primaryRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    let alive = true;
    setModelsLoading(true);
    setModelsError(null);
    void api
      .getModels()
      .then((models) => {
        if (!alive) return;
        setModel(preferredModel(models, state.modelId));
      })
      .catch((reason: unknown) => {
        if (alive) setModelsError(errorText(reason));
      })
      .finally(() => {
        if (alive) setModelsLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [modelsAttempt, state.modelId]);

  const modelReady =
    model !== null && (model.downloaded || completedModelId === model.id);
  const modelActive = modelReady && state.modelId === model.id;
  const progressForModel = modelProgress?.id === model?.id ? modelProgress : null;
  const activeProgress = progressForModel && !progressForModel.error ? progressForModel : null;
  const displayInstallError = installError ?? progressForModel?.error ?? null;
  const working = installing || activating;

  /*
    Every step change lands focus on that step's primary action and has the
    live region say which step arrived. Without it each step change unmounted
    the control that had focus — Continue, Back, Download — and focus fell to
    <body>, so the next Tab restarted at the top of the window. The arriving
    primary also takes focus when it replaces another mid-step: the download
    swaps itself for Continue.
  */
  const bindPrimary = useCallback((node: HTMLButtonElement | null) => {
    primaryRef.current = node;
    if (!node) return;
    const active = document.activeElement;
    const root = rootRef.current;
    // Someone is already working inside this screen on something else; the
    // arriving primary must not snatch focus from them.
    if (active instanceof HTMLElement && root && active !== root && root.contains(active)) return;
    node.focus();
  }, []);

  useEffect(() => {
    // A step whose primary is not up yet — the download, mid-flight — still
    // keeps focus inside the screen rather than dropping it on <body>.
    if (!primaryRef.current) rootRef.current?.focus();
    setAnnouncement(`Step ${step + 1} of ${STEPS} — ${STEP_TITLES[step]}`);
  }, [step]);

  const installModel = async () => {
    if (!model || working) return;
    setInstalling(true);
    setInstallError(null);
    try {
      await api.downloadModel(model.id);
      await api.setModel(model.id);
      setCompletedModelId(model.id);
      const models = await api.getModels();
      const refreshed = models.find((item) => item.id === model.id);
      if (refreshed) setModel(refreshed);
    } catch (reason) {
      setInstallError(errorText(reason));
    } finally {
      setInstalling(false);
    }
  };

  const activateAndContinue = async () => {
    if (!model || !modelReady || working) return;
    setActivating(true);
    setInstallError(null);
    try {
      if (!modelActive) await api.setModel(model.id);
      setCompletedModelId(model.id);
      setStep(2);
    } catch (reason) {
      setInstallError(errorText(reason));
    } finally {
      setActivating(false);
    }
  };

  const finish = async () => {
    if (finishing) return;
    setFinishing(true);
    setFinishError(null);
    try {
      // The 14-day trial is the default path, not a decision the user has to
      // make to leave this screen. Per the blueprint, "Skipping key entry starts
      // the 14-day local trial and never consumes a paid activation" — so
      // arriving here without a licence starts one rather than raising an error
      // at someone who has not yet decided about money.
      if (!license.canUse) {
        await api.startTrial();
      }

      // Autostart is not the app's to set on the Microsoft Store build, where
      // Windows owns it and `set_autostart` reports that as an error. That is a
      // channel fact, not a failure, and treating it as fatal left Store users
      // stuck on the last onboarding screen with an error that reads like a bug
      // report. A genuine failure to register autostart on the direct channel is
      // still worth surfacing.
      try {
        await api.setAutostart(autostart);
      } catch (reason) {
        if (!/store/i.test(errorText(reason))) throw reason;
      }

      await onDone();
    } catch (reason) {
      setFinishError(errorText(reason));
    } finally {
      setFinishing(false);
    }
  };

  return (
    // The screen itself is a focus fallback: a step whose primary is mid-flight
    // hands focus here rather than letting it fall to <body>.
    <div className="onboarding" ref={rootRef} tabIndex={-1}>
      <div className="mark">{WAVE}</div>

      {/* The promise. One column on a side edge; on a band it sits beside the
          body instead of above it, so a 250px-tall window keeps its room. */}
      <div className="onb-copy">
        {step === 0 ? (
          <>
            {/* The brand is the mark above this. The headline is the promise. */}
            <h1>Clipboard, Draw and Dictate.</h1>
            <p>
              Bard catches anything you copy, turns your voice into text without
              sending audio anywhere, and keeps both on this machine. Refine is
              always opt-in.
            </p>
          </>
        ) : step === 1 ? (
          <>
            <h1>Offline voice</h1>
            <p>
              {model
                ? `${model.name} runs here, on this machine. Nothing in this step leaves it.`
                : "One model runs here, on this machine. Nothing in this step leaves it."}
            </p>
          </>
        ) : step === 2 ? (
          <>
            <h1>Make it yours</h1>
            <p>Two taps of a modifier do the everyday work, in any app. These gestures are fixed and always available.</p>
          </>
        ) : (
          <>
            <h1>You're set</h1>
            <p>Start with a 14-day local trial, or enter the licence key from your purchase email.</p>
          </>
        )}
      </div>

      {/* The one step being asked about. */}
      <div className="onb-step">
        {step === 1 ? (
          <div className="model-card">
            <div className="name">{model?.name ?? (modelsLoading ? "Preparing model…" : "Voice model")}</div>
            <div className="meta">
              {modelsLoading
                ? "Checking the Bard model catalogue…"
                : modelsError
                  ? "The model catalogue could not be loaded."
                  : modelReady
                    ? modelActive
                      ? "Installed, active, and ready"
                      : "Installed — activate it to continue"
                    : model
                      ? `${model.sizeMb} MB · runs fully offline`
                      : "No downloadable model is available"}
            </div>
            {activeProgress || installing ? (
              <div
                className="progress"
                role="progressbar"
                aria-label="Voice model installation"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(activeProgress?.percent ?? 0)}
              >
                <div
                  className="fill"
                  style={{ width: `${Math.max(0, Math.min(100, activeProgress?.percent ?? 0))}%` }}
                />
              </div>
            ) : null}
            {displayInstallError ? <div className="meta" role="alert">{displayInstallError}</div> : null}
            {modelsError ? (
              <button
                ref={bindPrimary}
                className="btn"
                onClick={() => setModelsAttempt((attempt) => attempt + 1)}
                disabled={modelsLoading}
              >
                <RotateCcw aria-hidden="true" />
                Retry catalogue
              </button>
            ) : !modelReady && model && !working ? (
              <button
                ref={bindPrimary}
                className="btn primary"
                onClick={() => void installModel()}
                disabled={Boolean(activeProgress)}
              >
                {displayInstallError ? <RotateCcw aria-hidden="true" /> : <Download aria-hidden="true" />}
                {/* It activates as well as downloads, which "Download" alone
                    did not say — the same wording the Models pane uses. */}
                {displayInstallError
                  ? "Retry download & use"
                  : `Download & use ${model.sizeMb} MB`}
              </button>
            ) : null}
          </div>
        ) : null}

        {step === 2 ? (
          <div className="shortcut-list" aria-label="Bard keyboard shortcuts">
            <div><kbd>Left Shift</kbd><kbd>Left Shift</kbd><span>Capture</span></div>
            <div><kbd>Right Shift</kbd><kbd>Right Shift</kbd><kbd>1 / 2 / 3</kbd><span>Paste item</span></div>
            <div><kbd>Right Shift</kbd><kbd>Right Shift</kbd><span>Panel</span></div>
            <div><kbd>Right Alt</kbd><kbd>Right Alt</kbd><span>Settings</span></div>
            <div><kbd>Left Alt</kbd><kbd>Left Alt</kbd><span>Draw</span></div>
            <div><kbd>Ctrl</kbd><kbd>Ctrl</kbd><span>Dictate — hold to talk</span></div>
          </div>
        ) : null}

        {step === 3 ? (
          <>
            <LicenseCard status={license} onStatus={setLicense} compact />
            <div className="setting-row">
              <span>
                <div className="label">Start Bard at login</div>
                <div className="sub">Runs quietly in the tray</div>
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={autostart}
                aria-label="Start Bard at login"
                className={`toggle${autostart ? " on" : ""}`}
                onClick={() => setAutostart((current) => !current)}
              >
                <span className="knob" />
              </button>
            </div>
            {finishError ? <div className="meta" role="alert">{finishError}</div> : null}
          </>
        ) : null}
      </div>

      <div className="onb-foot">
        {/*
          The position is announced once, by the bar. The segments are pictures of
          it, so they are hidden from assistive technology — and `aria-current`
          was on a `<div>` with no role, which named nothing.
        */}
        <div
          className="steps"
          role="progressbar"
          aria-label="Onboarding progress"
          aria-valuemin={1}
          aria-valuemax={STEPS}
          aria-valuenow={step + 1}
          aria-valuetext={`Step ${step + 1} of ${STEPS}`}
        >
          {Array.from({ length: STEPS }, (_, index) => (
            <span
              key={index}
              className={`step${index < step ? " on" : ""}`}
              aria-hidden="true"
              style={index === step ? CURRENT_STEP : undefined}
            />
          ))}
        </div>

        <div className="actions">
          {/* Every step after the first can be left the way it came, including the
              one that downloads two hundred megabytes. */}
          {step > 0 ? (
            <button
              className="btn"
              onClick={() => setStep((current) => current - 1)}
              disabled={working || finishing}
            >
              <ChevronLeft aria-hidden="true" />
              Back
            </button>
          ) : null}
          {step === 0 ? (
            <button ref={bindPrimary} className="btn primary" onClick={() => setStep(1)}>
              Continue
              <ArrowRight aria-hidden="true" />
            </button>
          ) : step === 1 ? (
            working ? null : (
              <>
                {modelReady ? (
                  <button
                    ref={bindPrimary}
                    className="btn primary"
                    onClick={() => void activateAndContinue()}
                    disabled={activating}
                  >
                    {activating ? <LoaderCircle aria-hidden="true" /> : <ArrowRight aria-hidden="true" />}
                    {activating ? "Activating…" : "Continue"}
                  </button>
                ) : null}
                <button className="btn" onClick={() => setStep(2)} disabled={installing || activating}>
                  Skip for now
                </button>
              </>
            )
          ) : step === 2 ? (
            <button ref={bindPrimary} className="btn primary" onClick={() => setStep(3)}>
              Continue
              <ArrowRight aria-hidden="true" />
            </button>
          ) : (
            <button ref={bindPrimary} className="btn primary" onClick={() => void finish()} disabled={finishing}>
              {finishing ? <LoaderCircle aria-hidden="true" /> : <ArrowRight aria-hidden="true" />}
              {finishing ? "Starting Bard…" : "Start using Bard"}
            </button>
          )}
        </div>
      </div>

      {/*
        What each step arriving is called, for a screen reader. The bar above
        states the position; this states the step, once, as it changes.
      */}
      <div className="sr-only" role="status" aria-live="polite">
        {announcement}
      </div>
    </div>
  );
}
