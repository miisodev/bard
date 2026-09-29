import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertCircle, Check, Download, LoaderCircle, RotateCcw, Trash2, X } from "lucide-react";
import { api } from "../../store";
import type {
  BackendModelCompatibility,
  DetectedModel,
  HfFile,
  HfInstallResult,
  ModelCompatibility,
  ModelCompatibilityResult,
  ModelInfo,
  ModelProgress,
} from "../../types";
import type { PaneProps } from "../SettingsWindow";
import {
  Button,
  Group,
  InlineConfirm,
  Note,
  Pane,
  Progress,
  TextField,
  formatBytes,
} from "../ui";

/** The catalogue is three curated first-party models; a fourth is not one. */
const CURATED_LIMIT = 3;

type CompatibilityState = {
  compatibility: ModelCompatibility;
  message?: string;
};

type ParsedHfInput = {
  repo: string;
  requestedFile?: string;
  revision?: string;
};

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

function normalizeCompatibility(
  value: BackendModelCompatibility | undefined,
): ModelCompatibility {
  if (value === "verified") return "compatible";
  if (value === "unverified") return "unknown";
  if (
    value === "unknown" ||
    value === "checking" ||
    value === "compatible" ||
    value === "incompatible" ||
    value === "missing"
  ) {
    return value;
  }
  return "unknown";
}

function compatibilityFrom(
  value: BackendModelCompatibility | ModelCompatibilityResult | undefined,
): CompatibilityState {
  if (typeof value === "string") return { compatibility: normalizeCompatibility(value) };
  return {
    compatibility: normalizeCompatibility(value?.compatibility ?? value?.status),
    message: value?.compatibilityMessage ?? value?.message,
  };
}

function firstPartyModels(models: ModelInfo[]): ModelInfo[] {
  const explicitlyFirstParty = models.filter(
    (model) =>
      model.hosted === true ||
      Boolean(model.host) ||
      model.catalog === true ||
      model.origin?.toLowerCase() === "bard",
  );
  const compatibleWithOlderBackends = models.filter(
    (model) =>
      model.catalog !== false &&
      !model.id.startsWith("hf|") &&
      !["custom", "detected", "huggingface", "hugging-face"].includes(
        model.origin?.toLowerCase() ?? "",
      ),
  );
  const candidates =
    explicitlyFirstParty.length > 0 ? explicitlyFirstParty : compatibleWithOlderBackends;

  return candidates.slice(0, CURATED_LIMIT);
}

function modelSize(model: ModelInfo): string {
  return formatBytes(model.sizeBytes ?? model.sizeMb * 1024 * 1024);
}

function detectedKey(model: DetectedModel): string {
  return model.id || `${model.localPath}|${model.repo}|${model.file}`;
}

function parseHfInput(raw: string): ParsedHfInput | { error: string } {
  const value = raw.trim();
  if (!value) return { error: "Enter a Hugging Face repository ID or URL." };
  if (/^http:\/\//i.test(value)) {
    return { error: "Use an HTTPS Hugging Face URL." };
  }

  const isUrl = /^https?:\/\//i.test(value);
  const isBareHost = /^(?:www\.)?(?:huggingface|hf)\.co\//i.test(value);
  let parts: string[];
  try {
    if (isUrl) {
      const url = new URL(value);
      if (
        !/(^|\.)huggingface\.co$/i.test(url.hostname) &&
        !/(^|\.)hf\.co$/i.test(url.hostname)
      ) {
        return { error: "Use a huggingface.co repository or resolve URL." };
      }
      parts = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
    } else if (isBareHost) {
      parts = value.split("?")[0].split("#")[0].split("/").slice(1).map(decodeURIComponent);
    } else {
      parts = value.replace(/^@/, "").split("/").filter(Boolean);
    }
  } catch {
    return { error: "Enter a valid Hugging Face URL." };
  }

  if (parts[0]?.toLowerCase() === "models") parts = parts.slice(1);
  if (parts.length < 2 || parts[0] === "." || parts[0] === ".." || parts[1] === "." || parts[1] === "..") {
    return { error: "The URL does not contain a valid owner/repository ID." };
  }
  const repo = `${parts[0]}/${parts[1]}`;
  if (parts[2] === "resolve") {
    const revision = parts[3];
    const requestedFile = parts.slice(4).join("/");
    if (!revision || !requestedFile) {
      return { error: "The resolve URL does not include a revision and model file." };
    }
    if (revision !== "main") {
      return {
        error: "Bard imports the main branch. Open the model's main-branch resolve URL and try again.",
      };
    }
    return { repo, revision, requestedFile };
  }
  if (!isUrl && !isBareHost && parts.length !== 2) {
    return { error: "Use an ID such as owner/repository, or paste a Hugging Face URL." };
  }
  return { repo, revision: parts[2] === "tree" ? parts[3] : undefined };
}

