import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, Check, Download, LoaderCircle, RotateCcw, Trash2, X } from "lucide-react";
import { api } from "../../store";
import type { ModelInfo, ModelProgress } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, InlineConfirm, Progress, formatBytes } from "../ui";

/*
  The two speech models Bard offers, and nothing else. Each is a file Bard
  hosts, pinned by size and hash, checked before it is ever used. There is no
  way to point Bard at an arbitrary model: a tool trusted with a microphone
  should only run what it can vouch for.
*/

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

function modelSize(model: ModelInfo): string {
  return formatBytes(model.sizeBytes ?? model.sizeMb * 1024 * 1024);
}

function formatEta(seconds?: number): string | null {
  if (seconds === undefined || !Number.isFinite(seconds) || seconds < 0) return null;
  if (seconds < 60) return `${Math.ceil(seconds)} sec left`;
  const minutes = Math.ceil(seconds / 60);
  return `${minutes} min left`;
}

/**
 * A download in progress, in the space the row's action button occupied: a
 * thin bar, a percentage with time left, and the way out.
 */
function InlineProgress({
  progress,
  onCancel,
  onRetry,
}: {
  progress: ModelProgress;
  onCancel: () => void;
  onRetry: () => void;
}) {
  const percent = Math.round(progress.percent);
  if (progress.error) {
    return (
      <div className="inline-progress failed">
        <span role="alert" title={progress.error}>Download failed</span>
        <Button onClick={onRetry}>
          <RotateCcw aria-hidden="true" />
          Retry
        </Button>
      </div>
    );
  }
  const eta = formatEta(progress.etaSeconds);
  return (
    <div className="inline-progress">
      <div className="inline-progress-copy">
        <span role="status">{progress.phase === "Verifying" ? "Verifying" : `${percent}%`}</span>
        {eta ? <span className="inline-progress-eta">{eta}</span> : null}
      </div>
      <Progress
        percent={progress.percent}
        label={`Downloading: ${percent} percent`}
        valueText={`${percent} percent${eta ? `, ${eta}` : ""}`}
      />
      <button
        type="button"
        className="icon-btn"
        aria-label="Cancel download"
        title="Cancel"
        onClick={onCancel}
      >
        <X aria-hidden="true" />
      </button>
    </div>
  );
}

