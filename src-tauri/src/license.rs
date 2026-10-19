//! Bard's local licence gate.
//!
//! The product is usable without an account for a bounded trial. A paid
//! licence is a Polar licence key activated through the fixed miiso.dev
//! licence service. The key itself lives in Windows Credential Manager; the
//! service returns a short, signed receipt that can be checked locally while
//! offline. The receipt is deliberately small and contains no customer data,
//! transcript data, or provider secret.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::ttl::Ttl;

const LICENSE_USERNAME: &str = "license-key";
const INSTALLATION_ID_USERNAME: &str = "installation-id";
const TRIAL_SECRET_USERNAME: &str = "trial-secret";
const TRIAL_STATE_USERNAME: &str = "trial-state";
const TRIAL_DAYS: i64 = 14;
const TRIAL_SETTING: &str = "license_trial_started_at";
const RECEIPT_SETTING: &str = "license_receipt";
const RECEIPT_USERNAME: &str = "license-receipt";
const MACHINE_ID_SETTING: &str = "license_machine_id";
const LAST_CHECK_SETTING: &str = "license_last_checked_at";
/// The last wall-clock time this installation is known to have seen, kept in
/// Credential Manager beside the trial pair. See `observed_now`.
const LAST_SEEN_USERNAME: &str = "last-seen-at";
/// How many distinct machines this key has been activated on, as far as this
/// install has observed. Written when an activation succeeds.
const ACTIVATION_COUNT_SETTING: &str = "license_activation_count";
const DEFAULT_LICENSE_SERVER: &str = "https://miiso.dev/api/bard/license";
/// What "Release this PC" answers when there is no cached activation to release.
/// The seat exists on the service, not here, and only the portal or support can
/// free it without the activation id a receipt carries.
const NO_ACTIVATION_TO_RELEASE: &str = "This computer holds no saved activation to release. To free a seat held on a computer you no longer use, open Manage licence at miiso.dev/bard.";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const KEYRING_TIMEOUT: Duration = Duration::from_millis(400);
const REVALIDATION_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// How long a computed licence status is reused.
///
/// `status` reads Windows Credential Manager several times per answer — the
/// key, the receipt, the trial pair, the machine id — and it is asked on every
/// refresh, every capture and every gated command. The vault's round trips,
/// not the state machine, are what made a plain UI tick cost milliseconds.
/// Two seconds trades that for a bounded staleness: every revocation Bard
/// performs itself (activate, refresh, deactivate, clear, start-trial, and the
/// erase inside delete-all-data) drops this cache immediately, and a change
/// made out of band can be seen up to two seconds late. That is the accepted
/// window; do not widen it without the same debt being written down here.
const STATUS_TTL: Duration = Duration::from_secs(2);

static STATUS_CACHE: Ttl<LicenseStatus> = Ttl::new(STATUS_TTL);

/// The high-water mark of wall-clock time this installation is known to have
/// seen. Read once per process from Credential Manager, then advanced in
/// memory; see `observed_now`.
static LAST_SEEN: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
static LAST_SEEN_READ: AtomicBool = AtomicBool::new(false);

/// How far the wall clock must advance past the stored mark before the mark is
/// written back. The mark exists to bound a clock that has moved backwards, so
/// it needs to be current to within minutes, not seconds; one vault write per
/// quarter of an hour keeps that promise without turning every status read into
/// a write.
const LAST_SEEN_WRITE_INTERVAL_SECONDS: i64 = 15 * 60;

/// How many 15-minute intervals make one heartbeat — four hours.
const REVALIDATION_HEARTBEAT_TICKS: u64 = 16;
static REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// How many revalidation attempts in a row failed before the card says so.
/// One transport blip is not worth a notice; two cycles is a pattern, and the
/// notice clears the moment a check succeeds. The retry cadence itself is
/// unchanged — this only makes it visible.
const REVALIDATION_FAILURE_THRESHOLD: u64 = 2;

/// Consecutive revalidation attempts that could not reach the service.
static REVALIDATION_FAILURES: AtomicU64 = AtomicU64::new(0);

fn revalidation_failures() -> u64 {
    REVALIDATION_FAILURES.load(Ordering::Relaxed)
}

/// Whether the failure count has reached the point where the card should say
/// "retrying". Pure over its input, so the boundary is testable.
fn revalidation_failing(failures: u64) -> bool {
    failures >= REVALIDATION_FAILURE_THRESHOLD
}