function installIdFromResult(result: HfInstallResult): string | null {
  if (typeof result === "string" && result) return result;
  if (result && typeof result === "object" && typeof result.id === "string") return result.id;
  return null;
}

function hasInstallMetadata(file: HfFile): boolean {
  return file.size > 0 && /^[0-9a-f]{64}$/i.test(file.sha256);
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
  const [detected, setDetected] = useState<DetectedModel[]>([]);
  const [modelsLoading, setModelsLoading] = useState(true);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [detectedError, setDetectedError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [removeConfirmId, setRemoveConfirmId] = useState<string | null>(null);
  const [actionErrors, setActionErrors] = useState<Record<string, string>>({});

  const [repoInput, setRepoInput] = useState("");
  const [files, setFiles] = useState<HfFile[] | null>(null);
  const [scannedInput, setScannedInput] = useState<ParsedHfInput | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [fileChecks, setFileChecks] = useState<Record<string, CompatibilityState>>({});
  const [installErrors, setInstallErrors] = useState<Record<string, string>>({});
  const [installedFile, setInstalledFile] = useState<string | null>(null);

  const mountedRef = useRef(true);
  const modelsEpochRef = useRef(0);
  const detectedEpochRef = useRef(0);

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

  const checkDetectedModel = useCallback(
    async (model: DetectedModel, generation = detectedEpochRef.current) => {
      if (!mountedRef.current || generation !== detectedEpochRef.current) return;
      const key = detectedKey(model);
      setDetected((current) =>
        current.map((item) =>
          detectedKey(item) === key
            ? { ...item, compatibility: "checking", compatibilityMessage: undefined }
            : item,
        ),
      );
      setActionError(`detected:${key}`, null);
      try {
        const result = await api.checkModel(model.localPath);
        if (!mountedRef.current || generation !== detectedEpochRef.current) return;
        const compatibility = compatibilityFrom(result);
        setDetected((current) =>
          current.map((item) =>
            detectedKey(item) === key
              ? {
                  ...item,
                  compatibility: compatibility.compatibility,
                  compatibilityMessage: compatibility.message,
                }
              : item,
          ),
        );
      } catch (error) {
        if (!mountedRef.current || generation !== detectedEpochRef.current) return;
        const message = errorText(error);
        setDetected((current) =>
          current.map((item) =>
            detectedKey(item) === key
              ? {
                  ...item,
                  compatibility: "unknown",
                  compatibilityMessage: "Compatibility could not be checked.",
                }
              : item,
          ),
        );
        setActionError(`detected:${key}`, message);
      }
    },
    [setActionError],
  );

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

  const loadDetectedModels = useCallback(async () => {
    const generation = ++detectedEpochRef.current;
    setDetectedError(null);
    try {
      const next = await api.getDetectedModels();
      if (!mountedRef.current || generation !== detectedEpochRef.current) return;
      setDetected(next);
      for (const model of next) {
        if (
          model.compatibility === "unknown" ||
          model.compatibility === "unverified" ||
          model.compatibility === "checking"
        ) {
          // Opening several multi-gigabyte GGUFs in parallel can exhaust memory
          // and make every result slow. Compatibility probes run one at a time.
          await checkDetectedModel(model, generation);
        }
      }
    } catch (error) {
      if (!mountedRef.current || generation !== detectedEpochRef.current) return;
      setDetectedError(errorText(error));
    }
  }, [checkDetectedModel]);

  useEffect(() => {
    void loadModels();
    void loadDetectedModels();
  }, [inventoryVersion, loadDetectedModels, loadModels, state.modelId, state.modelReady]);

  const firstParty = useMemo(() => firstPartyModels(models), [models]);
  const requestedFiles = useMemo(() => {
    if (!files) return [];
    if (!scannedInput?.requestedFile) return files;
    return files.filter((file) => file.path === scannedInput.requestedFile);
  }, [files, scannedInput]);
  const activeProgress = progress && !progress.error && progress.percent < 100 ? progress : null;
  const checkingDetected = detected.some(
    (model) => compatibilityFrom(model.compatibility).compatibility === "checking",
  );
  const hfProgress =
    progress && (progress.id.startsWith("hf|") || files?.some((file) => file.path === progress.id))
      ? progress
      : null;

  const downloadFirstParty = async (model: ModelInfo, activate: boolean) => {
    const key = `${model.id}:${activate ? "use" : "only"}`;
    if (busyAction || checkingDetected || activeProgress) return;
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
    if (busyAction || checkingDetected) return;
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
    if (busyAction || checkingDetected) return;
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

  const activateDetected = async (model: DetectedModel) => {
    const key = detectedKey(model);
    if (busyAction || checkingDetected) return;
    setBusyAction(`detected-use:${key}`);
    setActionError(`detected:${key}`, null);
    try {
      await api.activateDetectedModel(model.id);
      refresh();
      await loadModels();
    } catch (error) {
      setActionError(`detected:${key}`, errorText(error));
    } finally {
      if (mountedRef.current) setBusyAction(null);
    }
  };

  const forgetDetected = async (model: DetectedModel) => {
    const key = detectedKey(model);
    if (busyAction) return;
    setBusyAction(`detected-forget:${key}`);
    setActionError(`detected:${key}`, null);
    try {
      await api.forgetDetectedModel(model.id);
      await loadDetectedModels();
      await loadModels();
      refresh();
    } catch (error) {
      setActionError(`detected:${key}`, errorText(error));
    } finally {
      if (mountedRef.current) setBusyAction(null);
    }
  };

  const scanRepository = async () => {
    if (scanning) return;
    const parsed = parseHfInput(repoInput);
    if ("error" in parsed) {
      setScanError(parsed.error);
      setFiles(null);
      setScannedInput(null);
      return;
    }

    setScanning(true);
    setScanError(null);
    setFiles(null);
    setScannedInput(null);
    setFileChecks({});
    setInstallErrors({});
    setInstalledFile(null);
    try {
      const listed = await api.hfListRepo(parsed.repo);
      const candidateFiles = listed.filter(
        (file) => file.compatibility !== "incompatible" && file.compatibility !== "missing",
      );
      if (!mountedRef.current) return;
      setScannedInput(parsed);
      setFiles(candidateFiles);
      setFileChecks(
        Object.fromEntries(
          candidateFiles.map((file) => [
            file.path,
            compatibilityFrom(file.compatibility ?? "unknown"),
          ]),
        ),
      );
    } catch (error) {
      if (mountedRef.current) setScanError(errorText(error));
    } finally {
      if (mountedRef.current) setScanning(false);
    }
  };

  const installFile = async (file: HfFile) => {
    if (!scannedInput || busyAction || activeProgress) return;
    const check = fileChecks[file.path] ?? compatibilityFrom(file.compatibility ?? "unknown");
    if (!hasInstallMetadata(file)) {
      setInstallErrors((current) => ({
        ...current,
        [file.path]: "This repository file has no complete size/checksum metadata and cannot be installed safely.",
      }));
      return;
    }
    if (check.compatibility === "incompatible" || check.compatibility === "missing") return;

    setBusyAction(`install:${file.path}`);
    setInstallErrors((current) => {
      const next = { ...current };
      delete next[file.path];
      return next;
    });
    try {
      const result = await api.hfInstall(
        scannedInput.repo,
        file.path,
        file.size,
        file.sha256,
      );
      const installedId = installIdFromResult(result);
      if (!installedId) {
        throw new Error("Bard installed the file but did not return its model identity.");
      }
      await api.setModel(installedId);
      setInstalledFile(file.path);
      setFileChecks((current) => ({
        ...current,
        [file.path]: { compatibility: "compatible" },
      }));
      refresh();
      await loadModels();
    } catch (error) {
      if (mountedRef.current) setInstallErrors((current) => ({ ...current, [file.path]: errorText(error) }));
    } finally {
      if (mountedRef.current) setBusyAction(null);
    }
  };

  const busy = Boolean(busyAction) || checkingDetected;
  const otherModels = detected.filter(
    (model) => compatibilityFrom(model.compatibility ?? "unknown").compatibility !== "missing",
  );

  return (
    <Pane
      title="Models"
      description="Bard turns speech into text on this PC. Your voice is never uploaded."
    >
      <Group>
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
            {firstParty.map((model) => {
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
                        ariaLabel={`Download ${model.name}, ${modelSize(model)}`}
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

      {otherModels.length > 0 ? (
        <Group
          title="Found on this PC"
          note="Models other apps have downloaded. Bard uses them in place and never deletes them."
        >
          {otherModels.map((model) => {
            const key = detectedKey(model);
            const compatibility = compatibilityFrom(model.compatibility ?? "unknown");
            const active = state.modelId === model.id;
            const error = actionErrors[`detected:${key}`];
            return (
              <div key={key} className={`model-row compact${active ? " active" : ""}`}>
                <span className="model-text">
                  <span className="model-name ellipsis" title={model.localPath}>
                    {model.name ?? model.file}
                  </span>
                  <span className="model-facts">
                    {[
                      formatBytes(model.size),
                      compatibility.compatibility === "checking"
                        ? "Checking…"
                        : compatibility.compatibility === "incompatible"
                          ? "Not supported"
                          : null,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </span>
                <div className="model-trailing">
                  {active ? (
                    <span className="model-state">In use</span>
                  ) : compatibility.compatibility === "compatible" ? (
                    <Button
                      onClick={() => void activateDetected(model)}
                      disabled={busy}
                      ariaBusy={busyAction === `detected-use:${key}`}
                    >
                      Use
                    </Button>
                  ) : compatibility.compatibility !== "checking" &&
                    compatibility.compatibility !== "incompatible" ? (
                    <Button onClick={() => void checkDetectedModel(model)} disabled={busy}>
                      Check
                    </Button>
                  ) : null}
                  {model.registered && !active ? (
                    <button
                      type="button"
                      className="icon-btn model-remove"
                      aria-label={`${model.owned ? "Remove" : "Forget"} ${model.file}`}
                      title={model.owned ? "Remove" : "Forget"}
                      disabled={busy}
                      onClick={() => void forgetDetected(model)}
                    >
                      <X aria-hidden="true" />
                    </button>
                  ) : null}
                </div>
                {error ? <div className="inline-error" role="alert">{error}</div> : null}
              </div>
            );
          })}
        </Group>
      ) : detectedError ? (
        <Note tone="danger">Local models could not be scanned: {detectedError}</Note>
      ) : null}

      <details className="model-disclosure">
        <summary>
          <span>
            <strong>Add a model from Hugging Face</strong>
            <small>For people who know which GGUF file they want.</small>
          </span>
        </summary>
        <div className="disclosure-body">
          <div className="repo-form">
            <TextField
              id="hf-model-source"
              label="Hugging Face repository or file URL"
              value={repoInput}
              placeholder="owner/repository"
              autoComplete="off"
              spellCheck={false}
              onChange={(next) => {
                setRepoInput(next);
                setFiles(null);
                setScannedInput(null);
                setScanError(null);
                setInstallErrors({});
                setInstalledFile(null);
              }}
              onEnter={() => void scanRepository()}
            />
            <Button
              onClick={() => void scanRepository()}
              disabled={!repoInput.trim() || scanning || Boolean(activeProgress)}
              ariaBusy={scanning}
            >
              {scanning ? "Looking…" : "Look up"}
            </Button>
          </div>
          <p className="import-intro">
            Bard checks each file's size and checksum before downloading, and makes sure it
            loads before switching to it.
          </p>

          {scanError ? <Note tone="danger">{scanError}</Note> : null}

          {files ? (
            <div className="repo-results" aria-live="polite">
              {requestedFiles.length === 0 ? (
                <Note>
                  {scannedInput?.requestedFile
                    ? "That file is not in the repository any more."
                    : "No supported model files in that repository."}
                </Note>
              ) : (
                requestedFiles.map((file) => {
                  const check = fileChecks[file.path] ?? compatibilityFrom(file.compatibility ?? "unknown");
                  const fileProgress = hfProgress?.id.endsWith(file.path) ? hfProgress : null;
                  const installing = busyAction === `install:${file.path}`;
                  const error = installErrors[file.path];
                  const metadataReady = hasInstallMetadata(file);
                  const installed = installedFile === file.path;
                  return (
                    <div key={file.path} className="model-row compact">
                      <span className="model-text">
                        <span className="model-name ellipsis" title={file.path}>{file.path}</span>
                        <span className="model-facts">
                          {[
                            formatBytes(file.size),
                            file.quant,
                            !metadataReady ? "No checksum, so Bard will not install it" : null,
                            check.compatibility === "incompatible" ? "Not supported" : null,
                          ]
                            .filter(Boolean)
                            .join(" · ")}
                        </span>
                      </span>
                      <div className="model-trailing">
                        {fileProgress ? (
                          <InlineProgress
                            progress={fileProgress}
                            onCancel={() =>
                              void api
                                .downloadModelCancel()
                                .catch((failure) => setScanError(errorText(failure)))
                            }
                            onRetry={() => void installFile(file)}
                          />
                        ) : installed ? (
                          <span className="model-state">In use</span>
                        ) : (
                          <Button
                            onClick={() => void installFile(file)}
                            disabled={
                              !metadataReady ||
                              (check.compatibility !== "compatible" &&
                                check.compatibility !== "unknown") ||
                              installing ||
                              busy ||
                              Boolean(activeProgress)
                            }
                            ariaBusy={installing}
                          >
                            {installing ? "Installing…" : "Get"}
                          </Button>
                        )}
                      </div>
                      {error ? <div className="inline-error" role="alert">{error}</div> : null}
                    </div>
                  );
                })
              )}
            </div>
          ) : null}
        </div>
      </details>

      <p className="pane-footnote">
        Bard's models are open-weight speech models, converted and hosted by Bard.{" "}
        <a
          href="https://github.com/miisodev/bard-models/blob/main/NOTICE.md"
          target="_blank"
          rel="noreferrer"
        >
          Licences and sources
        </a>
      </p>
    </Pane>
  );
}