export default function ModelsPane({
  state,
  refresh,
  progress,
  inventoryVersion,
}: PaneProps & { progress: ModelProgress | null; inventoryVersion: number }) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [modelsLoading, setModelsLoading] = useState(true);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [removeConfirmId, setRemoveConfirmId] = useState<string | null>(null);
  const [actionErrors, setActionErrors] = useState<Record<string, string>>({});

  const mountedRef = useRef(true);
  const modelsEpochRef = useRef(0);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const setActionError = useCallback((key: string, message: string | null) => {
    if (!mountedRef.current) return;
    setActionErrors((current) => {
      if (message === null) {
        if (!(key in current)) return current;
        const next = { ...current };
        delete next[key];
        return next;
      }
      return { ...current, [key]: message };
    });
  }, []);

  const loadModels = useCallback(async () => {
    const generation = ++modelsEpochRef.current;
    setModelsLoading(true);
    setModelsError(null);
    try {
      const next = await api.getModels();
      if (!mountedRef.current || generation !== modelsEpochRef.current) return;
      setModels(next);
    } catch (error) {
      if (!mountedRef.current || generation !== modelsEpochRef.current) return;
      setModelsError(errorText(error));
    } finally {
      if (mountedRef.current && generation === modelsEpochRef.current) setModelsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadModels();
  }, [inventoryVersion, loadModels, state.modelId, state.modelReady]);

  const activeProgress = progress && !progress.error && progress.percent < 100 ? progress : null;
  const busy = busyAction !== null;

  const downloadFirstParty = async (model: ModelInfo, activate: boolean) => {
    const key = `${model.id}:${activate ? "use" : "only"}`;
    if (busyAction || activeProgress) return;
    setBusyAction(key);
    setActionError(model.id, null);
    try {
      await api.downloadModel(model.id);
      // The Rust contract separates installation from activation. Use is a
      // second, verified step; Download only deliberately stops after install.
      if (activate) await api.setModel(model.id);
      refresh();
      await loadModels();
    } catch (error) {
      setActionError(model.id, errorText(error));
    } finally {
      if (mountedRef.current) setBusyAction(null);
    }
  };

  const useFirstParty = async (model: ModelInfo) => {
    if (busyAction) return;
    setBusyAction(`${model.id}:use`);
    setActionError(model.id, null);
    try {
      await api.setModel(model.id);
      refresh();
    } catch (error) {
      setActionError(model.id, errorText(error));
    } finally {
      if (mountedRef.current) setBusyAction(null);
    }
  };

  const removeFirstParty = async (model: ModelInfo) => {
    if (busyAction) return;
    setRemoveConfirmId(null);
    setBusyAction(`${model.id}:remove`);
    setActionError(model.id, null);
    try {
      await api.deleteModel(model.id);
      refresh();
      await loadModels();
    } catch (error) {
      setActionError(model.id, errorText(error));
    } finally {
      if (mountedRef.current) setBusyAction(null);
    }
  };

  return (
    <>
      <Group title="Speech model">
        {modelsLoading && models.length === 0 ? (
          <div className="loading-row" role="status">
            <LoaderCircle className="spinning" aria-hidden="true" />
            Loading models…
          </div>
        ) : modelsError && models.length === 0 ? (
          <div className="error-empty" role="alert">
            <AlertCircle aria-hidden="true" />
            <div>
              <strong>Models could not be loaded.</strong>
              <span>{modelsError}</span>
            </div>
            <Button onClick={() => void loadModels()}>Retry</Button>
          </div>
        ) : (
          <div className="model-list" role="radiogroup" aria-label="Transcription model">
            {models.map((model) => {
              const active = state.modelId === model.id;
              const downloading = progress?.id === model.id ? progress : null;
              const error = actionErrors[model.id];
              const facts = [
                model.languages === 1 ? "English" : `${model.languages} languages`,
                modelSize(model),
              ].join(" · ");
              const choose = () => {
                if (active || busy || downloading) return;
                if (model.downloaded) void useFirstParty(model);
              };
              return (
                <div key={model.id} className={`model-row${active ? " active" : ""}`}>
                  <div
                    className="model-choice"
                    role="radio"
                    aria-checked={active}
                    aria-disabled={!model.downloaded || busy}
                    tabIndex={active ? 0 : -1}
                    onClick={choose}
                    onKeyDown={(event) => {
                      if (event.key === " " || event.key === "Enter") {
                        event.preventDefault();
                        choose();
                      }
                    }}
                  >
                    <span className="model-radio" aria-hidden="true">
                      {active ? <Check strokeWidth={3} /> : null}
                    </span>
                    <span className="model-text">
                      <span className="model-line">
                        <span className="model-name">{model.name}</span>
                      </span>
                      <span className="model-desc">{model.description}</span>
                      <span className="model-facts">{facts}</span>
                    </span>
                  </div>
                  <div className="model-trailing">
                    {downloading ? (
                      <InlineProgress
                        progress={downloading}
                        onCancel={() =>
                          void api
                            .downloadModelCancel()
                            .catch((failure) => setActionError(model.id, errorText(failure)))
                        }
                        onRetry={() => void downloadFirstParty(model, true)}
                      />
                    ) : !model.downloaded ? (
                      <Button
                        onClick={() => void downloadFirstParty(model, true)}
                        disabled={busy || Boolean(activeProgress)}
                        ariaBusy={busyAction === `${model.id}:use`}
                        ariaLabel={`Get ${model.name}, ${modelSize(model)}`}
                      >
                        <Download aria-hidden="true" />
                        Get
                      </Button>
                    ) : active ? (
                      <span className="model-state">In use</span>
                    ) : (
                      <>
                        <button
                          type="button"
                          className="icon-btn model-remove"
                          aria-label={`Remove ${model.name}`}
                          title="Remove"
                          disabled={busy}
                          onClick={() => setRemoveConfirmId(model.id)}
                        >
                          <Trash2 aria-hidden="true" />
                        </button>
                        <Button
                          onClick={() => void useFirstParty(model)}
                          disabled={busy}
                          ariaBusy={busyAction === `${model.id}:use`}
                        >
                          Use
                        </Button>
                      </>
                    )}
                  </div>
                  {removeConfirmId === model.id ? (
                    <InlineConfirm
                      onCancel={() => setRemoveConfirmId(null)}
                      onConfirm={() => void removeFirstParty(model)}
                      confirming={busyAction === `${model.id}:remove`}
                      confirmLabel="Remove"
                    >
                      Remove {model.name}? You can download it again at any time.
                    </InlineConfirm>
                  ) : null}
                  {error ? <div className="inline-error" role="alert">{error}</div> : null}
                </div>
              );
            })}
          </div>
        )}
      </Group>

      <p className="pane-footnote">
        <a
          href="https://github.com/miisodev/bard-models/blob/main/NOTICE.md"
          target="_blank"
          rel="noreferrer"
        >
          Model licences and sources
        </a>
      </p>
    </>
  );
}
