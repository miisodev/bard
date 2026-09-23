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
import type { PostprocessingSettings, PostprocessingTestResponse } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Row, Select, Tag, TextField, Toggle } from "../ui";

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

export default function DictationPane(_: PaneProps) {
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    Promise.all([api.getMicrophones(), api.getSettingStr("selected_microphone")])
      .then(([available, selected]) => {
        if (!alive) return;
        setMicrophones(available);
        setSelectedMic(selected);
        setError(null);
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
  }, [loadAttempt]);

  return (
    <Pane title="Dictation" description="Speech to text, entirely on this machine.">
      <Group title="Input">
        <Row label="Microphone" hint="The system default is used unless you pick one.">
          <Select
            label="Microphone"
            value={selectedMic}
            disabled={loading || Boolean(error)}
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

      {loading ? (
        <div className="loading-row" role="status">
          <LoaderCircle className="spinning" aria-hidden="true" />
          Loading input devices…
        </div>
      ) : error ? (
        <div className="error-empty" role="alert">
          <AlertCircle aria-hidden="true" />
          <div>
            <strong>Microphones could not be loaded.</strong>
            <span>{error}</span>
          </div>
          <Button
            onClick={() => setLoadAttempt((attempt) => attempt + 1)}
            disabled={loading}
          >
            <RotateCcw aria-hidden="true" />
            Retry
          </Button>
        </div>
      ) : null}

      <Note>
        Audio capture and transcription stay on this device. If cloud post-processing is
        enabled, only the finished transcript text is sent to the service you configure.
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
      setError("Enter the exact OpenRouter model ID before enabling post-processing.");
      return;
    }
    if (enabled && !keyConfigured && !apiKey.trim()) {
      setError("Add an OpenRouter API key before enabling post-processing.");
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
      setStatus({ tone: "success", message: "Post-processing settings saved." });
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
      setStatus({ tone: "success", message: "API key cleared. Cloud post-processing is off." });
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setClearing(false);
    }
  };

  const controlsDisabled = loading || !settings || saving || testing || clearing;

  return (
    <Pane
      title="Post-processing"
      description="Optional text cleanup after local transcription. Off by default."
    >
      <Group
        title="OpenRouter"
        note="Bard sends one finished transcript to the exact model ID you choose. Audio, captures, history, and unrelated items stay on this device."
      >
        <Row
          label={
            <span className="setting-label-with-tag">
              Cloud post-processing
              <Tag tone={enabled ? "success" : "muted"}>{enabled ? "On" : "Off"}</Tag>
            </span>
          }
          hint="The local transcript is used unchanged whenever this is off or a request fails."
        >
          <Toggle
            label="Cloud post-processing"
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
          <Button
            onClick={() => void testConnection()}
            disabled={controlsDisabled || dirty}
            ariaBusy={testing}
          >
            {testing ? <LoaderCircle className="spinning" aria-hidden="true" /> : <PlugZap aria-hidden="true" />}
            {testing ? "Testing…" : "Test connection"}
          </Button>
          <Button
            onClick={() => void clearKey()}
            disabled={controlsDisabled || (!keyConfigured && !apiKey)}
            ariaBusy={clearing}
            title="Clear the stored key and turn cloud post-processing off"
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

      {loading ? (
        <div className="loading-row" role="status">
          <LoaderCircle className="spinning" aria-hidden="true" />
          Loading post-processing settings…
        </div>
      ) : error && !settings ? (
        <div className="error-empty" role="alert">
          <AlertCircle aria-hidden="true" />
          <div>
            <strong>Post-processing settings could not be loaded.</strong>
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
        Audio and local transcription always stay on this machine. When cloud
        post-processing is enabled, only the already-produced transcript text is sent to
        OpenRouter. Bard has no account, sync, or telemetry.
      </Note>
    </Pane>
  );
}
