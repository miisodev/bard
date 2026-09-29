import { useState } from "react";
import { ExternalLink, Eye, EyeOff, LoaderCircle } from "lucide-react";
import { api } from "../store";
import { InlineConfirm } from "../settings/ui";
import type { LicenseStatus } from "../types";

const PURCHASE_URL = "https://miiso.dev/tools/bard";
/** What a licence buys: one payment, this many computers, no account. */
const ACTIVATION_LIMIT = 3;

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

/**
 * The commercial terms, in the card rather than only in a Terms hint three
 * screens away. Both strings render only when the backend reports the numbers;
 * nothing here invents a value.
 */
function terms(status: LicenseStatus): string[] {
  const lines: string[] = [];
  if (typeof status.activationCount === "number") {
    const limit = status.activationLimit ?? ACTIVATION_LIMIT;
    const used = Math.min(status.activationCount, limit);
    lines.push(`Active on ${used} of ${limit} PCs`);
  }
  if (typeof status.offlineLeaseDaysLeft === "number") {
    const days = Math.max(0, Math.round(status.offlineLeaseDaysLeft));
    lines.push(`Offline licence · ${days} ${days === 1 ? "day" : "days"} left`);
  }
  return lines;
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
  const [confirmRelease, setConfirmRelease] = useState(false);

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

  /**
   * Freeing this computer's activation.
   *
   * A lapsed or restricted licence used to have no way out at all: only
   * Activate and Buy, so a user who wanted to move the licence to another
   * machine could not. `deactivate_license` is the real release — it tells the
   * service and frees the slot. It fails when there is no cached receipt to
   * release, which is exactly the state a restricted machine is in, and
   * `clear_license` is the local half. When only the local half succeeds, that
   * is said plainly rather than reported as a freed slot.
   */
  const release = async () => {
    if (busy) return;
    setConfirmRelease(false);
    setBusy(true);
    setError(null);
    try {
      onStatus?.(await api.deactivateLicense());
    } catch (reason) {
      try {
        onStatus?.(await api.clearLicense());
        setError(
          "The key was removed from this computer. Bard could not reach the licence service, so the activation itself is unchanged.",
        );
      } catch {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    } finally {
      setBusy(false);
    }
  };

  const active = status.state === "active";
  const trial = status.state === "trial";
  const trialAvailable = status.state === "trial_available";
  const restricted = status.state === "restricted";
  const label = active ? "Licensed" : trial ? `${daysLeft(status.trialExpiresAt)} days left in trial` : trialAvailable ? "14-day local trial" : restricted ? "Activation required" : "Licence status";
  // A stored key is a slot this computer may be holding, whatever state the
  // licence is in, so releasing it has to be offered wherever there is one.
  const releasable = status.keyConfigured;
  const termLines = terms(status);

  return (
    <section className={`license-card${compact ? " compact" : ""}`} aria-labelledby="bard-license-title">
      <div className="license-card-head">
        <h2 id="bard-license-title">{label}</h2>
      </div>
      <p className="license-card-copy">
        {active
          ? `Valid until ${dateLabel(status.expiresAt)}.`
          : trial
            ? `Try Bard free for ${daysLeft(status.trialExpiresAt)} more days. No card is required.`
            : restricted
              ? "Your timeline remains readable. Buy another licence to capture, dictate, draw, and edit."
              : "Bard is checking its local licence state."}
      </p>
      {/*
        The backend's own message, in every state. It used to render only when
        the licence was neither active nor on trial, which is exactly when the
        one message that matters — "Offline licence check pending." on a live
        licence — was thrown away.
      */}
      {status.message ? (
        <p className="license-card-note" role="status">{status.message}</p>
      ) : null}
      {termLines.length > 0 ? (
        <p className="license-card-note">
          {termLines.map((line, index) => (
            <span key={line}>
              {index > 0 ? " · " : null}
              {line}
            </span>
          ))}
        </p>
      ) : null}

      {active ? (
        <div className="license-card-actions">
          <button type="button" className="link-button" onClick={() => void api.openLink(PURCHASE_URL)}>
            Manage licence
          </button>
          <button type="button" className="link-button" onClick={() => void refresh()} disabled={busy}>
            {busy ? "Checking…" : "Check now"}
          </button>
          <button type="button" className="link-button" onClick={() => setConfirmRelease(true)} disabled={busy}>
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
          <button type="button" className="btn" onClick={() => void api.openLink(PURCHASE_URL)}>
            Buy <ExternalLink size={14} aria-hidden="true" />
          </button>
        </div>
      )}

      {/*
        Manage and release, outside the active state. Without these a user whose
        licence lapsed could neither move the licence to another computer nor
        free the slot this one is holding.
      */}
      {!active && releasable ? (
        <div className="license-card-actions">
          <button type="button" className="link-button" onClick={() => void refresh()} disabled={busy}>
            {busy ? "Checking…" : "Check again"}
          </button>
          <button type="button" className="link-button" onClick={() => setConfirmRelease(true)} disabled={busy}>
            Release this PC
          </button>
        </div>
      ) : null}

      {confirmRelease ? (
        <InlineConfirm
          onCancel={() => setConfirmRelease(false)}
          onConfirm={() => void release()}
          confirming={busy}
          confirmLabel="Release this PC"
        >
          Release this computer's activation? You can activate again later if a slot is
          free.
        </InlineConfirm>
      ) : null}

      {error ? <p className="form-error" role="alert">{error}</p> : null}
    </section>
  );
}
