import { useEffect, useState } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "../../store";
import type { PostprocessingSettings, PostprocessingTestResponse } from "../../types";
import { Button, Group, Row, Section, TextField, Toggle } from "../ui";

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
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

const REFINE_DESCRIPTION =
  "Tidy finished transcripts with a language model you choose: filler words out, punctuation in.";

export function RefineSection() {
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
  const [replacingKey, setReplacingKey] = useState(false);

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
      setReplacingKey(false);
      setStatus({ tone: "success", message: "Saved." });
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
      setReplacingKey(false);
      setStatus({ tone: "muted", message: "Key removed. Refine is off." });
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setClearing(false);
    }
  };

  const controlsDisabled = loading || !settings || saving || testing || clearing;
  const showKeyField = !keyConfigured || replacingKey;

  if (loading && !settings) {
    return (
      <Section title="Refine" description={REFINE_DESCRIPTION}>
        <div className="loading-row" role="status">
          <LoaderCircle className="spinning" aria-hidden="true" />
          Loading…
        </div>
      </Section>
    );
  }

  if (error && !settings) {
    return (
      <Section title="Refine" description={REFINE_DESCRIPTION}>
        <div className="error-empty" role="alert">
          <AlertCircle aria-hidden="true" />
          <div>
            <strong>Refine could not load.</strong>
            <span>{error}</span>
          </div>
          <Button onClick={loadSettings}>
            <RotateCcw aria-hidden="true" />
            Retry
          </Button>
        </div>
      </Section>
    );
  }

  return (
    <Section title="Refine" description={REFINE_DESCRIPTION}>
      <Group>
        <Row
          label="Refine transcripts"
          hint="Only the finished text is sent — never audio, captures or history. If a request fails, the local transcript is used."
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
      </Group>

      {enabled || keyConfigured || dirty ? (
      <Group
        title="OpenRouter"
        note={
          <>
            Refine uses{" "}
            <a href="https://openrouter.ai/keys" target="_blank" rel="noreferrer">
              your own OpenRouter key
            </a>
            , so you choose the model and pay OpenRouter directly.
          </>
        }
      >
        <Row
          htmlFor={showKeyField ? "openrouter-api-key" : undefined}
          label="API key"
          stacked={showKeyField}
          hint={showKeyField ? undefined : "Saved in Windows Credential Manager."}
        >
          {showKeyField ? (
            <TextField
              id="openrouter-api-key"
              label="OpenRouter API key"
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={apiKey}
              disabled={controlsDisabled}
              placeholder="sk-or-…"
              onChange={(next) => {
                setApiKey(next);
                setError(null);
                setStatus(null);
              }}
              onEnter={() => void save()}
            />
          ) : (
            <div className="row-buttons">
              <Button onClick={() => setReplacingKey(true)} disabled={controlsDisabled}>
                Replace
              </Button>
              <Button
                variant="danger"
                onClick={() => void clearKey()}
                disabled={controlsDisabled}
                ariaBusy={clearing}
              >
                Remove
              </Button>
            </div>
          )}
        </Row>
        <Row
          stacked
          htmlFor="openrouter-model-id"
          label="Model"
          hint={
            <>
              Any model ID from{" "}
              <a href="https://openrouter.ai/models" target="_blank" rel="noreferrer">
                openrouter.ai/models
              </a>
              .
            </>
          }
        >
          <TextField
            id="openrouter-model-id"
            label="OpenRouter model ID"
            autoComplete="off"
            spellCheck={false}
            value={modelId}
            disabled={controlsDisabled}
            placeholder="anthropic/claude-haiku-4.5"
            onChange={(next) => {
              setModelId(next);
              setError(null);
              setStatus(null);
            }}
            onEnter={() => void save()}
          />
        </Row>
        <div className="refine-footer">
          <div
            className={`postprocessing-status ${error ? "danger" : status?.tone ?? "muted"}`}
            role={error || status?.tone === "danger" ? "alert" : "status"}
            aria-live="polite"
          >
            {error ?? status?.message ?? (keyConfigured && savedModelId ? "Ready." : "")}
          </div>
          {testing ? (
            <Button onClick={() => void cancelTest()} ariaBusy={true}>
              <LoaderCircle className="spinning" aria-hidden="true" />
              Cancel
            </Button>
          ) : (
            <Button
              onClick={() => void testConnection()}
              disabled={controlsDisabled || dirty || !keyConfigured || !savedModelId}
              title={dirty ? "Save first" : undefined}
            >
              Test
            </Button>
          )}
          {dirty ? (
            <Button
              variant="primary"
              onClick={() => void save()}
              disabled={controlsDisabled}
              ariaBusy={saving}
            >
              {saving ? "Saving…" : "Save"}
            </Button>
          ) : null}
        </div>
      </Group>
      ) : null}

      <p className="pane-footnote">
        Did Refine return something wrong or inappropriate?{" "}
        <a
          href="https://miiso.dev/bard#support"
          onClick={(event) => {
            event.preventDefault();
            void api.openLink("https://miiso.dev/bard#support");
          }}
        >
          Report it
        </a>
      </p>
    </Section>
  );
}
