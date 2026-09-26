//! Bard's local licence gate.
//!
//! The product is usable without an account for a bounded trial. A paid
//! licence is a Polar licence key activated through the fixed miiso.dev
//! licence service. The key itself lives in Windows Credential Manager; the
//! service returns a short, signed receipt that can be checked locally while
//! offline. The receipt is deliberately small and contains no customer data,
//! transcript data, or provider secret.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

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
const DEFAULT_LICENSE_SERVER: &str = "https://miiso.dev/api/bard/license";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const KEYRING_TIMEOUT: Duration = Duration::from_millis(400);
const REVALIDATION_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// How long a signed offline lease stays usable past the point it asked to be
/// re-checked. Three days covers a laptop that is genuinely offline for a
/// weekend without punishing someone whose connection is unreliable, and it
/// replaces an unbounded month-long window in which a revoked licence could not
/// be enforced at all.
const OFFLINE_REVALIDATION_GRACE_SECONDS: i64 = 3 * 24 * 60 * 60;

/// How many 15-minute intervals make one heartbeat — four hours.
const REVALIDATION_HEARTBEAT_TICKS: u64 = 16;
static REFRESH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

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
}

impl LicenseStatus {
    fn unavailable() -> Self {
        Self {
            state: "unavailable".into(),
            can_use: false,
            key_configured: false,
            trial_started_at: None,
            trial_expires_at: None,
            expires_at: None,
            last_checked_at: None,
            message: Some("Bard is still starting.".into()),
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
    Revoked,
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
            Self::Revoked => f.write_str("This Bard licence is no longer active."),
            Self::Service(message) => f.write_str(message),
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
}

#[derive(Serialize)]
struct ActivateRequest<'a> {
    key: &'a str,
    machine_id: &'a str,
    app_version: &'a str,
}

#[derive(Serialize)]
struct ValidateRequest<'a> {
    key: &'a str,
    machine_id: &'a str,
    activation_id: Option<&'a str>,
    app_version: &'a str,
}

#[derive(Serialize)]
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

fn new_machine_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn is_machine_id(candidate: &str) -> bool {
    candidate.len() == 32 && candidate.chars().all(|character| character.is_ascii_hexdigit())
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
/// So an unavailable store is a hard error, and only a confirmed `NoEntry` is
/// permission to create.
fn machine_id(app: &tauri::AppHandle) -> Result<String, LicenseError> {
    match read_credential_bounded(INSTALLATION_ID_USERNAME) {
        CredentialRead::Value(existing) if is_machine_id(&existing) => return Ok(existing),
        CredentialRead::Unavailable => {
            crate::crash::record_event("licence:machine-id-unavailable");
            return Err(LicenseError::CredentialStore);
        }
        // A stored value that is not a well-formed ID is corrupt rather than
        // absent, and replacing it would be the same mistake as a timeout. The
        // legacy migration below is the one path allowed to write here.
        CredentialRead::Value(_) | CredentialRead::Absent => {}
    }

    // Migrate the old opaque value once so existing paid activations remain
    // valid, then remove it from SQLite. New installations never derive an ID
    // from a computer name, username, or hardware string.
    if let Some(existing) = crate::db::get_setting(app, MACHINE_ID_SETTING) {
        if is_machine_id(&existing) {
            write_credential_bounded(INSTALLATION_ID_USERNAME, existing.clone())?;
            let _ = crate::db::set_setting_checked(app, MACHINE_ID_SETTING, "");
            return Ok(existing);
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
    Ok(status(app))
}

fn status_from_receipt(
    app: &tauri::AppHandle,
    receipt: &Receipt,
    key_configured: bool,
    message: Option<String>,
) -> LicenseStatus {
    let (trial_started_at, trial_expires_at) =
        existing_trial_window(app).unwrap_or_else(|| (now_seconds(), now_seconds()));
    let now = now_seconds();
    let temporal_bounds_valid = receipt.issued_at <= now + 300
        && receipt.issued_at >= now.saturating_sub(30 * 24 * 60 * 60 + 300)
        && receipt.valid_until >= receipt.issued_at
        && receipt.valid_until <= receipt.issued_at.saturating_add(30 * 24 * 60 * 60)
        && receipt.next_check_at >= receipt.issued_at
        && receipt.next_check_at <= receipt.issued_at.saturating_add(24 * 60 * 60 + 300)
        && receipt.license_expires_at >= receipt.issued_at
        && receipt.valid_until <= receipt.license_expires_at;
    let machine_matches = machine_id(app)
        .map(|machine| receipt.machine_id == machine)
        .unwrap_or(false);
    // The receipt asks the client to re-check within 24 hours. Previously nothing
    // enforced that, so a licence revoked or ended on the server stayed usable
    // for the whole 30-day lease as long as the machine stayed offline — bounded,
    // but a month rather than days. A short grace keeps genuine offline use
    // (a laptop on a train, a weekend away) working while making the offline
    // window days instead of a month.
    let revalidation_current =
        receipt.next_check_at > now.saturating_sub(OFFLINE_REVALIDATION_GRACE_SECONDS);
    let valid = temporal_bounds_valid
        && receipt.entitlement == "active"
        && receipt.valid_until > now
        && receipt.license_expires_at > now
        && machine_matches
        && revalidation_current;
    let default_message = match receipt.entitlement.as_str() {
        "suspended" => Some("This Bard licence is temporarily suspended.".into()),
        "ended" => Some("This Bard licence has ended. Buy another licence to keep creating.".into()),
        _ => Some("The Bard licence has expired or is not valid on this computer.".into()),
    };
    LicenseStatus {
        state: if valid { "active" } else { "restricted" }.into(),
        can_use: valid,        key_configured,
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

pub fn status(app: &tauri::AppHandle) -> LicenseStatus {
    if !crate::db::is_ready(app) {
        return LicenseStatus::unavailable();
    }
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
        };
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
            };
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
        };
    };
    let now = now_seconds();
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
        };
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
    }
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

