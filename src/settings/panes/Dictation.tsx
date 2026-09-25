import { useEffect, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Eraser,
  KeyRound,
  LoaderCircle,
  PlugZap,
  RotateCcw,
  Save,
  ShieldCheck,
} from "lucide-react";
import { api } from "../../store";
import type {
  OverlayHorizontal,
  OverlayPosition,
  OverlayVertical,
  PostprocessingSettings,
  PostprocessingTestResponse,
} from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row, Segmented, Select, Tag, TextField, Toggle } from "../ui";

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

const DEFAULT_OVERLAY_POSITION: OverlayPosition = "bottom-middle";
const OVERLAY_POSITIONS: OverlayPosition[] = [
  "top-left",
  "top-middle",
  "top-right",
  "bottom-left",
  "bottom-middle",
  "bottom-right",
];
const OVERLAY_VERTICAL_OPTIONS: { value: OverlayVertical; label: string }[] = [
  { value: "top", label: "Top" },
  { value: "bottom", label: "Bottom" },
];
const OVERLAY_HORIZONTAL_OPTIONS: { value: OverlayHorizontal; label: string }[] = [
  { value: "left", label: "Left" },
  { value: "middle", label: "Middle" },
  { value: "right", label: "Right" },
];

function parseOverlayPosition(value: string): OverlayPosition {
  return OVERLAY_POSITIONS.includes(value as OverlayPosition)
    ? (value as OverlayPosition)
    : DEFAULT_OVERLAY_POSITION;
}

function overlayParts(position: OverlayPosition): [OverlayVertical, OverlayHorizontal] {
  const [vertical, horizontal] = position.split("-");
  return [vertical as OverlayVertical, horizontal as OverlayHorizontal];
}

