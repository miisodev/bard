import { useState } from "react";
import { ExternalLink, Eye, EyeOff, KeyRound, LoaderCircle, RefreshCw } from "lucide-react";
import { api } from "../store";
import type { LicenseStatus } from "../types";

function dateLabel(seconds: number | null): string {
  if (!seconds) return "";
  return new Date(seconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function daysLeft(seconds: number | null): number {
  if (!seconds) return 0;
  return Math.max(0, Math.ceil((seconds * 1000 - Date.now()) / 86_400_000));
}

export default function LicenseCard({
  status,
  onStatus,
  compact = false,
}: {
  status: LicenseStatus;
  onStatus?: (status: LicenseStatus) => void;
  compact?: boolean;
}) {
  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const activate = async () => {
    if (!key.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await api.activateLicense(key.trim());
      setKey("");
      onStatus?.(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const startTrial = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      onStatus?.(await api.startTrial());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const refresh = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await api.refreshLicense();
      onStatus?.(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const release = async () => {
    if (busy || !window.confirm("Release this computer's activation? You can activate it again later if a slot is available.")) return;
    setBusy(true);
    setError(null);
    try {
      onStatus?.(await api.deactivateLicense());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const active = status.state === "active";
  const trial = status.state === "trial";
  const trialAvailable = status.state === "trial_available";
  const restricted = status.state === "restricted";
  const label = active ? "Active licence" : trial ? `${daysLeft(status.trialExpiresAt)} days left in trial` : trialAvailable ? "14-day local trial" : restricted ? "Activation required" : "Licence status";

  return (
    <section className={`license-card${compact ? " compact" : ""}`} aria-labelledby="bard-license-title">
      <div className="license-card-head">
        <div>
          <p className="eyebrow">Licence</p>
          <h2 id="bard-license-title">{label}</h2>
        </div>
        <KeyRound size={18} aria-hidden="true" />
      </div>
      <p className="license-card-copy">
        {active
          ? `Valid through ${dateLabel(status.expiresAt)}. Your work stays on this computer.`
          : trial
            ? `Try Bard free for ${daysLeft(status.trialExpiresAt)} more days. No card is required.`
            : restricted
              ? "Your timeline remains readable. Buy another licence to capture, dictate, draw, and edit."
              : status.message || "Bard is checking its local licence state."}
      </p>
      {status.message && !active && !trial ? <p className="license-card-note" role="status">{status.message}</p> : null}
      {active ? (
        <div className="license-card-actions">
          <button type="button" className="btn" onClick={() => void refresh()} disabled={busy}>
            {busy ? <LoaderCircle className="spin" size={14} aria-hidden="true" /> : <RefreshCw size={14} aria-hidden="true" />}
            Check
          </button>
          <button type="button" className="btn" onClick={() => void api.openLink("https://miiso.dev/tools/bard")}>
            Manage <ExternalLink size={14} aria-hidden="true" />
          </button>
          <button type="button" className="link-button" onClick={() => void release()} disabled={busy}>
            Release this PC
          </button>
        </div>
      ) : (
        <div className="license-activation">
          {trialAvailable ? (
            <button type="button" className="btn primary" onClick={() => void startTrial()} disabled={busy}>
              {busy ? <LoaderCircle className="spin" size={14} aria-hidden="true" /> : null}
              Start 14-day trial
            </button>
          ) : null}
          <label className="sr-only" htmlFor="bard-license-key">Bard licence key</label>
          <div className="license-input-wrap">
            <input
              id="bard-license-key"
              className="license-input"
              type={showKey ? "text" : "password"}
              value={key}
              onChange={(event) => setKey(event.target.value)}
              onKeyDown={(event) => { if (event.key === "Enter") void activate(); }}
              placeholder="BARD-…"
              autoComplete="off"
              spellCheck={false}
              maxLength={512}
            />
            <button
              type="button"
              className="icon-btn"
              aria-label={showKey ? "Hide licence key" : "Show licence key"}
              onClick={() => setShowKey((visible) => !visible)}
            >
              {showKey ? <EyeOff size={14} aria-hidden="true" /> : <Eye size={14} aria-hidden="true" />}
            </button>
          </div>
          <button type="button" className="btn primary" onClick={() => void activate()} disabled={busy || !key.trim()}>
            {busy ? <LoaderCircle className="spin" size={14} aria-hidden="true" /> : null}
            Activate
          </button>
          {/* A trial with no way to buy is a dead end. */}
          <button type="button" className="btn" onClick={() => void api.openLink("https://miiso.dev/tools/bard")}>
            Buy <ExternalLink size={14} aria-hidden="true" />
          </button>
        </div>
      )}
      {error ? <p className="form-error" role="alert">{error}</p> : null}
      {status.keyConfigured && !active ? (
        <button type="button" className="link-button" onClick={() => void refresh()} disabled={busy}>Check saved licence</button>
      ) : null}
    </section>
  );
}
