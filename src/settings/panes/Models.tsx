import { useEffect, useState } from "react";
import { api } from "../../store";
import type { CachedHfModel, HfFile, ModelInfo, ModelProgress } from "../../types";
import type { PaneProps } from "../SettingsWindow";
import { Button, Group, Note, Pane, Progress, Row, TextField, formatBytes } from "../ui";

/*
  Three ways to get a model, in the order a person would try them:

    1. the curated list, which is the only one most people should need,
    2. models already sitting in the machine's Hugging Face cache, and
    3. any Hugging Face repo, by name.

  (1) is still bard's own small hardcoded set. Handy's catalog.json — ranked
  recommendations with accuracy and speed, language counts, streaming and
  translation capability, size and per-quant checksums — is the shape this
  should take; see docs/PARITY.md B1.
*/
export default function ModelsPane({
  state,
  refresh,
  progress,
}: PaneProps & { progress: ModelProgress | null }) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [cached, setCached] = useState<CachedHfModel[]>([]);
  const [repo, setRepo] = useState("");
  const [files, setFiles] = useState<HfFile[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reloadModels = () => void api.getModels().then(setModels);

  useEffect(() => {
    reloadModels();
    void api.hfCachedModels().then(setCached);
  }, []);

  const browse = async () => {
    const name = repo.trim();
    if (!name || busy) return;
    setBusy(true);
    setError(null);
    try {
      setFiles(await api.hfListRepo(name));
    } catch (err) {
      setError(String(err));
      setFiles(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Pane title="Models" description="Transcription runs offline. Pick the model it runs.">
      <Group title="Recommended">
        {models.map((m) => {
          const downloading = progress?.id === m.id && progress.percent < 100;
          return (
            <div key={m.id} className="model">
              <Row
                label={m.name}
                hint={m.downloaded ? "Installed" : `${m.sizeMb} MB · runs offline`}
              >
                {downloading ? (
                  <Button onClick={() => void api.downloadModelCancel()}>Cancel</Button>
                ) : !m.downloaded ? (
                  <Button variant="primary" onClick={() => void api.downloadModel(m.id)}>
                    Download
                  </Button>
                ) : m.id !== state.modelId ? (
                  <Button onClick={() => void api.setModel(m.id).then(refresh)}>Use</Button>
                ) : (
                  <span className="active-tag">Active</span>
                )}
              </Row>
              {progress?.id === m.id ? <Progress percent={progress.percent} /> : null}
            </div>
          );
        })}
      </Group>

      {cached.length > 0 ? (
        <Group
          title="Already on this machine"
          note="Found in the shared Hugging Face cache — these need no download."
        >
          {cached.map((c, i) => (
            <Row
              key={`${c.repo}-${i}`}
              label={<span className="ellipsis">{c.file}</span>}
              hint={<span className="ellipsis">{c.repo} · {formatBytes(c.size)}</span>}
            >
              <Button
                variant="primary"
                onClick={() => {
                  void api
                    .hfUseCached(c.repo, c.file, c.localPath, c.size)
                    .then(() => {
                      refresh();
                      reloadModels();
                    })
                    .catch((err) => setError(String(err)));
                }}
              >
                Use
              </Button>
            </Row>
          ))}
        </Group>
      ) : null}

      <Group title="From Hugging Face">
        <Row
          label="Repository"
          hint="Paste a repo name to list the transcription models inside it."
          stacked
        >
          <div className="inline-form">
            <TextField
              label="Hugging Face repository"
              value={repo}
              placeholder="e.g. unslothai/Qwen3-ASR-0.6B-GGUF"
              onChange={(v) => {
                setRepo(v);
                setFiles(null);
                setError(null);
              }}
              onEnter={() => void browse()}
            />
            <Button onClick={() => void browse()} disabled={!repo.trim() || busy}>
              {busy ? "Listing…" : "Files"}
            </Button>
          </div>
        </Row>

        {error ? <Note tone="danger">{error}</Note> : null}

        {files && !busy ? (
          files.length === 0 ? (
            <Note>No transcription models (.gguf or ggml-*.bin) in this repository.</Note>
          ) : (
            files.map((f) => (
              <Row key={f.path} label={<span className="ellipsis">{f.path}</span>}
                   hint={formatBytes(f.size)}>
                <Button
                  variant="primary"
                  onClick={() => {
                    void api
                      .hfInstall(repo.trim(), f.path, f.size, f.sha256)
                      .then(() => {
                        setFiles(null);
                        refresh();
                        reloadModels();
                      })
                      .catch((err) => setError(String(err)));
                  }}
                >
                  Install
                </Button>
              </Row>
            ))
          )
        ) : null}

        {progress && progress.id.startsWith("hf|") ? (
          <Progress percent={progress.percent} />
        ) : null}
      </Group>

      <Note>
        A model is registered only once it has actually loaded, so a repository that
        looks right but cannot run is refused rather than half-installed. Whisper-family
        GGUF models work today; Parakeet, Moonshine and Canary need a second engine that
        is not wired up yet.
      </Note>
    </Pane>
  );
}
