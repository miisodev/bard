import { useEffect, useState, type ReactNode } from "react";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { api } from "../../store";
import type { TranscriptOptions } from "../../types";
import { Button, Group, Note, Pane, Row, Segmented, Select, Toggle } from "../ui";

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  const text = String(error);
  return text.replace(/^Error:\s*/i, "").trim() || "Something went wrong.";
}

/** The silence lengths a take can use, in the order they are offered. */
const SILENCE_OPTIONS: { value: string; label: string }[] = [
  { value: "1200", label: "1.2 s" },
  { value: "1600", label: "1.6 s" },
  { value: "2400", label: "2.4 s" },
  { value: "0", label: "Never" },
];

const DEFAULTS: TranscriptOptions = {
  trailingSpace: true,
  fillers: false,
  silenceMs: 1600,
};

export default function DictationPane({ models }: { models?: ReactNode }) {
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState("");
  const [options, setOptions] = useState<TranscriptOptions>(DEFAULTS);
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
      api.getDictationOptions(),
    ])
      .then(([available, selected, transcript]) => {
        if (!alive) return;
        setMicrophones(available);
        setSelectedMic(selected);
        setOptions(transcript);
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

  /**
   * Save one of the transcript choices.
   *
   * The change is shown at once and the write happens behind it: the person
   * is choosing how the next transcript should read, and a slow disk must not
   * hold the switch still.
   */
  const change = (next: Partial<TranscriptOptions>) => {
    const updated = { ...options, ...next };
    setOptions(updated);
    setError(null);
    void api.setDictationOptions(updated).catch((reason: unknown) => {
      setError(errorText(reason));
      setOptions(options);
    });
  };

  return (
    <Pane title="Dictation">
      {models}
      <Group title="Input">
        <Row label="Microphone">
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

      <Group title="Transcript">
        <Row label="End with a space">
          <Toggle
            label="End the transcript with a space"
            checked={options.trailingSpace}
            disabled={loading}
            onChange={(next) => change({ trailingSpace: next })}
          />
        </Row>
        <Row label="Remove filler words" hint="Um, uh, hmm.">
          <Toggle
            label="Take filler words out of the transcript"
            checked={options.fillers}
            disabled={loading}
            onChange={(next) => change({ fillers: next })}
          />
        </Row>
        <Row label="Stop after silence">
          <Segmented
            label="Stop after silence"
            value={String(options.silenceMs)}
            disabled={loading}
            options={SILENCE_OPTIONS}
            onChange={(next) => change({ silenceMs: Number(next) })}
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
    </Pane>
  );
}
