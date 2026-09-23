import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  Check,
  CheckCircle2,
  Cloud,
  Download,
  HardDrive,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Trash2,
  X,
} from "lucide-react";
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
  Meter,
  Note,
  Pane,
  Progress,
  Tag,
  TextField,
  formatBytes,
} from "../ui";

const ROLE_LABELS = ["Fast multilingual", "Best English", "Best multilingual"] as const;

type CompatibilityState = {
  compatibility: ModelCompatibility;
  message?: string;
};

type ParsedHfInput = {
  repo: string;
  requestedFile?: string;
  revision?: string;
};

type FirstPartyModel = ModelInfo & { roleLabel: string };

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

function roleLabel(role: string | undefined, index: number): string {
  const normalized = role?.toLowerCase() ?? "";
  if (normalized.includes("fast")) return ROLE_LABELS[0];
  if (normalized.includes("english")) return ROLE_LABELS[1];
  if (normalized.includes("multilingual")) return ROLE_LABELS[2];
  return ROLE_LABELS[index] ?? ROLE_LABELS[0];
}

function firstPartyModels(models: ModelInfo[]): FirstPartyModel[] {
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

  return candidates.slice(0, ROLE_LABELS.length).map((model, index) => ({
    ...model,
    roleLabel: roleLabel(model.role, index),
  }));
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

function formatSpeed(bytesPerSecond?: number): string | null {
  if (!bytesPerSecond || !Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return null;
  if (bytesPerSecond >= 1024 ** 2) {
    return `${Math.round((bytesPerSecond / 1024 ** 2) * 10) / 10} MB/s`;
  }
  return `${Math.round(bytesPerSecond / 1024)} KB/s`;
}

function formatEta(seconds?: number): string | null {
  if (seconds === undefined || !Number.isFinite(seconds) || seconds < 0) return null;
  if (seconds < 60) return `${Math.ceil(seconds)} sec left`;
  const minutes = Math.ceil(seconds / 60);
  return `${minutes} min left`;
}

function ProgressDetails({
  progress,
  onCancel,
  onRetry,
}: {
  progress: ModelProgress;
  onCancel: () => void;
  onRetry: () => void;
}) {
  const speed = formatSpeed(progress.speedBytesPerSecond);
  const eta = formatEta(progress.etaSeconds);
  const bytes =
    progress.total > 0
      ? `${formatBytes(progress.received)} of ${formatBytes(progress.total)}`
      : formatBytes(progress.received);
  const phase = progress.phase ?? (progress.error ? "Download failed" : "Downloading");

  return (
    <div className={`model-progress${progress.error ? " has-error" : ""}`}>
      <div className="progress-copy" aria-live="polite">
        <span>{progress.message ?? phase}</span>
        <span className="progress-measure">
          {[bytes, speed, eta].filter(Boolean).join(" · ")}
        </span>
      </div>
      <Progress
        percent={progress.percent}
        label={`${phase}: ${Math.round(progress.percent)} percent`}
        valueText={progress.error ? `${phase}. ${progress.error}` : `${phase}, ${Math.round(progress.percent)} percent`}
      />
      <div className="progress-actions">
        {progress.error ? (
          <Button onClick={onRetry} disabled={false}>
            <RotateCcw aria-hidden="true" />
            Retry
          </Button>
        ) : (
          <Button onClick={onCancel} disabled={false}>
            <X aria-hidden="true" />
            Cancel
          </Button>
        )}
      </div>
      {progress.error ? <div className="inline-error">{progress.error}</div> : null}
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
  const [detectedLoading, setDetectedLoading] = useState(true);
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
    setDetectedLoading(true);
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
    } finally {
      if (mountedRef.current && generation === detectedEpochRef.current) setDetectedLoading(false);
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

  const downloadFirstParty = async (model: FirstPartyModel, activate: boolean) => {
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

  const verifiedFileCount = requestedFiles.filter(
    (file) =>
      (fileChecks[file.path] ?? compatibilityFrom(file.compatibility ?? "unknown"))
        .compatibility === "compatible",
  ).length;
  const filesAwaitingVerification = requestedFiles.length - verifiedFileCount;

  return (
    <Pane title="Models" description="Choose what Bard uses for local transcription.">
      <Group
        title="Bard models"
        note="Three curated first-party choices. Quality and speed are relative to this list, not benchmark scores."
      >
        {modelsLoading && models.length === 0 ? (
          <div className="loading-row" role="status">
            <LoaderCircle className="spinning" aria-hidden="true" />
            Loading Bard models…
          </div>
        ) : modelsError && models.length === 0 ? (
          <div className="error-empty" role="alert">
            <AlertCircle aria-hidden="true" />
            <div>
              <strong>Bard models could not be loaded.</strong>
              <span>{modelsError}</span>
            </div>
            <Button onClick={() => void loadModels()}>Retry</Button>
          </div>
        ) : firstParty.length === 0 ? (
          <div className="empty-row">
            <strong>No Bard models are available.</strong>
            <span>Close Settings and reopen it after the catalogue finishes loading.</span>
          </div>
        ) : (
          firstParty.map((model) => {
            const active = state.modelId === model.id;
            const downloading = progress?.id === model.id;
            const error = actionErrors[model.id];
            return (
              <article key={model.id} className={`model${active ? " active" : ""}`}>
                <div className="model-top">
                  <div className="model-id">
                    <div className="model-title-line">
                      <span className="model-name">{model.name}</span>
                      <span className="role-label">{model.roleLabel}</span>
                    </div>
                    <div className="model-badges">
                      <Tag
                        tone="accent"
                        title={model.hostName ?? model.host?.repository ?? "Hosted by Bard"}
                      >
                        <Cloud aria-hidden="true" />
                        Hosted
                      </Tag>
                      {active ? <Tag tone="success">Active</Tag> : null}
                    </div>
                    <p className="model-desc">{model.description}</p>
                    <div
                      className="model-source ellipsis"
                      title={model.source?.url ?? model.host?.url}
                    >
                      {model.host?.repository ?? "Bard release"}
                      {model.source?.repo ? ` · ${model.source.repo}` : ""}
                    </div>
                  </div>
                  {model.accuracy > 0 || model.speed > 0 ? (
                    <div className="model-meters" aria-label="Relative model characteristics">
                      <Meter label="Quality" value={model.accuracy} />
                      <Meter label="Speed" value={model.speed} />
                    </div>
                  ) : null}
                </div>

                <div className="model-meta-row">
                  <div className="model-tags" aria-label="Model details">
                    <Tag>{modelSize(model)}</Tag>
                    <Tag>
                      {model.languages === 1
                        ? "English"
                        : model.languages > 1
                          ? `${model.languages} languages`
                          : "Language coverage not supplied"}
                    </Tag>
                    {model.parameters ? <Tag>{model.parameters} parameters</Tag> : null}
                    {model.quantization ?? model.quant ? (
                      <Tag>{model.quantization ?? model.quant}</Tag>
                    ) : null}
                    {model.license ?? model.source?.license?.name ? (
                      <Tag>{model.license ?? model.source?.license?.name}</Tag>
                    ) : null}
                    {model.downloaded ? <Tag tone="success">Installed</Tag> : null}
                  </div>
                  <div className="model-actions" aria-label={`Actions for ${model.name}`}>
                    {!model.downloaded ? (
                      <>
                        <Button
                          variant="primary"
                          onClick={() => void downloadFirstParty(model, true)}
                          disabled={Boolean(busyAction) || checkingDetected || Boolean(activeProgress)}
                          ariaBusy={busyAction === `${model.id}:use`}
                        >
                          <Download aria-hidden="true" />
                          Download &amp; use
                        </Button>
                        <Button
                          onClick={() => void downloadFirstParty(model, false)}
                          disabled={Boolean(busyAction) || checkingDetected || Boolean(activeProgress)}
                          ariaBusy={busyAction === `${model.id}:only`}
                        >
                          Download only
                        </Button>
                      </>
                    ) : !active ? (
                      <Button
                        variant="primary"
                        onClick={() => void useFirstParty(model)}
                        disabled={Boolean(busyAction) || checkingDetected}
                        ariaBusy={busyAction === `${model.id}:use`}
                      >
                        <Check aria-hidden="true" />
                        Use
                      </Button>
                    ) : null}
                    {model.downloaded ? (
                      <Button
                        variant="danger"
                        onClick={() => setRemoveConfirmId(model.id)}
                        disabled={active || Boolean(busyAction) || checkingDetected}
                        ariaLabel={`Remove ${model.name} from Bard`}
                        title={active ? "Choose another model before removing this one" : "Remove the downloaded model from Bard"}
                        ariaBusy={busyAction === `${model.id}:remove`}
                      >
                        <Trash2 aria-hidden="true" />
                        Remove
                      </Button>
                    ) : null}
                  </div>
                </div>

                {removeConfirmId === model.id ? (
                  <div className="remove-confirm" role="alert">
                    <span>Remove {model.name} from Bard?</span>
                    <div className="remove-confirm-actions">
                      <Button
                        onClick={() => setRemoveConfirmId(null)}
                        disabled={Boolean(busyAction) || checkingDetected}
                      >
                        Cancel
                      </Button>
                      <Button
                        variant="danger"
                        onClick={() => void removeFirstParty(model)}
                        disabled={Boolean(busyAction) || checkingDetected}
                        ariaBusy={busyAction === `${model.id}:remove`}
                      >
                        Remove
                      </Button>
                    </div>
                  </div>
                ) : null}
                {downloading ? (
                  <ProgressDetails
                    progress={progress}
                    onCancel={() => void api.downloadModelCancel().catch((error) => setActionError(model.id, errorText(error)))}
                    onRetry={() => void downloadFirstParty(model, !model.downloaded)}
                  />
                ) : null}
                {error ? <div className="inline-error" role="alert">{error}</div> : null}
              </article>
            );
          })
        )}
      </Group>

      <Group
        title="On this computer"
        note="Models found in shared local folders. Bard can register and use them without copying or deleting the source file."
      >
        <div className="group-toolbar">
          <span>Scan known local model folders without uploading their contents.</span>
          <Button onClick={() => void loadDetectedModels()} disabled={detectedLoading} ariaBusy={detectedLoading}>
            <RefreshCw aria-hidden="true" />
            Rescan
          </Button>
        </div>
        {detectedLoading && detected.length === 0 ? (
          <div className="loading-row" role="status">
            <LoaderCircle className="spinning" aria-hidden="true" />
            Checking local models…
          </div>
        ) : detectedError && detected.length === 0 ? (
          <div className="error-empty" role="alert">
            <AlertCircle aria-hidden="true" />
            <div>
              <strong>Local models could not be scanned.</strong>
              <span>{detectedError}</span>
            </div>
            <Button onClick={() => void loadDetectedModels()}>Retry</Button>
          </div>
        ) : detected.length === 0 ? (
          <div className="empty-row">
            <HardDrive aria-hidden="true" />
            <div>
              <strong>No local models detected.</strong>
              <span>Use Advanced model import to inspect a specific Hugging Face repository.</span>
            </div>
          </div>
        ) : (
          detected.map((model) => {
            const key = detectedKey(model);
            const compatibility = compatibilityFrom(model.compatibility ?? "unknown");
            const active = state.modelId === model.id;
            const error = actionErrors[`detected:${key}`];
            return (
              <div key={key} className={`detected-model${active ? " active" : ""}`}>
                <div className="detected-main">
                  <div className="detected-title">
                    <span className="detected-name">{model.name ?? model.file}</span>
                    <Tag
                      tone={
                        compatibility.compatibility === "compatible"
                          ? "success"
                          : compatibility.compatibility === "incompatible" ||
                              compatibility.compatibility === "missing"
                            ? "danger"
                            : "default"
                      }
                    >
                      {compatibility.compatibility === "checking" ? (
                        <LoaderCircle className="spinning" aria-hidden="true" />
                      ) : compatibility.compatibility === "compatible" ? (
                        <CheckCircle2 aria-hidden="true" />
                      ) : null}
                      {compatibility.compatibility[0].toUpperCase() + compatibility.compatibility.slice(1)}
                    </Tag>
                    {active ? <Tag tone="success">Active</Tag> : null}
                  </div>
                  <div className="detected-source ellipsis" title={model.localPath}>{model.repo} · {model.file}</div>
                  <div className="detected-meta">
                    {formatBytes(model.size)}
                    {model.parameters ? ` · ${model.parameters} parameters` : ""}
                    {model.quantization ?? model.quant ? ` · ${model.quantization ?? model.quant}` : ""}
                    {model.sourceRevision ? ` · ${model.sourceRevision.slice(0, 12)}` : ""}
                  </div>
                  {compatibility.message ? (
                    <div className="detected-message">{compatibility.message}</div>
                  ) : null}
                </div>
                <div className="model-actions" aria-label={`Actions for ${model.file}`}>
                  {compatibility.compatibility === "missing" ? (
                     <Tag tone="danger">File missing</Tag>
                   ) : compatibility.compatibility !== "compatible" ? (
                    <Button
                      onClick={() => void checkDetectedModel(model)}
                      disabled={checkingDetected || Boolean(busyAction)}
                      ariaBusy={compatibility.compatibility === "checking"}
                    >
                      {compatibility.compatibility === "checking" ? (
                        <LoaderCircle className="spinning" aria-hidden="true" />
                      ) : (
                        <ShieldCheck aria-hidden="true" />
                      )}
                      {compatibility.compatibility === "checking" ? "Checking" : "Check again"}
                    </Button>
                  ) : !active ? (
                    <Button
                      variant="primary"
                      onClick={() => void activateDetected(model)}
                      disabled={Boolean(busyAction) || checkingDetected}
                      ariaBusy={busyAction === `detected-use:${key}`}
                    >
                      <Check aria-hidden="true" />
                      Use
                    </Button>
                  ) : null}
                  <Button
                    onClick={() => model.registered && void forgetDetected(model)}
                    disabled={active || !model.registered || Boolean(busyAction) || checkingDetected}
                    ariaLabel={`${model.owned ? "Remove" : "Forget"} ${model.file}`}
                    title={
                      model.owned
                        ? "Remove Bard's installed copy"
                        : "Remove Bard's reference; the external file is not deleted"
                    }
                    ariaBusy={busyAction === `detected-forget:${key}`}
                  >
                    <Trash2 aria-hidden="true" />
                    {model.owned ? "Remove" : "Forget"}
                  </Button>
                </div>
                {error ? <div className="inline-error" role="alert">{error}</div> : null}
              </div>
            );
          })
        )}
        <Note>Forget only removes Bard's reference to a local model. Bard never deletes a file from an external cache or folder; Remove applies only to copies Bard owns.</Note>
      </Group>

      <details className="model-disclosure">
        <summary>
          <span className="disclosure-leading">
            <span className="disclosure-icon"><HardDrive aria-hidden="true" /></span>
            <span>
              <strong>Advanced model import</strong>
              <small>Inspect a Hugging Face repository before Bard installs anything.</small>
            </span>
          </span>
          <span className="disclosure-trailing">Optional</span>
        </summary>
        <div className="disclosure-body">
          <p className="import-intro">
            Paste a repository ID, repository URL, or resolve URL. Bard checks the file
            list, source, size, and checksum before download, then verifies that the engine
            can load the exact file before activating it.
          </p>
          <div className="repo-form">
            <TextField
              id="hf-model-source"
              label="Hugging Face repository or resolve URL"
              showLabel
              value={repoInput}
              placeholder="owner/repository or huggingface.co/owner/repository/resolve/main/model.gguf"
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
              variant="primary"
              onClick={() => void scanRepository()}
              disabled={!repoInput.trim() || scanning || Boolean(activeProgress)}
              ariaBusy={scanning}
            >
              {scanning ? (
                <LoaderCircle className="spinning" aria-hidden="true" />
              ) : (
                <ShieldCheck aria-hidden="true" />
              )}
              {scanning ? "Checking…" : "Check repository"}
            </Button>
          </div>

          {scanError ? <Note tone="danger">{scanError}</Note> : null}

          {files ? (
            <div className="repo-results" aria-live="polite">
              <div className="repo-results-head">
                <div>
                  <strong>
                    {filesAwaitingVerification > 0
                      ? `${requestedFiles.length} model ${requestedFiles.length === 1 ? "file" : "files"} to verify`
                      : `${verifiedFileCount} compatible ${verifiedFileCount === 1 ? "file" : "files"}`}
                  </strong>
                  <span>{scannedInput?.repo}</span>
                </div>
                <Button
                  onClick={() => void scanRepository()}
                  disabled={scanning || Boolean(activeProgress)}
                  ariaBusy={scanning}
                >
                  <RefreshCw aria-hidden="true" />
                  Rescan
                </Button>
              </div>

              {requestedFiles.length === 0 ? (
                <div className="empty-row">
                  <AlertCircle aria-hidden="true" />
                  <div>
                    <strong>
                      {scannedInput?.requestedFile
                        ? "The linked file is not available."
                        : "No compatible model files found."}
                    </strong>
                    <span>
                      {scannedInput?.requestedFile
                        ? "The resolve URL may be stale, or its file may not be a supported model."
                        : "Choose a repository containing a compatible GGUF model file."}
                    </span>
                  </div>
                </div>
              ) : (
                requestedFiles.map((file) => {
                  const check = fileChecks[file.path] ?? compatibilityFrom(file.compatibility ?? "unknown");
                  const fileProgress = hfProgress?.id.endsWith(file.path) ? hfProgress : null;
                  const installing = busyAction === `install:${file.path}`;
                  const error = installErrors[file.path];
                  const metadataReady = hasInstallMetadata(file);
                  const sourceRepo = file.sourceRepo ?? scannedInput?.repo ?? "Hugging Face";
                  const revision = file.sourceRevision ?? "main";
                  return (
                    <div key={file.path} className="hf-file">
                      <div className="hf-file-main">
                        <div className="hf-file-title">
                          <span className="hf-file-name ellipsis" title={file.path}>{file.path}</span>
                          {installedFile === file.path ? <Tag tone="success">Installed</Tag> : null}
                        </div>
                        <div className="hf-file-meta">
                          <span>{formatBytes(file.size)}</span>
                          {file.quant ? <span>{file.quant}</span> : null}
                          <span title={file.sha256 || "No checksum supplied"} className="checksum">
                            {file.sha256 ? `SHA-256 ${file.sha256.slice(0, 12)}…` : "Checksum not supplied"}
                          </span>
                          <span className="ellipsis" title={sourceRepo}>{sourceRepo}{revision ? ` @ ${revision.slice(0, 12)}` : ""}</span>
                        </div>
                        {check.message ? <div className="detected-message">{check.message}</div> : null}
                      </div>
                      <div className="model-actions">
                        {!metadataReady ? <Tag tone="danger">No integrity metadata</Tag> : null}
                        {check.compatibility === "compatible" ? (
                          fileProgress ? (
                            <span className="file-check-state">
                              <CheckCircle2 aria-hidden="true" />
                              Compatible
                            </span>
                          ) : (
                            <Tag tone="success">
                              <CheckCircle2 aria-hidden="true" />
                              Compatible
                            </Tag>
                          )
                        ) : check.compatibility === "unknown" ? (
                          <Tag>Verify on install</Tag>
                        ) : check.compatibility === "checking" ? (
                          <span className="file-check-state">
                            <LoaderCircle className="spinning" aria-hidden="true" />
                            Checking
                          </span>
                        ) : (
                          <Tag tone="danger">Not supported</Tag>
                        )}
                        <Button
                          variant="primary"
                          onClick={() => void installFile(file)}
                          disabled={
                            installedFile === file.path ||
                            !metadataReady ||
                            (check.compatibility !== "compatible" &&
                              check.compatibility !== "unknown") ||
                            installing ||
                            Boolean(busyAction) ||
                            Boolean(activeProgress)
                          }
                          ariaBusy={installing}
                        >
                          {installing ? (
                            <LoaderCircle className="spinning" aria-hidden="true" />
                          ) : installedFile === file.path ? (
                            <CheckCircle2 aria-hidden="true" />
                          ) : (
                            <Download aria-hidden="true" />
                          )}
                          {installing
                            ? "Installing…"
                            : installedFile === file.path
                              ? "Installed"
                              : !metadataReady
                                ? "Integrity metadata required"
                                : check.compatibility === "unknown"
                                ? "Verify & install"
                                : "Install & use"}
                        </Button>
                      </div>
                      {fileProgress ? (
                        <ProgressDetails
                          progress={fileProgress}
                          onCancel={() => void api.downloadModelCancel().catch((scanFailure) => setScanError(errorText(scanFailure)))}
                          onRetry={() => void installFile(file)}
                        />
                      ) : null}
                      {error ? <div className="inline-error" role="alert">{error}</div> : null}
                    </div>
                  );
                })
              )}
            </div>
          ) : null}
        </div>
      </details>
    </Pane>
  );
}