export default function DictationPane(_: PaneProps) {
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");
  const [overlayPosition, setOverlayPosition] = useState<OverlayPosition>(DEFAULT_OVERLAY_POSITION);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setLoadError(null);
    setError(null);
    Promise.all([
      api.getMicrophones(),
      api.getSettingStr("selected_microphone"),
      api.getSettingStr("dictation_overlay_position"),
    ])
      .then(([available, selected, position]) => {
        if (!alive) return;
        setMicrophones(available);
        setSelectedMic(selected);
        setOverlayPosition(parseOverlayPosition(position));
        setError(null);
      })
      .catch((reason: unknown) => {
        if (alive) setLoadError(errorText(reason));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [loadAttempt]);

  const [overlayVertical, overlayHorizontal] = overlayParts(overlayPosition);
  const changeOverlayPosition = (vertical: OverlayVertical, horizontal: OverlayHorizontal) => {
    const next = `${vertical}-${horizontal}` as OverlayPosition;
    const previous = overlayPosition;
    setOverlayPosition(next);
    setError(null);
    void api.setDictationOverlayPosition(next).catch((reason: unknown) => {
      setOverlayPosition(previous);
      setError(errorText(reason));
    });
  };

  return (
    <Pane title="Dictation" description="Speech to text, entirely on this machine.">
      <Group title="Input">
        <Row label="Microphone" hint="The system default is used unless you pick one.">
          <Select
            label="Microphone"
            value={selectedMic}
            disabled={loading || Boolean(loadError)}
            onChange={(name) => {
              setSelectedMic(name);
              setError(null);
              void api.setMicrophone(name).catch((reason: unknown) => setError(errorText(reason)));
            }}
            options={[
              { value: "", label: "System default" },
              ...microphones.map((microphone) => ({ value: microphone, label: microphone })),
            ]}
          />
        </Row>
      </Group>

      <Group
        title="Overlay position"
        note="Choose where the recording pill appears on the screen. These controls make six positions."
      >
        <Row label="Vertical" hint="Top or bottom of the screen.">
          <Segmented
            label="Vertical overlay position"
            value={overlayVertical}
            options={OVERLAY_VERTICAL_OPTIONS}
            disabled={loading || Boolean(loadError)}
            onChange={(value) => changeOverlayPosition(value, overlayHorizontal)}
          />
        </Row>
        <Row label="Horizontal" hint="Left, middle, or right of the screen.">
          <Segmented
            label="Horizontal overlay position"
            value={overlayHorizontal}
            options={OVERLAY_HORIZONTAL_OPTIONS}
            disabled={loading || Boolean(loadError)}
            onChange={(value) => changeOverlayPosition(overlayVertical, value)}
          />
        </Row>
      </Group>

      {loading ? (
        <div className="loading-row" role="status">
          <LoaderCircle className="spinning" aria-hidden="true" />
          Loading input devices…
        </div>
      ) : loadError ? (
        <div className="error-empty" role="alert">
          <AlertCircle aria-hidden="true" />
          <div>
            <strong>Dictation settings could not be loaded.</strong>
            <span>{loadError}</span>
          </div>
          <Button onClick={() => setLoadAttempt((attempt) => attempt + 1)}>
            <RotateCcw aria-hidden="true" />
            Retry
          </Button>
        </div>
      ) : null}

      {!loading && error ? <Note tone="danger">{error}</Note> : null}

      <Note>
        Audio capture and transcription stay on this device. If Refine is on, only the
        finished transcript text is sent to the service you configure.
      </Note>
    </Pane>
  );
}

type TestStatus = { tone: "success" | "danger" | "muted"; message: string };

function testResponse(response: PostprocessingTestResponse, fallbackModel?: string): TestStatus {
  if (typeof response === "boolean") {
    return response
      ? { tone: "success", message: `Connection successful${fallbackModel ? ` · ${fallbackModel}` : ""}.` }
      : { tone: "danger", message: "OpenRouter could not validate this configuration." };
  }
  if ("ok" in response) {
    return {
      tone: response.ok ? "success" : "danger",
      message:
        response.message ??
        (response.ok
          ? `Connection successful${response.modelName ? ` · ${response.modelName}` : ""}.`
          : "OpenRouter could not validate this configuration."),
    };
  }
  return { tone: "danger", message: "OpenRouter returned an unexpected test response." };
}

export function PostprocessingPane(_: PaneProps) {
  const [settings, setSettings] = useState<PostprocessingSettings | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [modelId, setModelId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<TestStatus | null>(null);

  const keyConfigured = Boolean(
    settings?.keyConfigured ??
      settings?.apiKeyConfigured ??
      settings?.hasApiKey ??
      false,
  );
  const savedModelId = settings
    ? settings.modelIds?.length
      ? settings.modelIds.join(",")
      : settings.modelId ?? ""
    : "";
  const dirty =
    apiKey.trim().length > 0 ||
    (settings !== null && (enabled !== settings.enabled || modelId.trim() !== savedModelId));

  const loadSettings = () => {
    setLoading(true);
    setError(null);
    void api
      .getPostprocessing()
      .then((next) => {
        setSettings(next);
        setEnabled(next.enabled);
        setModelId(next.modelIds?.join(",") ?? next.modelId ?? "");
        setStatus(
          next.statusMessage
            ? { tone: next.enabled ? "success" : "muted", message: next.statusMessage }
            : null,
        );
      })
      .catch((reason: unknown) => setError(errorText(reason)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    let alive = true;
    setLoading(true);
    void api
      .getPostprocessing()
      .then((next) => {
        if (!alive) return;
        setSettings(next);
        setEnabled(next.enabled);
        setModelId(next.modelIds?.join(",") ?? next.modelId ?? "");
        setStatus(
          next.statusMessage
            ? { tone: next.enabled ? "success" : "muted", message: next.statusMessage }
            : null,
        );
      })
      .catch((reason: unknown) => {
        if (alive) setError(errorText(reason));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  const save = async () => {
    if (!settings || saving || testing || clearing) return;
    const normalizedModel = modelId.trim();
    const parsedModelIds = normalizedModel
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean);
    if (enabled && !normalizedModel) {
      setError("Enter the exact OpenRouter model ID before turning on Refine.");
      return;
    }
    if (enabled && !keyConfigured && !apiKey.trim()) {
      setError("Add an OpenRouter API key before turning on Refine.");
      return;
    }

    setSaving(true);
    setError(null);
    setStatus(null);
    try {
      const saved = await api.savePostprocessing({
        enabled,
        modelId: normalizedModel,
        modelIds: parsedModelIds,
        ...(apiKey.trim() ? { apiKey: apiKey.trim() } : {}),
      });
      const next: PostprocessingSettings = saved ?? {
        ...settings,
        enabled,
        modelId: normalizedModel,
        modelIds: parsedModelIds,
        keyConfigured: keyConfigured || Boolean(apiKey.trim()),
      };
      setSettings(next);
      setEnabled(next.enabled);
      setModelId(next.modelIds?.join(",") ?? next.modelId ?? "");
      setApiKey("");
      setStatus({ tone: "success", message: "Refine settings saved." });
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setSaving(false);
    }
  };

  const testConnection = async () => {
    if (!settings || testing || saving || clearing) return;
    if (dirty) {
      setError("Save or clear your changes before testing the connection.");
      return;
    }
    setTesting(true);
    setError(null);
    setStatus(null);
    try {
      const response = await api.testPostprocessing();
      setStatus(testResponse(response, settings.modelName ?? savedModelId));
    } catch (reason) {
      const message = errorText(reason);
      if (message.toLowerCase().includes("cancel")) {
        setStatus({ tone: "muted", message: "Connection test cancelled." });
      } else {
        setError(message);
      }
    } finally {
      setTesting(false);
    }
  };

  const cancelTest = async () => {
    try {
      await api.testPostprocessingCancel();
      setStatus({ tone: "muted", message: "Connection test cancelled." });
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setTesting(false);
    }
  };

  const clearKey = async () => {
    if (!settings || clearing || saving || testing) return;
    if (!keyConfigured && !apiKey) return;
    setClearing(true);
    setError(null);
    setStatus(null);
    try {
      const normalizedModel = modelId.trim();
      const parsedModelIds = normalizedModel
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean);
      const saved = await api.savePostprocessing({
        enabled: false,
        modelId: normalizedModel,
        modelIds: parsedModelIds,
        apiKey: "",
        clearKey: true,
      });
      const next: PostprocessingSettings = saved ?? {
        ...settings,
        enabled: false,
        modelId: normalizedModel,
        modelIds: parsedModelIds,
        keyConfigured: false,
        apiKeyConfigured: false,
        hasApiKey: false,
        ready: false,
      };
      setSettings(next);
      setEnabled(false);
      setApiKey("");
      setStatus({ tone: "success", message: "API key cleared. Refine is off." });
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setClearing(false);
    }
  };

  const controlsDisabled = loading || !settings || saving || testing || clearing;

  return (
    <Pane
      title="Refine"
      description="Optional cloud cleanup of finished transcripts. Off by default."
    >
      <Group
        title="OpenRouter"
        note="Bard sends one finished transcript to the exact model ID you choose. Audio, captures, history, and unrelated items stay on this device."
      >
        <Row
          label={
            <span className="setting-label-with-tag">
              Refine transcripts
              <Tag tone={enabled ? "success" : "muted"}>{enabled ? "On" : "Off"}</Tag>
            </span>
          }
          hint="The local transcript is used unchanged whenever this is off or a request fails."
        >
          <Toggle
            label="Refine transcripts"
            checked={enabled}
            disabled={controlsDisabled}
            onChange={(next) => {
              setEnabled(next);
              setError(null);
              setStatus(null);
            }}
          />
        </Row>
        <Row
          stacked
          label="OpenRouter API key"
          hint={
            keyConfigured
              ? "A key is stored in Windows Credential Manager. Enter a new key only to replace it."
              : "Stored in Windows Credential Manager, never in Bard's database or settings state."
          }
        >
          <TextField
            id="openrouter-api-key"
            label="OpenRouter API key"
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={apiKey}
            disabled={controlsDisabled}
            placeholder={keyConfigured ? "Stored securely — enter a replacement" : "Enter OpenRouter API key"}
            onChange={(next) => {
              setApiKey(next);
              setError(null);
              setStatus(null);
            }}
            onEnter={() => void save()}
          />
        </Row>
        <Row
          stacked
          label="Model IDs"
          hint="Use the provider-prefixed IDs shown by OpenRouter, separated by commas. Bard sends the first ID; it never substitutes a fallback."
        >
          <TextField
            id="openrouter-model-id"
            label="OpenRouter model IDs"
            autoComplete="off"
            spellCheck={false}
            value={modelId}
            disabled={controlsDisabled}
            placeholder="vendor/model"
            onChange={(next) => {
              setModelId(next);
              setError(null);
              setStatus(null);
            }}
            onEnter={() => void save()}
          />
        </Row>
        <div className="postprocessing-actions">
          <Button
            variant="primary"
            onClick={() => void save()}
            disabled={controlsDisabled || !dirty}
            ariaBusy={saving}
          >
            {saving ? <LoaderCircle className="spinning" aria-hidden="true" /> : <Save aria-hidden="true" />}
            {saving ? "Saving…" : "Save"}
          </Button>
          {testing ? (
            <Button onClick={() => void cancelTest()} ariaBusy={true}>
              <LoaderCircle className="spinning" aria-hidden="true" />
              Cancel test
            </Button>
          ) : (
            <Button
              onClick={() => void testConnection()}
              disabled={controlsDisabled || dirty}
              ariaBusy={false}
            >
              <PlugZap aria-hidden="true" />
              Test connection
            </Button>
          )}
          <Button
            onClick={() => void clearKey()}
            disabled={controlsDisabled || (!keyConfigured && !apiKey)}
            ariaBusy={clearing}
            title="Clear the stored key and turn Refine off"
          >
            {clearing ? <LoaderCircle className="spinning" aria-hidden="true" /> : <Eraser aria-hidden="true" />}
            Clear key
          </Button>
        </div>
        <div className="postprocessing-security">
          <ShieldCheck aria-hidden="true" />
          <span>
            <KeyRound aria-hidden="true" />
            The key is sent only when you save and is never included in app state or events.
          </span>
        </div>
      </Group>

      <Group title="Report">
        <Row
          label="Report a refined transcript"
          hint="If Refine returns something wrong, harmful or inappropriate, send what you said and what came back."
        >
          <Button onClick={() => void api.openLink("https://miiso.dev/tools/bard#support")}>Report</Button>
        </Row>
      </Group>

      {loading ? (
        <div className="loading-row" role="status">
          <LoaderCircle className="spinning" aria-hidden="true" />
          Loading Refine settings…
        </div>
      ) : error && !settings ? (
        <div className="error-empty" role="alert">
          <AlertCircle aria-hidden="true" />
          <div>
            <strong>Refine settings could not be loaded.</strong>
            <span>{error}</span>
          </div>
          <Button onClick={loadSettings}>
            <RotateCcw aria-hidden="true" />
            Retry
          </Button>
        </div>
      ) : null}

      {error && settings ? <Note tone="danger">{error}</Note> : null}
      {status ? (
        <div
          className={`postprocessing-status ${status.tone}`}
          role={status.tone === "danger" ? "alert" : "status"}
          aria-live="polite"
        >
          {status.tone === "success" ? <CheckCircle2 aria-hidden="true" /> : <AlertCircle aria-hidden="true" />}
          {status.message}
        </div>
      ) : null}

      <Note>
        Audio and local transcription always stay on this machine. When Refine is on,
        only the already-produced transcript text is sent to OpenRouter. Bard has no account, sync, or telemetry.
      </Note>
    </Pane>
  );
}