fn service_message(status: StatusCode, body: &str) -> String {
    let parsed = serde_json::from_str::<ServiceResponse>(body).ok();
    let message = parsed
        .and_then(|response| response.message)
        .unwrap_or_else(|| match status {
            StatusCode::UNAUTHORIZED => "The licence key was not accepted.".into(),
            StatusCode::TOO_MANY_REQUESTS => {
                "The licence service is busy. Try again shortly.".into()
            }
            StatusCode::PAYMENT_REQUIRED => {
                "The licence service could not confirm this subscription.".into()
            }
            _ => "The licence service could not complete the request.".into(),
        });
    let mut output = String::new();
    for character in message.chars().take(240) {
        if character.is_control() {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    if output.is_empty() {
        "The licence service could not complete the request.".into()
    } else {
        output
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
        if matches!(
            status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ) {
            return Err(LicenseError::Revoked);
        }
        return Err(LicenseError::Service(service_message(status, &text)));
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
        Ok(response) => match store_response_receipt(app, response, true, &key) {
            Ok(status) => status,
            Err(_) => status(app),
        },
        Err(LicenseError::Revoked) => {
            let _ = delete_credential_bounded(RECEIPT_USERNAME);
            let _ = crate::db::set_setting_checked(app, RECEIPT_SETTING, "");
            status(app)
        }
        Err(_) => status(app),
    };
    key.zeroize();
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
        loop {
            std::thread::sleep(REVALIDATION_INTERVAL);
            ticks += 1;
            // Every four hours, and only while a licence is actually in play.
            // An idle install writes nothing.
            if ticks % REVALIDATION_HEARTBEAT_TICKS == 0 {
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
                let due = cached_receipt(&app)
                    .map(|receipt| receipt.next_check_at <= now_seconds())
                    .unwrap_or(false);
                if due {
                    let _ = tauri::async_runtime::block_on(refresh(&app));
                }
            }));
            if stepped.is_err() {
                crate::crash::record_event("licence:revalidation-iteration-failed");
            }
        }
    });
}

pub async fn deactivate(app: &tauri::AppHandle) -> Result<LicenseStatus, LicenseError> {
    let Some(mut key) = read_key_bounded() else {
        return Ok(status(app));
    };
    let Some(activation_id) = cached_receipt(app).map(|receipt| receipt.activation_id) else {
        key.zeroize();
        return Err(LicenseError::InvalidReceipt);
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
    crate::db::set_settings_checked(app, &[(RECEIPT_SETTING, ""), (LAST_CHECK_SETTING, "")])
        .map_err(|_| LicenseError::Service("Bard could not clear its licence state.".into()))?;
    delete_credential_bounded(RECEIPT_USERNAME)?;
    delete_key_bounded()?;
    Ok(status(app))
}

pub fn erase_local(app: &tauri::AppHandle) -> Result<(), LicenseError> {
    delete_credential_bounded(LICENSE_USERNAME)?;
    delete_credential_bounded(RECEIPT_USERNAME)?;
    delete_credential_bounded(INSTALLATION_ID_USERNAME)?;
    delete_credential_bounded(TRIAL_SECRET_USERNAME)?;
    delete_credential_bounded(TRIAL_STATE_USERNAME)?;
    let _ = app;
    Ok(())
}

pub fn clear(app: &tauri::AppHandle) -> Result<LicenseStatus, LicenseError> {
    crate::db::set_settings_checked(app, &[(RECEIPT_SETTING, ""), (LAST_CHECK_SETTING, "")])
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
