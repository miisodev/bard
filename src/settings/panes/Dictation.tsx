import { useEffect, useState } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "../../store";
import { Button, Group, Note, Pane, Row, Select } from "../ui";

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

export default function DictationPane() {
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");
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
    ])
      .then(([available, selected]) => {
        if (!alive) return;
        setMicrophones(available);
        setSelectedMic(selected);
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

  return (
    <Pane title="Dictation" description="Speech to text on this PC. Your voice never leaves it.">
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

      <Group title="Using it">
        <Row
          label="Dictate from anywhere"
          hint="Hold the dictation shortcut to talk and let go to finish, or tap it to start and tap again to stop. Escape cancels."
        />
        <Row
          label="Where the words go"
          hint="The transcript goes on your clipboard and into Bard's history. Paste it wherever you like."
        />
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
        Audio and transcription stay on this device. If Refine is on (Settings ›
        Models), the finished transcript text — and nothing else — is sent to
        OpenRouter.
      </Note>
    </Pane>
  );
}