/// Record one revalidation outcome: a reached service clears the count, an
/// unreachable one advances it.
fn note_revalidation_outcome(reached: bool) {
    if reached {
        REVALIDATION_FAILURES.store(0, Ordering::Relaxed);
    } else {
        REVALIDATION_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
}

struct RefreshGuard;
impl Drop for RefreshGuard {
    fn drop(&mut self) {
        REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Public halves of the active and next release signing keys. Private keys
/// live only in the miiso.dev signing environment or the operator's escrow;
/// neither is part of the application repository.
const LICENSE_PUBLIC_KEYS: &[(&str, [u8; 32])] = &[
    (
        "2026-01",
        [
            0xc4, 0x6d, 0x1c, 0x09, 0x57, 0xee, 0xb0, 0x0e, 0x04, 0xfc, 0x51, 0xeb, 0xe7, 0xb8,
            0x14, 0x70, 0xa9, 0xa2, 0xc3, 0x6f, 0x19, 0x7d, 0xd3, 0x73, 0xc5, 0xaa, 0xe3, 0x8b,
            0x3c, 0xed, 0x4f, 0xfa,
        ],
    ),
    (
        "2026-02",
        [
            0x67, 0x66, 0xa1, 0x16, 0xd0, 0x85, 0xdd, 0x55, 0xf0, 0xa7, 0x90, 0x8d, 0x7f, 0xf1,
            0xee, 0x33, 0xbf, 0x32, 0x11, 0x49, 0x7d, 0xa4, 0x03, 0x3c, 0x6d, 0xda, 0x67, 0x1e,
            0xd9, 0xaa, 0x1c, 0x33,
        ],
    ),
];

impl LicenseStatus {
    /// Fill in the commercial terms that are the same for every state.
    ///
    /// Five of the six construction sites in `status` are states with no licence
    /// in force — a trial, a key not yet verified, a store still starting — and
    /// all of them need the same three fields. Spelling them out per site is six
    /// chances to add a field later and miss one, which is exactly how the
    /// three-machine limit ended up as prose in a Terms hint.
    fn with_terms(mut self, app: &tauri::AppHandle) -> Self {
        self.activation_count = crate::db::get_setting(app, ACTIVATION_COUNT_SETTING)
            .and_then(|value| value.parse::<i64>().ok());
        self.activation_limit = Some(ACTIVATION_LIMIT);
        self
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    /// `trial_available`, `trial`, `active`, `restricted`, or `unavailable`.
    pub state: String,
    pub can_use: bool,
    pub key_configured: bool,
    pub trial_started_at: Option<i64>,
    pub trial_expires_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub message: Option<String>,
    /// How many of the three permitted machines this key is signed in to, and the
    /// total.
    ///
    /// These are the headline terms of a $20 one-off licence and the product
    /// rendered neither: the three-machine limit was a sentence in a Terms hint,
    /// so a person only discovered it by hitting the wall. `None` when the
    /// broker has not supplied a count, which is the local-only case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_limit: Option<i64>,
    /// Days left in the current offline lease. `None` when the licence was
    /// validated against the service and no offline window is in play.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline_lease_days_left: Option<i64>,
    /// True while licence revalidation has failed for two or more cycles in a
    /// row: the service cannot be reached and the client keeps retrying on its
    /// normal schedule. The licence keeps working on its offline lease; this
    /// only makes the retrying visible instead of silent.
    pub revalidation_failing: bool,
}

/// The commercial terms, in one place. The UI must not hardcode these: a $20
/// licence with three seats is the product's promise, and the number has to come
/// from the same place the enforcement comes from.
pub const ACTIVATION_LIMIT: i64 = 3;

impl LicenseStatus {
    pub fn unavailable() -> Self {
        Self {
            state: "unavailable".into(),
            can_use: false,
            key_configured: false,
            trial_started_at: None,
            trial_expires_at: None,
            expires_at: None,
            last_checked_at: None,
            message: Some("Bard is still starting.".into()),
            activation_count: None,
            activation_limit: None,
            offline_lease_days_left: None,
            revalidation_failing: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Receipt {
    v: u8,
    lease_version: u8,
    product: String,
    key_id: String,
    key_fingerprint: String,
    machine_id: String,
    activation_id: String,
    entitlement: String,
    lease_sequence: i64,
    issued_at: i64,
    license_expires_at: i64,
    valid_until: i64,
    next_check_at: i64,
}

#[derive(Debug)]
pub enum LicenseError {
    InvalidKey,
    CredentialStore,
    /// The stored installation identity is present but is not the 32-hex form
    /// the broker accepts. It is kept and reported, never replaced: a fresh ID
    /// would bind the next activation to a second identity while the original
    /// one still holds its seat.
    DamagedInstallation,
    /// The service has said this key or its activation can no longer be used,
    /// carrying its own words. Only this signal is a revocation.
    Revoked(String),
    Service(String),
    InvalidReceipt,
}

impl fmt::Display for LicenseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("Enter a valid Bard licence key."),
            Self::CredentialStore => {
                f.write_str("Windows Credential Manager could not store the licence key.")
            }
            Self::DamagedInstallation => f.write_str(
                "Bard cannot read this computer's installation identity. Contact support before activating again.",
            ),
            Self::Revoked(message) | Self::Service(message) => f.write_str(message),
            Self::InvalidReceipt => f.write_str("Bard received an invalid licence receipt."),
        }
    }
}

impl std::error::Error for LicenseError {}

#[derive(Debug, Deserialize)]
struct ServiceResponse {
    ok: bool,
    receipt: Option<String>,
    message: Option<String>,
    /// The broker's machine-readable reason, present on every refusal it
    /// decides itself (`{ok: false, code, message}`). It is what tells a
    /// revocation apart from a mistyped key, and both apart from a proxy's
    /// bare 404.
    code: Option<String>,
}

// The three request bodies are a wire contract with the licence broker
// (miisodev `lib/bard-license.ts`), which reads camelCase.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivateRequest<'a> {
    key: &'a str,
    machine_id: &'a str,
    app_version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateRequest<'a> {
    key: &'a str,
    machine_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    activation_id: Option<&'a str>,
    app_version: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeactivateRequest<'a> {
    key: &'a str,
    machine_id: &'a str,
    activation_id: &'a str,
    app_version: &'a str,
}

enum CredentialRead {
    Value(String),
    Absent,
    Unavailable,
}

impl CredentialRead {
    fn value(self) -> Option<String> {
        match self {
            Self::Value(value) => Some(value),
            Self::Absent | Self::Unavailable => None,
        }
    }
}

/// A read that distinguishes "nothing stored" from "could not find out".
///
/// Collapsing the two is what let a slow vault look like a fresh install, which
/// destroyed a paying customer's machine binding irrecoverably: the new random ID
/// overwrote the old one, and the original activation could never validate again.
fn read_credential_bounded(username: &str) -> CredentialRead {
    match crate::credential::read(username, KEYRING_TIMEOUT) {
        crate::credential::Read::Value(value) => CredentialRead::Value(value),
        crate::credential::Read::Absent => CredentialRead::Absent,
        crate::credential::Read::Unavailable => CredentialRead::Unavailable,
    }
}

fn write_credential_bounded(username: &str, value: String) -> Result<(), LicenseError> {
    match crate::credential::write(username, value, KEYRING_TIMEOUT) {
        crate::credential::Done::Ok => Ok(()),
        crate::credential::Done::Unavailable => Err(LicenseError::CredentialStore),
    }
}

fn delete_credential_bounded(username: &str) -> Result<(), LicenseError> {
    match crate::credential::delete(username, KEYRING_TIMEOUT) {
        crate::credential::Done::Ok => Ok(()),
        crate::credential::Done::Unavailable => Err(LicenseError::CredentialStore),
    }
}

/// Read the stored licence key.
///
/// Absence and unavailability both read as "no key", because every caller acts
/// on that the same way. They are still told apart in the log: a vault that stops
/// answering would otherwise look to a paying customer like a licence that
/// silently vanished, and the reason would be nowhere.
fn read_key_bounded() -> Option<String> {
    match read_credential_bounded(LICENSE_USERNAME) {
        CredentialRead::Value(value) => Some(value),
        CredentialRead::Absent => None,
        CredentialRead::Unavailable => {
            crate::crash::record_event("licence:key-unavailable");
            None
        }
    }
}

fn write_key_bounded(key: String) -> Result<(), LicenseError> {
    write_credential_bounded(LICENSE_USERNAME, key)
}

fn delete_key_bounded() -> Result<(), LicenseError> {
    delete_credential_bounded(LICENSE_USERNAME)
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// The clock reading the gate is allowed to believe: never earlier than the last
/// time this installation is known to have seen.
fn believable_now(mark: i64, now: i64) -> i64 {
    mark.max(now)
}

/// The wall clock, floored by the last time this installation is known to have
/// seen.
///
/// Every deadline this gate compares — the trial's end, the signed lease's
/// `valid_until`, the receipt's own temporal bounds — is only as trustworthy as
/// the clock it is compared against, and a clock moved backwards is the oldest
/// way to make a 14-day trial or a 30-day offline lease run longer than either
/// is allowed to. The mark lives in Credential Manager rather than SQLite for
/// the same reason the trial pair does: it is the one store that survives the
/// database, "Delete everything" and a reinstall, so a rolled-back clock cannot
/// wash it away. It records the time and nothing about the person.
///
/// The mark is the higher of what is stored and the clock, and that is the only
/// clamp: backwards it refuses to lose time, and forwards it takes the clock at
/// its word — there is nothing better to believe, and a jump forward can only
/// shorten a window, never extend one. It is written back at most once per
/// interval of elapsed time.
fn observed_now() -> i64 {
    let now = now_seconds();
    let seen = if LAST_SEEN_READ.swap(true, Ordering::SeqCst) {
        LAST_SEEN.load(Ordering::Relaxed)
    } else {
        // One vault read per process: the mark only moves forward, so a value
        // read at the start of a session cannot be stale in the direction that
        // matters.
        let stored = match read_credential_bounded(LAST_SEEN_USERNAME) {
            CredentialRead::Value(value) => value.parse::<i64>().unwrap_or_default(),
            CredentialRead::Absent => 0,
            // A mark that could not be read is not written over: the same
            // mistake as minting a machine id in front of a locked vault.
            CredentialRead::Unavailable => {
                crate::crash::record_event("licence:last-seen-unavailable");
                now
            }
        };
        LAST_SEEN.store(stored, Ordering::Relaxed);
        stored
    };
    let floor = believable_now(seen, now);
    if floor > seen {
        LAST_SEEN.store(floor, Ordering::Relaxed);
        if floor.saturating_sub(seen) >= LAST_SEEN_WRITE_INTERVAL_SECONDS {
            let _ = write_credential_bounded(LAST_SEEN_USERNAME, floor.to_string());
        }
    }
    floor
}

fn new_machine_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn is_machine_id(candidate: &str) -> bool {
    candidate.len() == 32
        && candidate
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

/// The one spelling the broker accepts: 32 lowercase hex digits. A stored ID in
/// any other case is the same installation, so it is lowercased, never reminted.
fn canonical_machine_id(candidate: &str) -> Option<String> {
    is_machine_id(candidate).then(|| candidate.to_ascii_lowercase())
}

/// What a credential-stored installation id is worth.
enum StoredMachineId {
    /// 32 hex digits, in whatever case or padding an earlier build left them:
    /// the same installation, sent in the broker's one spelling.
    Usable(String),
    /// Present, but not an installation id. Damage, not absence.
    Damaged,
}

/// Judge a stored id without touching it. Trimming first means a stray newline
/// an older writer left behind is padding, not damage.
fn judge_machine_id(stored: &str) -> StoredMachineId {
    match canonical_machine_id(stored.trim()) {
        Some(canonical) => StoredMachineId::Usable(canonical),
        None => StoredMachineId::Damaged,
    }
}

/// The stable identifier this installation is bound to.
///
/// The ordering here is the whole point. A credential read that did not *answer*
/// must never fall through to minting a new ID: the new value overwrites the old
/// one, and because the receipt was signed against the old one, the customer's
/// original activation can never validate on this machine again. That turned one
/// slow vault into permanent loss of a paid licence, and Credential Manager
/// stalls are ordinary on domain-joined machines, in RDP sessions, and behind
/// locked vaults.
///
/// A stored value that is not a well-formed ID is damage of the same kind, and
/// it is neither replaced with a fresh identity nor silently overwritten: the
/// next activation would bind to a second identity while the first still holds
/// its seat. The person is told and pointed at support instead. Only a confirmed
/// `NoEntry` is permission to create.
fn machine_id(app: &tauri::AppHandle) -> Result<String, LicenseError> {
    match read_credential_bounded(INSTALLATION_ID_USERNAME) {
        CredentialRead::Value(existing) => {
            let StoredMachineId::Usable(canonical) = judge_machine_id(&existing) else {
                crate::crash::record_event("licence:machine-id-damaged");
                return Err(LicenseError::DamagedInstallation);
            };
            if canonical != existing {
                // The broker has never accepted another spelling, so no receipt
                // can be bound to it and rewriting loses nothing.
                write_credential_bounded(INSTALLATION_ID_USERNAME, canonical.clone())?;
            }
            return Ok(canonical);
        }
        CredentialRead::Unavailable => {
            crate::crash::record_event("licence:machine-id-unavailable");
            return Err(LicenseError::CredentialStore);
        }
        CredentialRead::Absent => {}
    }

    // Migrate the old opaque value once so existing paid activations remain
    // valid, then remove it from SQLite. New installations never derive an ID
    // from a computer name, username, or hardware string.
    if let Some(existing) = crate::db::get_setting(app, MACHINE_ID_SETTING) {
        if let Some(canonical) = canonical_machine_id(&existing) {
            write_credential_bounded(INSTALLATION_ID_USERNAME, canonical.clone())?;
            let _ = crate::db::set_setting_checked(app, MACHINE_ID_SETTING, "");
            return Ok(canonical);
        }
    }

    let id = new_machine_id();
    write_credential_bounded(INSTALLATION_ID_USERNAME, id.clone())?;
    Ok(id)
}

fn public_key(key_id: &str) -> Result<VerifyingKey, LicenseError> {
    let bytes = LICENSE_PUBLIC_KEYS
        .iter()
        .find_map(|(id, bytes)| (*id == key_id).then_some(*bytes))
        .ok_or(LicenseError::InvalidReceipt)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| LicenseError::InvalidReceipt)
}

fn decode_receipt(encoded: &str) -> Result<Receipt, LicenseError> {
    for (key_id, _bytes) in LICENSE_PUBLIC_KEYS {
        let key = public_key(key_id)?;
        if let Ok(receipt) = decode_receipt_with_key(encoded, &key) {
            if receipt.key_id == *key_id {
                return Ok(receipt);
            }
        }
    }
    Err(LicenseError::InvalidReceipt)
}

fn decode_receipt_with_key(encoded: &str, key: &VerifyingKey) -> Result<Receipt, LicenseError> {
    let (payload, signature) = encoded
        .split_once('.')
        .ok_or(LicenseError::InvalidReceipt)?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| LicenseError::InvalidReceipt)?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| LicenseError::InvalidReceipt)?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|_| LicenseError::InvalidReceipt)?;
    key.verify(&payload, &signature)
        .map_err(|_| LicenseError::InvalidReceipt)?;
    let receipt: Receipt =
        serde_json::from_slice(&payload).map_err(|_| LicenseError::InvalidReceipt)?;
    if receipt.v != 1
        || receipt.lease_version != 1
        || receipt.product != "bard"
        || receipt.key_id.is_empty()
        || receipt.key_fingerprint.len() != 64
        || !receipt
            .key_fingerprint
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || receipt.machine_id.is_empty()
        || receipt.activation_id.is_empty()
        || !matches!(
            receipt.entitlement.as_str(),
            "active" | "suspended" | "ended"
        )
        || receipt.lease_sequence <= 0
    {
        return Err(LicenseError::InvalidReceipt);
    }
    Ok(receipt)
}

fn cached_receipt(app: &tauri::AppHandle) -> Option<Receipt> {
    if let Some(encoded) = read_credential_bounded(RECEIPT_USERNAME).value() {
        return decode_receipt(&encoded).ok();
    }
    // One-way migration for builds that briefly stored the signed lease in
    // SQLite. The old value is removed immediately after it is secured in
    // Credential Manager.
    let legacy = crate::db::get_setting(app, RECEIPT_SETTING)?;
    let receipt = decode_receipt(&legacy).ok()?;
    if write_credential_bounded(RECEIPT_USERNAME, legacy).is_ok() {
        let _ = crate::db::set_setting_checked(app, RECEIPT_SETTING, "");
    }
    Some(receipt)
}

fn trial_mac(secret: &str, started: i64) -> String {
    let digest = Sha256::digest(format!("bard-trial-v1|{started}|{secret}").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The trial clock is backed by Credential Manager as well as SQLite. Deleting
/// the database therefore cannot silently grant another 14 days, while a
/// failed/altered state fails closed instead of manufacturing a fresh trial.
fn trial_window(app: &tauri::AppHandle) -> Option<(i64, i64)> {
    let now = now_seconds();
    let secret = read_credential_bounded(TRIAL_SECRET_USERNAME).value();
    let state = read_credential_bounded(TRIAL_STATE_USERNAME).value();
    let (secret, started) = match (secret, state) {
        (None, None) => {
            let secret = uuid::Uuid::new_v4().to_string();
            write_credential_bounded(TRIAL_SECRET_USERNAME, secret.clone()).ok()?;
            let started = crate::db::get_setting(app, TRIAL_SETTING)
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(now);
            let state = format!("v1|{}|{}", started, trial_mac(&secret, started));
            write_credential_bounded(TRIAL_STATE_USERNAME, state).ok()?;
            crate::db::set_setting_checked(app, TRIAL_SETTING, &started.to_string()).ok()?;
            (secret, started)
        }
        (Some(secret), Some(raw)) => {
            let mut parts = raw.split('|');
            let version = parts.next()?;
            let started = parts.next()?.parse::<i64>().ok()?;
            let mac = parts.next()?;
            if version != "v1" || parts.next().is_some() || mac != trial_mac(&secret, started) {
                return None;
            }
            (secret, started)
        }
        _ => return None,
    };
    let _ = secret;
    Some((started, started.saturating_add(TRIAL_DAYS * 24 * 60 * 60)))
}

fn existing_trial_window(_app: &tauri::AppHandle) -> Option<(i64, i64)> {
    let secret = read_credential_bounded(TRIAL_SECRET_USERNAME).value()?;
    let raw = read_credential_bounded(TRIAL_STATE_USERNAME).value()?;
    let mut parts = raw.split('|');
    let version = parts.next()?;
    let started = parts.next()?.parse::<i64>().ok()?;
    let mac = parts.next()?;
    if version != "v1" || parts.next().is_some() || mac != trial_mac(&secret, started) {
        return None;
    }
    Some((started, started.saturating_add(TRIAL_DAYS * 24 * 60 * 60)))
}

pub fn start_trial(app: &tauri::AppHandle) -> Result<LicenseStatus, LicenseError> {
    if read_key_bounded().is_some() {
        return Err(LicenseError::Service(
            "A saved licence already controls Bard.".into(),
        ));
    }
    trial_window(app).ok_or(LicenseError::CredentialStore)?;
    invalidate_status();
    Ok(status(app))
}

/// The bounds inside the receipt's own signature, re-checked so a lease older
/// than the longest one the broker issues — or whose own timestamps disagree —
/// cannot be believed even though the signature is real.
fn temporal_bounds_valid(receipt: &Receipt, now: i64) -> bool {
    receipt.issued_at <= now + 300
        && receipt.issued_at >= now.saturating_sub(30 * 24 * 60 * 60 + 300)
        && receipt.valid_until >= receipt.issued_at
        && receipt.valid_until <= receipt.issued_at.saturating_add(30 * 24 * 60 * 60)
        && receipt.next_check_at >= receipt.issued_at
        && receipt.next_check_at <= receipt.issued_at.saturating_add(24 * 60 * 60 + 300)
        && receipt.license_expires_at >= receipt.issued_at
        && receipt.valid_until <= receipt.license_expires_at
}

/// Whether this signed lease still entitles the machine holding it: the bounds
/// inside the signature, the entitlement, the machine binding, and the two ends
/// of the window — the offline lease's own `valid_until` and the paid term's
/// `license_expires_at`.
fn lease_entitles(receipt: &Receipt, now: i64, machine_matches: bool) -> bool {
    temporal_bounds_valid(receipt, now)
        && receipt.entitlement == "active"
        && receipt.valid_until > now
        && receipt.license_expires_at > now
        && machine_matches
}

fn status_from_receipt(
    app: &tauri::AppHandle,
    receipt: &Receipt,
    key_configured: bool,
    message: Option<String>,
) -> LicenseStatus {
    let (trial_started_at, trial_expires_at) =
        existing_trial_window(app).unwrap_or_else(|| (now_seconds(), now_seconds()));
    let now = observed_now();
    // Offline use is bounded by the lease's own signed `valid_until`, at most
    // 30 days from issue: the window the licence card and the terms promise.
    // The receipt still asks to be re-checked within about a day, and the
    // revalidation loop does that whenever a connection exists, but a laptop
    // that is genuinely offline keeps the whole lease it was signed. A shorter
    // grace made a promise the client did not keep, and a longer window is not
    // one the broker ever signs.
    let machine = machine_id(app);
    let machine_matches = machine
        .as_ref()
        .map(|machine| receipt.machine_id == *machine)
        .unwrap_or(false);
    let valid = lease_entitles(receipt, now, machine_matches);
    let default_message = match receipt.entitlement.as_str() {
        "suspended" => Some("This Bard licence is temporarily suspended.".into()),
        "ended" => {
            Some("This Bard licence has ended. Buy another licence to keep creating.".into())
        }
        // A receipt that cannot be matched because the stored identity is
        // damaged has a different answer from one that has simply expired, and
        // it is the one the person can act on.
        _ if matches!(machine, Err(LicenseError::DamagedInstallation)) => {
            Some(LicenseError::DamagedInstallation.to_string())
        }
        _ => Some("The Bard licence has expired or is not valid on this computer.".into()),
    };
    LicenseStatus {
        state: if valid { "active" } else { "restricted" }.into(),
        can_use: valid,
        key_configured,
        trial_started_at: Some(trial_started_at),
        trial_expires_at: Some(trial_expires_at),
        expires_at: Some(receipt.license_expires_at),
        last_checked_at: crate::db::get_setting(app, LAST_CHECK_SETTING)
            .and_then(|value| value.parse::<i64>().ok()),
        message: if valid {
            message.or_else(|| {
                (receipt.next_check_at <= now).then(|| "Offline licence check pending.".into())
            })
        } else {
            default_message
        },
        // What this machine has observed of its own activations. The signed
        // receipt identifies this activation but carries no count, and the
        // broker does not send one, so this number is not the buyer's live
        // figure and is not rendered as one: the card states the limit only,
        // and the Polar portal is where the real list lives.
        activation_count: crate::db::get_setting(app, ACTIVATION_COUNT_SETTING)
            .and_then(|value| value.parse::<i64>().ok()),
        activation_limit: Some(ACTIVATION_LIMIT),
        // Only while the licence is usable. Beside "Activation required", a
        // countdown to the end of a lease reads as a promise the card is not
        // keeping.
        offline_lease_days_left: (valid && receipt.valid_until > now).then(|| {
            receipt
                .valid_until
                .saturating_sub(now)
                .div_euclid(24 * 60 * 60)
        }),
        revalidation_failing: false,
    }
}

pub fn require(app: &tauri::AppHandle) -> Result<(), String> {
    let status = status(app);
    if status.can_use {
        Ok(())
    } else {
        Err(status
            .message
            .unwrap_or_else(|| "Activate a Bard licence to continue.".into()))
    }
}

/// Drop the cached status, so the next read recomputes it.
///
/// Every path that can change what `status` would answer calls this — the
/// activate/refresh/deactivate/clear/start-trial actions and the local erase
/// behind "Delete all data". Nothing else may write the cache directly.
fn invalidate_status() {
    STATUS_CACHE.invalidate();
}

/// The current status with a one-off note, for an action that completed without
/// changing anything but owes the person an explanation.
fn status_with_message(app: &tauri::AppHandle, message: &str) -> LicenseStatus {
    let mut current = status(app);
    current.message = Some(message.to_owned());
    current
}

pub fn status(app: &tauri::AppHandle) -> LicenseStatus {
    if !crate::db::is_ready(app) {
        return LicenseStatus::unavailable();
    }
    let mut status = match STATUS_CACHE.get() {
        Some(cached) => cached,
        None => {
            let computed = compute_status(app);
            STATUS_CACHE.set(computed.clone());
            computed
        }
    };
    // Stamped on every read rather than cached: the retry counter changes
    // between two status reads, and the card has to clear the moment a check
    // succeeds.
    status.revalidation_failing = revalidation_failing(revalidation_failures());
    status
}

fn compute_status(app: &tauri::AppHandle) -> LicenseStatus {
    let key_configured = read_key_bounded().is_some();
    if let Some(receipt) = cached_receipt(app) {
        return status_from_receipt(app, &receipt, key_configured, None);
    }
    if key_configured {
        return LicenseStatus {
            state: "restricted".into(),
            can_use: false,
            key_configured,
            trial_started_at: None,
            trial_expires_at: None,
            expires_at: None,
            last_checked_at: None,
            message: Some("Re-enter or refresh this Bard licence to continue.".into()),
            activation_count: None,
            activation_limit: None,
            offline_lease_days_left: None,
            revalidation_failing: false,
        }
        .with_terms(app);
    }
    let Some((started, expires)) = existing_trial_window(app) else {
        if matches!(
            read_credential_bounded(TRIAL_STATE_USERNAME),
            CredentialRead::Absent
        ) {
            return LicenseStatus {
                state: "trial_available".into(),
                can_use: false,
                key_configured,
                trial_started_at: None,
                trial_expires_at: None,
                expires_at: None,
                last_checked_at: None,
                message: Some("Start Bard's 14-day local trial when you are ready.".into()),
                activation_count: None,
                activation_limit: None,
                offline_lease_days_left: None,
                revalidation_failing: false,
            }
            .with_terms(app);
        }
        return LicenseStatus {
            state: "restricted".into(),
            can_use: false,
            key_configured,
            trial_started_at: None,
            trial_expires_at: None,
            expires_at: None,
            last_checked_at: None,
            message: Some(
                "Bard could not verify its trial state. Reinstall or contact support.".into(),
            ),
            activation_count: None,
            activation_limit: None,
            offline_lease_days_left: None,
            revalidation_failing: false,
        }
        .with_terms(app);
    };
    let now = observed_now();
    if now < expires {
        return LicenseStatus {
            state: "trial".into(),
            can_use: true,
            key_configured,
            trial_started_at: Some(started),
            trial_expires_at: Some(expires),
            expires_at: None,
            last_checked_at: crate::db::get_setting(app, LAST_CHECK_SETTING)
                .and_then(|value| value.parse::<i64>().ok()),
            message: None,
            activation_count: None,
            activation_limit: None,
            offline_lease_days_left: None,
            revalidation_failing: false,
        }
        .with_terms(app);
    }
    LicenseStatus {
        state: "restricted".into(),
        can_use: false,
        key_configured,
        trial_started_at: Some(started),
        trial_expires_at: Some(expires),
        expires_at: None,
        last_checked_at: crate::db::get_setting(app, LAST_CHECK_SETTING)
            .and_then(|value| value.parse::<i64>().ok()),
        message: Some("Activate a Bard licence to continue using the app.".into()),
        activation_count: None,
        activation_limit: None,
        offline_lease_days_left: None,
        revalidation_failing: false,
    }
    .with_terms(app)
}

fn service_url(path: &str) -> String {
    let base = if cfg!(debug_assertions) {
        std::env::var("BARD_LICENSE_SERVER")
            .ok()
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
            .unwrap_or_else(|| DEFAULT_LICENSE_SERVER.to_string())
    } else {
        DEFAULT_LICENSE_SERVER.to_string()
    };
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// The service's sentence when its body carries none, chosen by status.
fn default_service_message(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => "That licence key was not accepted.".into(),
        StatusCode::NOT_FOUND => "That licence key was not found.".into(),
        StatusCode::TOO_MANY_REQUESTS => "The licence service is busy. Try again shortly.".into(),
        StatusCode::PAYMENT_REQUIRED => {
            "The licence service could not confirm this licence.".into()
        }
        _ => "The licence service could not complete the request.".into(),
    }
}

/// A service message as it may be rendered: bounded and free of control
/// characters, so nothing the broker or a proxy sends back can rewrite the
/// window it is shown in.
fn sanitise_message(message: &str) -> String {
    let mut output = String::new();
    for character in message.chars().take(240) {
        if character.is_control() {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

/// The reason a request was refused, in the service's own words where it has
/// them.
///
/// The broker answers every refusal it decides with `{ok: false, code, message}`
/// and the client used to throw the body away for 401/403/404, so a mistyped
/// key, a fourth computer and a refunded key all read "This Bard licence is no
/// longer active." The code answers the second question too: whether the cached
/// receipt is dead. Only an explicit revocation is; a bare 404 from a proxy or a
/// stale route has no code and is a failure to complete the request.
fn service_error(status: StatusCode, body: &str) -> LicenseError {
    let fallback = default_service_message(status);
    let (code, raw) = match serde_json::from_str::<ServiceResponse>(body) {
        Ok(response) => (
            response.code.unwrap_or_default(),
            response.message.unwrap_or(fallback),
        ),
        Err(_) => (String::new(), fallback),
    };
    let mut message = sanitise_message(&raw);
    if message.trim().is_empty() {
        message = default_service_message(status);
    }
    match code.as_str() {
        "LICENSE_INACTIVE" | "KEY_NOT_FOUND" => LicenseError::Revoked(message),
        // Polar refuses a fourth activation the same way it refuses an
        // activation on a key it will not honour, so the broker's sentence
        // cannot say which happened. The seat limit is the one a person can do
        // something about, and the place to do it is the portal.
        "ACTIVATION_REJECTED" => {
            message.push_str(
                " Free a seat from Manage licence on miiso.dev if three computers are already active.",
            );
            LicenseError::Service(message)
        }
        _ => LicenseError::Service(message),
    }
}

async fn post_service(
    path: &str,
    body: serde_json::Value,
) -> Result<ServiceResponse, LicenseError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        // The body carries the licence key. A 307 or 308 preserves the method
        // and the body by HTTP semantics, so reqwest's default follow policy
        // would re-issue the key to whatever host a `Location` header named. The
        // licence service is a fixed endpoint with no legitimate reason to
        // redirect, so refusing to follow is the correct behaviour rather than a
        // limitation.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| LicenseError::Service("The licence service could not be reached.".into()))?;
    let response = client
        .post(service_url(path))
        .json(&body)
        .send()
        .await
        .map_err(|_| LicenseError::Service("The licence service could not be reached.".into()))?;
    let status = response.status();
    let text = response.text().await.map_err(|_| {
        LicenseError::Service("The licence service returned an unreadable response.".into())
    })?;
    if !status.is_success() {
        return Err(service_error(status, &text));
    }
    let response: ServiceResponse = serde_json::from_str(&text).map_err(|_| {
        LicenseError::Service("The licence service returned an invalid response.".into())
    })?;
    if !response.ok {
        return Err(LicenseError::Service(response.message.unwrap_or_else(
            || "The licence service rejected the request.".into(),
        )));
    }
    Ok(response)
}

fn key_fingerprint(key: &str) -> String {
    Sha256::digest(key.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn store_response_receipt(
    app: &tauri::AppHandle,
    response: ServiceResponse,
    key_configured: bool,
    expected_key: &str,
) -> Result<LicenseStatus, LicenseError> {
    let encoded = response.receipt.ok_or(LicenseError::InvalidReceipt)?;
    let receipt = decode_receipt(&encoded)?;
    let machine = machine_id(app)?;
    if receipt.machine_id != machine || receipt.key_fingerprint != key_fingerprint(expected_key) {
        return Err(LicenseError::InvalidReceipt);
    }
    if let Some(previous) = cached_receipt(app) {
        if receipt.lease_sequence < previous.lease_sequence
            || (receipt.lease_sequence == previous.lease_sequence
                && receipt.issued_at < previous.issued_at)
        {
            return Err(LicenseError::InvalidReceipt);
        }
    }
    write_credential_bounded(RECEIPT_USERNAME, encoded.clone())?;
    // Record that this machine now holds an activation. The count is what this
    // install has observed of its own activations, not the buyer's live figure:
    // the signed receipt carries none and the broker sends none, so the card
    // states the limit rather than a number that would be wrong on a second PC.
    let observed = crate::db::get_setting(app, ACTIVATION_COUNT_SETTING)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        .clamp(0, ACTIVATION_LIMIT);
    let _ =
        crate::db::set_setting_checked(app, ACTIVATION_COUNT_SETTING, &observed.max(1).to_string());
    crate::db::set_setting_checked(app, LAST_CHECK_SETTING, &now_seconds().to_string()).map_err(
        |_| LicenseError::Service("Bard could not persist its licence check time.".into()),
    )?;
    let _ = crate::db::set_setting_checked(app, RECEIPT_SETTING, "");
    Ok(status_from_receipt(
        app,
        &receipt,
        key_configured,
        response.message,
    ))
}

pub async fn activate(app: &tauri::AppHandle, key: String) -> Result<LicenseStatus, LicenseError> {
    let mut key = key.trim().to_owned();
    if key.is_empty() || key.len() > 512 || key.chars().any(|character| character.is_control()) {
        key.zeroize();
        return Err(LicenseError::InvalidKey);
    }
    let machine = match machine_id(app) {
        Ok(machine) => machine,
        Err(error) => {
            key.zeroize();
            return Err(error);
        }
    };
    let version = env!("CARGO_PKG_VERSION");
    let request = ActivateRequest {
        key: &key,
        machine_id: &machine,
        app_version: version,
    };
    let payload = match serde_json::to_value(request) {
        Ok(payload) => payload,
        Err(_) => {
            key.zeroize();
            return Err(LicenseError::InvalidKey);
        }
    };
    let response = post_service("activate", payload).await;
    let mut previous_key = read_key_bounded();
    let result = match response {
        Ok(response) => match write_key_bounded(key.clone()) {
            Ok(()) => {
                let stored = store_response_receipt(app, response, true, &key);
                if stored.is_err() {
                    if let Some(previous) = previous_key.take() {
                        let _ = write_key_bounded(previous);
                    } else {
                        let _ = delete_key_bounded();
                    }
                }
                stored
            }
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    };
    if let Some(previous) = previous_key.as_mut() {
        previous.zeroize();
    }
    key.zeroize();
    invalidate_status();
    result
}

pub async fn refresh(app: &tauri::AppHandle) -> LicenseStatus {
    let Some(mut key) = read_key_bounded() else {
        return status(app);
    };
    if REFRESH_IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        key.zeroize();
        return status(app);
    }
    let _refresh_guard = RefreshGuard;
    let machine = match machine_id(app) {
        Ok(machine) => machine,
        Err(_) => {
            key.zeroize();
            return status(app);
        }
    };
    let version = env!("CARGO_PKG_VERSION");
    let activation_id = cached_receipt(app).map(|receipt| receipt.activation_id);
    let request = ValidateRequest {
        key: &key,
        machine_id: &machine,
        activation_id: activation_id.as_deref(),
        app_version: version,
    };
    let result = post_service(
        "validate",
        serde_json::to_value(request).unwrap_or_default(),
    )
    .await;
    let refreshed = match result {
        Ok(response) => {
            // The service answered, so the retrying state clears even when the
            // answer itself is rejected — the loop is talking to it again.
            note_revalidation_outcome(true);
            match store_response_receipt(app, response, true, &key) {
                Ok(status) => status,
                Err(_) => status(app),
            }
        }
        Err(LicenseError::Revoked(message)) => {
            note_revalidation_outcome(true);
            let _ = delete_credential_bounded(RECEIPT_USERNAME);
            let _ = crate::db::set_setting_checked(app, RECEIPT_SETTING, "");
            // Keep the service's own words. "Not found" and "not active" are
            // different problems, and the old generic sentence said neither.
            let mut revoked = status(app);
            revoked.message = Some(message);
            revoked
        }
        Err(_) => {
            note_revalidation_outcome(false);
            status(app)
        }
    };
    key.zeroize();
    invalidate_status();
    refreshed
}

/// Keep the signed lease fresh.
///
/// This loop is the only thing that enforces a server-side revocation while the
/// app is running, so its death is a security-relevant event, not a cosmetic
/// one — and previously it died silently, with nothing recorded anywhere. It
/// also cannot die quietly from a panic any more: a panicking iteration is
/// caught and the loop continues, so one bad status read cannot quietly stop
/// enforcement for the rest of the session.
pub fn spawn_revalidation(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut ticks: u64 = 0;
        // The first pass runs before the first sleep, not 15 minutes in. A
        // licence that is already past its recheck point would otherwise be
        // unenforced for a quarter of an hour after launch.
        loop {
            ticks += 1;
            // Every four hours, and only while a licence is actually in play.
            // An idle install writes nothing.
            if ticks.is_multiple_of(REVALIDATION_HEARTBEAT_TICKS) {
                let configured = read_key_bounded().is_some();
                if configured {
                    crate::crash::record_event(&format!("licence:revalidation-alive:{ticks}"));
                }
            }
            let stepped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let current = status(&app);
                if !current.key_configured {
                    return;
                }
                // `observed_now` is also where the last-seen mark advances, so
                // this loop writes it about every quarter of an hour on a
                // running install.
                let due = cached_receipt(&app)
                    .map(|receipt| receipt.next_check_at <= observed_now())
                    .unwrap_or(false);
                if due {
                    let _ = tauri::async_runtime::block_on(refresh(&app));
                }
            }));
            if stepped.is_err() {
                crate::crash::record_event("licence:revalidation-iteration-failed");
            }
            // At the end of the pass, not the start, so the immediate first
            // iteration above is not followed by another straight away.
            std::thread::sleep(REVALIDATION_INTERVAL);
        }
    });
}

pub async fn deactivate(app: &tauri::AppHandle) -> Result<LicenseStatus, LicenseError> {
    let Some(mut key) = read_key_bounded() else {
        return Ok(status(app));
    };
    // A machine with no cached receipt is the *normal* state of a restricted or
    // lapsed install. It cannot release a seat: the broker's deactivate needs the
    // activation id that only a receipt carries — `validateInput` rejects an
    // empty one as invalid and the call would be answered 400 — and Polar's own
    // deactivate is by activation id too. Asking anyway burned a request to be
    // told nothing, and the UI papered over it by clearing the local key and
    // blaming the network while the seat stayed taken.
    //
    // So this answers with what is true and where the seat can actually be
    // freed, and touches neither the key nor any local state: the key is the
    // credential the portal and support identify the purchase by.
    let Some(activation_id) = cached_receipt(app).map(|receipt| receipt.activation_id) else {
        crate::crash::record_event("licence:deactivate-without-receipt");
        return Ok(status_with_message(app, NO_ACTIVATION_TO_RELEASE));
    };
    let machine = match machine_id(app) {
        Ok(machine) => machine,
        Err(error) => {
            key.zeroize();
            return Err(error);
        }
    };
    let request = DeactivateRequest {
        key: &key,
        machine_id: &machine,
        activation_id: &activation_id,
        app_version: env!("CARGO_PKG_VERSION"),
    };
    let payload = match serde_json::to_value(request) {
        Ok(payload) => payload,
        Err(_) => {
            key.zeroize();
            return Err(LicenseError::InvalidReceipt);
        }
    };
    let result = post_service("deactivate", payload).await;
    key.zeroize();
    result?;
    // The activation count goes with the receipt. Left behind, it renders as
    // "Licensed · 1 activation of 3 used" directly under "Activation required" on a
    // machine that holds no licence at all — the wrong number, on the one screen
    // whose job is to be trustworthy about the licence.
    crate::db::set_settings_checked(
        app,
        &[
            (RECEIPT_SETTING, ""),
            (LAST_CHECK_SETTING, ""),
            (ACTIVATION_COUNT_SETTING, ""),
        ],
    )
    .map_err(|_| LicenseError::Service("Bard could not clear its licence state.".into()))?;
    delete_credential_bounded(RECEIPT_USERNAME)?;
    delete_key_bounded()?;
    invalidate_status();
    Ok(status(app))
}

/// The credentials "Delete everything" removes: the key the person supplied and
/// the signed receipt bound to it. Everything else in the vault is Bard's own
/// bookkeeping — the installation identity, the trial pair, the last-seen mark —
/// and none of it is the person's data.
///
/// The old list took all five. That turned one button into an unlimited trial
/// reset and, worse, a burned paid seat: the next activation minted a new
/// identity while the old activation stayed held. Keeping them is also what the
/// trial pair and the machine-id comments already promise, and what the Data
/// pane now says.
fn erased_credentials() -> [&'static str; 2] {
    [LICENSE_USERNAME, RECEIPT_USERNAME]
}

pub fn erase_local(app: &tauri::AppHandle) -> Result<(), LicenseError> {
    // Every entry is attempted even if one fails, and the first failure is
    // reported at the end: a half-erased set leaves a key without its receipt or
    // the reverse, and the app cannot tell the person which half went.
    let mut first_error = None;
    for entry in erased_credentials() {
        if let Err(error) = delete_credential_bounded(entry) {
            first_error.get_or_insert(error);
        }
    }
    let _ = crate::db::set_setting_checked(app, ACTIVATION_COUNT_SETTING, "");
    // The credentials this cache was built from are gone; a cached `active`
    // would be a stale success on the one path that must never show one.
    invalidate_status();
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn clear(app: &tauri::AppHandle) -> Result<LicenseStatus, LicenseError> {
    crate::db::set_settings_checked(
        app,
        &[
            (RECEIPT_SETTING, ""),
            (LAST_CHECK_SETTING, ""),
            (ACTIVATION_COUNT_SETTING, ""),
        ],
    )
    .map_err(|_| LicenseError::Service("Bard could not clear its licence state.".into()))?;
    delete_credential_bounded(RECEIPT_USERNAME)?;
    delete_key_bounded()?;
    Ok(status(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_random_and_hex_only() {
        let first = new_machine_id();
        let second = new_machine_id();
        assert_ne!(first, second);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }

    /// 2.0.0 and 2.0.1 sent snake_case while the broker read camelCase, and
    /// every activation failed. Pin the field names.
    #[test]
    fn requests_use_the_brokers_camel_case_contract() {
        let id = "db7d4813db8c4d9da34a9bc4329a32d9";
        let activate = serde_json::to_value(ActivateRequest {
            key: "k",
            machine_id: id,
            app_version: "2.0.2",
        })
        .unwrap();
        assert_eq!(
            activate,
            serde_json::json!({"key": "k", "machineId": id, "appVersion": "2.0.2"})
        );
        let validate = serde_json::to_value(ValidateRequest {
            key: "k",
            machine_id: id,
            activation_id: Some("a"),
            app_version: "2.0.2",
        })
        .unwrap();
        assert_eq!(
            validate,
            serde_json::json!({"key": "k", "machineId": id, "activationId": "a", "appVersion": "2.0.2"})
        );
        let unbound = serde_json::to_value(ValidateRequest {
            key: "k",
            machine_id: id,
            activation_id: None,
            app_version: "2.0.2",
        })
        .unwrap();
        assert!(unbound.get("activationId").is_none());
        let deactivate = serde_json::to_value(DeactivateRequest {
            key: "k",
            machine_id: id,
            activation_id: "a",
            app_version: "2.0.2",
        })
        .unwrap();
        assert_eq!(
            deactivate,
            serde_json::json!({"key": "k", "machineId": id, "activationId": "a", "appVersion": "2.0.2"})
        );
    }

    /// A reinstall reads back whatever an earlier build stored: any case of a
    /// well-formed ID is the same installation, sent in the lowercase form the
    /// broker accepts.
    #[test]
    fn stored_ids_are_canonicalised_not_reminted() {
        let upper = "DB7D4813DB8C4D9DA34A9BC4329A32D9";
        assert_eq!(
            canonical_machine_id(upper).as_deref(),
            Some("db7d4813db8c4d9da34a9bc4329a32d9")
        );
        let fresh = new_machine_id();
        assert_eq!(canonical_machine_id(&fresh), Some(fresh.clone()));
        assert_eq!(canonical_machine_id("not-an-id"), None);
        assert_eq!(canonical_machine_id(&upper[..31]), None);
    }

    /// A stored value that is not a well-formed ID is damage, and it is neither
    /// replaced with a fresh identity (which would burn a seat) nor silently
    /// overwritten. Padding around a real ID is not damage.
    #[test]
    fn a_damaged_stored_id_is_reported_not_replaced() {
        let upper = "DB7D4813DB8C4D9DA34A9BC4329A32D9";
        assert!(matches!(
            judge_machine_id(&format!("\n{upper} ")),
            StoredMachineId::Usable(id) if id == "db7d4813db8c4d9da34a9bc4329a32d9"
        ));
        assert!(matches!(
            judge_machine_id("not-an-id"),
            StoredMachineId::Damaged
        ));
        assert!(matches!(
            judge_machine_id(&upper[..31]),
            StoredMachineId::Damaged
        ));
        assert!(matches!(judge_machine_id(""), StoredMachineId::Damaged));
        assert_eq!(
            LicenseError::DamagedInstallation.to_string(),
            "Bard cannot read this computer's installation identity. Contact support before activating again."
        );
    }

    /// "Delete everything" removes the key and the lease and keeps Bard's own
    /// bookkeeping: the installation identity, the trial pair and the last-seen
    /// mark. Keeping them is what stops the button from granting another 14 days
    /// and from burning a paid seat.
    #[test]
    fn delete_everything_keeps_the_licence_machinery() {
        assert_eq!(erased_credentials(), [LICENSE_USERNAME, RECEIPT_USERNAME]);
        for kept in [
            INSTALLATION_ID_USERNAME,
            TRIAL_SECRET_USERNAME,
            TRIAL_STATE_USERNAME,
            LAST_SEEN_USERNAME,
        ] {
            assert!(!erased_credentials().contains(&kept));
        }
    }

    /// The lease's own signed `valid_until` is the offline promise — up to 30
    /// days — and the re-check the broker asks for does not shorten it to days.
    #[test]
    fn a_signed_lease_is_kept_for_its_whole_window() {
        let issued = 1_760_000_000;
        let day = 24 * 60 * 60;
        let receipt = Receipt {
            v: 1,
            lease_version: 1,
            product: "bard".into(),
            key_id: "2026-01".into(),
            key_fingerprint: "0".repeat(64),
            machine_id: "m".into(),
            activation_id: "a".into(),
            entitlement: "active".into(),
            lease_sequence: 1,
            issued_at: issued,
            license_expires_at: issued + 365 * day,
            valid_until: issued + 30 * day,
            next_check_at: issued + 20 * 60 * 60,
        };
        assert!(lease_entitles(&receipt, issued + 29 * day, true));
        assert!(!lease_entitles(&receipt, receipt.valid_until, true));
        // The machine binding and the paid term still decide on their own.
        assert!(!lease_entitles(&receipt, issued + day, false));
        assert!(!lease_entitles(
            &Receipt {
                license_expires_at: issued + 10 * day,
                valid_until: issued + 10 * day,
                ..receipt.clone()
            },
            issued + 11 * day,
            true
        ));
    }

    /// The clock the gate believes never runs before the last time this
    /// installation was seen. A rollback therefore buys nothing, and a jump
    /// forward raises the mark, so the time it claimed cannot be recovered by
    /// moving the clock back afterwards.
    #[test]
    fn a_backwards_clock_cannot_buy_time_back() {
        let seen = 1_760_000_000;
        let day = 24 * 60 * 60;
        assert_eq!(believable_now(0, seen), seen);
        assert_eq!(believable_now(seen, seen - 30 * day), seen);
        let jumped = seen + 400 * day;
        let after_jump = believable_now(seen, jumped);
        assert_eq!(after_jump, jumped);
        assert_eq!(believable_now(after_jump, seen + day), jumped);
    }

    /// A refusal keeps the broker's own sentence, and its code decides whether
    /// the cached receipt is dead. A bare 404 from a proxy is not a revocation,
    /// and the three-computer case carries the way to free a seat.
    #[test]
    fn broker_refusals_keep_their_own_words() {
        let not_found = service_error(
            StatusCode::NOT_FOUND,
            r#"{"ok":false,"code":"KEY_NOT_FOUND","message":"That licence key was not found."}"#,
        );
        assert!(matches!(
            not_found,
            LicenseError::Revoked(ref message) if message == "That licence key was not found."
        ));

        let inactive = service_error(
            StatusCode::FORBIDDEN,
            r#"{"ok":false,"code":"LICENSE_INACTIVE","message":"That Bard licence is not active."}"#,
        );
        assert!(matches!(inactive, LicenseError::Revoked(_)));

        let mistyped = service_error(
            StatusCode::BAD_REQUEST,
            r#"{"ok":false,"code":"INVALID_KEY","message":"Enter a valid Bard licence key."}"#,
        );
        assert!(matches!(
            mistyped,
            LicenseError::Service(ref message) if message == "Enter a valid Bard licence key."
        ));

        let seat_limit = service_error(
            StatusCode::FORBIDDEN,
            r#"{"ok":false,"code":"ACTIVATION_REJECTED","message":"That licence cannot be activated on this computer."}"#,
        );
        match seat_limit {
            LicenseError::Service(message) => {
                assert!(message.starts_with("That licence cannot be activated on this computer."));
                assert!(message.contains("Manage licence on miiso.dev"));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }

        // No body, no code: a routing failure, never a licence that ended.
        let bare = service_error(StatusCode::NOT_FOUND, "<html>Not Found</html>");
        assert!(matches!(
            bare,
            LicenseError::Service(ref message) if message == "That licence key was not found."
        ));

        // The service's message is bounded and stripped of control characters
        // before it can reach the window.
        let hostile = service_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!(
                r#"{{"ok":false,"code":"INTERNAL_ERROR","message":"{}"}}"#,
                "x".repeat(300)
            ),
        );
        assert!(matches!(
            hostile,
            LicenseError::Service(ref message) if message.len() <= 240
        ));
    }

    #[test]
    fn receipt_rejects_tampering() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"v":1,"lease_version":1,"product":"bard","key_id":"k","key_fingerprint":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","machine_id":"m","activation_id":"a","entitlement":"active","lease_sequence":1,"issued_at":1,"license_expires_at":9999999999,"valid_until":9999999999,"next_check_at":1}"#);
        assert!(decode_receipt(&payload).is_err());
    }

    #[test]
    fn release_public_key_accepts_canonical_fixture() {
        let fixture = "eyJhY3RpdmF0aW9uX2lkIjoiYSIsImVudGl0bGVtZW50IjoiYWN0aXZlIiwiaXNzdWVkX2F0IjoxLCJrZXlfZmluZ2VycHJpbnQiOiI4Y2YwM2Q5NDBhMGJlZmZhMDdkOTJkYWU4MDYzYWE3MTJjNGRhNDhjYTU3OTBiZjI4OWQxZGQ1YTY3ZjczYTU1Iiwia2V5X2lkIjoiMjAyNi0wMSIsImxlYXNlX3NlcXVlbmNlIjoxLCJsZWFzZV92ZXJzaW9uIjoxLCJsaWNlbnNlX2V4cGlyZXNfYXQiOjk5OTk5OTk5OTksIm1hY2hpbmVfaWQiOiJtIiwibmV4dF9jaGVja19hdCI6MiwicHJvZHVjdCI6ImJhcmQiLCJ2IjoxLCJ2YWxpZF91bnRpbCI6OTk5OTk5OTk5OX0.FWhnuxjS6T8vMPOgOFHGpSg2Vkgg_YiHsKVrZTL4KE5ig6y4g57nrvxUGj5zkcdkW9V11aCetlNdHHXa-VSzAg";
        let receipt =
            decode_receipt(fixture).expect("release public key must verify the canonical fixture");
        assert_eq!(receipt.key_id, "2026-01");
        assert_eq!(receipt.entitlement, "active");
    }

    #[test]
    fn a_retrying_licence_is_flagged_only_after_two_failed_cycles() {
        assert!(!revalidation_failing(0));
        assert!(!revalidation_failing(1));
        assert!(revalidation_failing(REVALIDATION_FAILURE_THRESHOLD));
        assert!(revalidation_failing(REVALIDATION_FAILURE_THRESHOLD + 3));
        // The field exists in every status shape, including the one that says
        // the app is still starting.
        assert!(!LicenseStatus::unavailable().revalidation_failing);
    }

    #[test]
    fn signed_receipt_round_trip_uses_embedded_claim_checks() {
        use ed25519_dalek::{Signer, SigningKey};
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let payload = br#"{"v":1,"lease_version":1,"product":"bard","key_id":"k","key_fingerprint":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","machine_id":"m","activation_id":"a","entitlement":"active","lease_sequence":1,"issued_at":1,"license_expires_at":9999999999,"valid_until":9999999999,"next_check_at":2}"#;
        let encoded = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(signing_key.sign(payload).to_bytes()),
        );
        let receipt = decode_receipt_with_key(&encoded, &verifying_key).unwrap();
        assert_eq!(receipt.product, "bard");
        assert_eq!(receipt.activation_id, "a");
        assert!(decode_receipt_with_key(
            &encoded,
            &SigningKey::from_bytes(&[8u8; 32]).verifying_key()
        )
        .is_err());
    }
}
