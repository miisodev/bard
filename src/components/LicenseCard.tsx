import { useState } from "react";
import { ExternalLink, Eye, EyeOff, LoaderCircle, X } from "lucide-react";
import { api } from "../store";
import { InlineConfirm } from "../settings/ui";
import type { LicenseStatus } from "../types";

const PURCHASE_URL = "https://miiso.dev/bard";
/** What a licence buys: one payment, this many computers, no account. */
const ACTIVATION_LIMIT = 3;
/** How close to the end the calm expiry note appears. */
const EXPIRY_WARNING_DAYS = 7;

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
  // The count the backend records is what *this* PC has observed of its own
  // activations, not the buyer's live figure — the signed lease carries no
  // count and the broker sends none — so it is deliberately not rendered as
  // one. The limit is a promise; the Polar portal is where the real list is.
  const limit = status.activationLimit ?? ACTIVATION_LIMIT;
  if (typeof status.activationLimit === "number") {
    lines.push(`Activates on up to ${limit} ${limit === 1 ? "computer" : "computers"}`);
  }
  if (typeof status.offlineLeaseDaysLeft === "number") {
    // The lease renews on every daily check, so this only counts down while the
    // PC is offline, and the backend sends it only while the licence is usable.
    const days = Math.max(0, Math.round(status.offlineLeaseDaysLeft));
    lines.push(`Works offline for ${days} ${days === 1 ? "day" : "days"}`);
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
  const [warningDismissed, setWarningDismissed] = useState(false);

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
   * The backend decides what happened and says so: with no cached receipt there
   * is no activation id the service can release, and it answers with that and
   * the route to the portal instead of an error. When the call itself fails,
   * its own sentence is shown — it knows whether the service could not be
   * reached or refused the release — and the local key stays where it is either
   * way, because it is the credential the portal and support identify the
   * purchase by.
   */
  const release = async () => {
    if (busy) return;
    setConfirmRelease(false);
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
  const label = active ? "Licensed" : trial ? `${daysLeft(status.trialExpiresAt)} days left in trial` : trialAvailable ? "14-day local trial" : restricted ? "Activation required" : "Licence status";
  // A stored key is a slot this computer may be holding, whatever state the
  // licence is in, so releasing it has to be offered wherever there is one.
  const releasable = status.keyConfigured;
  const termLines = terms(status);
  // A licence or a trial that is ending is a date, not an event: one calm strip
  // inside the card, which can be put away, rather than a toast or an alert.
  const endingDaysLeft =
    active && status.expiresAt
      ? daysLeft(status.expiresAt)
      : trial && status.trialExpiresAt
        ? daysLeft(status.trialExpiresAt)
        : null;
  const expiryWarning =
    !warningDismissed && endingDaysLeft !== null && endingDaysLeft <= EXPIRY_WARNING_DAYS;

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
              ? "Your shelf remains readable. Buy another licence to capture, dictate, draw, and edit."
              : trialAvailable
                ? "There is no trial or licence on this PC yet."
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
      {/*
        Revalidation retries by design, and used to say nothing either way, so
        a service that could not be reached looked exactly like a licence that
        never needed checking. This keeps the calm offline state and just says
        the check is being retried; a successful check clears it.
      */}
      {status.revalidationFailing ? (
        <p className="license-card-note" role="status">
          Bard can't reach the licence service — retrying in the background. The licence
          keeps working while Bard tries again.
        </p>
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

      {expiryWarning ? (
        <div className="license-banner" role="status">
          <span>
            {active
              ? `Your licence ends in ${endingDaysLeft} ${endingDaysLeft === 1 ? "day" : "days"}, on ${dateLabel(status.expiresAt)}. Your timeline stays readable after that.`
              : `Your trial ends in ${endingDaysLeft} ${endingDaysLeft === 1 ? "day" : "days"}, on ${dateLabel(status.trialExpiresAt)}. Buy a licence to keep capture, dictation and draw going.`}
          </span>
          <button
            type="button"
            className="icon-btn"
            aria-label="Dismiss expiry warning"
            onClick={() => setWarningDismissed(true)}
          >
            <X size={14} aria-hidden="true" />
          </button>
        </div>
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
