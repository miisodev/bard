use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
    time::{Duration, Instant, SystemTime},
};

use serde::Serialize;
use tauri::AppHandle;
use transcribe_cpp::{CancelToken, Model, RunOptions, Session};

/// A model as the downloader needs it. Sourced from the catalog rather than
/// declared here — see `crate::catalog`, where a model's architecture also
/// lives, because that is what decides which engine loads it.
pub struct ModelDef {
    pub id: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub architecture: crate::catalog::Architecture,
}

fn def_of(model: &'static crate::catalog::CatalogModel) -> ModelDef {
    ModelDef {
        id: &model.id,
        filename: &model.file.filename,
        url: &model.file.url,
        sha256: &model.file.sha256,
        size_bytes: model.file.size_bytes,
        architecture: model.architecture,
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_mb: u64,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub architecture: crate::catalog::Architecture,
    pub family: String,
    pub parameters: Option<String>,
    pub role: Option<crate::catalog::ModelRole>,
    pub quant: Option<String>,
    pub languages: u32,
    pub capabilities: crate::catalog::Capabilities,
    /// 0–100, relative to catalog rows rather than to a benchmark.
    pub accuracy: u32,
    pub speed: u32,
    /// 0–100, relative value at the catalog's quality/size trade-off.
    pub value: u32,
    pub recommended: bool,
    pub recommended_rank: Option<u32>,
    pub source: Option<crate::catalog::Source>,
    pub host: Option<crate::catalog::FirstPartyHost>,
    pub compatibility: crate::hf::ModelCompatibility,
    pub local_path: Option<String>,
    pub owned: bool,
    pub catalog: bool,
}

/// The resident session: the path it was built from, the session, and the token
/// that aborts an in-flight run.
type SessionSlot = (String, Session, CancelToken);

static SESSION: Mutex<Option<SessionSlot>> = Mutex::new(None);
static ACTIVE_CANCEL: Mutex<Option<CancelToken>> = Mutex::new(None);
static INIT_BACKENDS: AtomicBool = AtomicBool::new(false);
static CANCEL_DOWNLOAD: AtomicBool = AtomicBool::new(false);
static LAST_USE: AtomicU64 = AtomicU64::new(0);
static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);
/// Bumped every time the shared slot is emptied. A run that took the session out
/// of the slot reads this before and after its work: if the number moved, the
/// slot was claimed while the run was in flight and the session must be dropped
/// rather than put back.
static SLOT_GENERATION: AtomicU64 = AtomicU64::new(0);

const UNLOAD_IDLE_MS: u64 = 5 * 60 * 1000;

fn session_lock() -> MutexGuard<'static, Option<SessionSlot>> {
    SESSION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn active_cancel_lock() -> MutexGuard<'static, Option<CancelToken>> {
    ACTIVE_CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A compatibility probe whose load may outlive the caller that started it.
///
/// `hf::spawn_probe` runs `Model::load` on its own thread and abandons the wait
/// on timeout, but the thread still owns the model until the load returns. This
/// registry is the hand-over point: a probe nobody abandoned drops its model
/// exactly as before, while an abandoned one leaves it here for
/// `reap_probe_residency`.
///
/// Integration point: `hf::spawn_probe` must call `begin_probe` before the
/// load, pass the `ProbeLease` to the loading closure, and call
/// `ProbeAbandon::abandon` when `recv_timeout` expires; `engine_load` must
/// hand the loaded `Model` to `ProbeLease::finish`. Until that hook lands
/// nothing registers here, so the watcher has no probe to reap and a load that
/// never returns is still only bounded by the engine itself.
struct ProbeSlot {
    path: PathBuf,
    started: Instant,
    abandoned: AtomicBool,
    /// Set when an abandoned probe has left its model in `resident`. The lease
    /// must then outlive its own drop, because the watcher owns the model now.
    keep: AtomicBool,
    reported: AtomicBool,
    resident: Mutex<Option<Box<dyn std::any::Any + Send>>>,
}

static PROBES: Mutex<Vec<std::sync::Arc<ProbeSlot>>> = Mutex::new(Vec::new());

/// A probe older than the same idle window the transcription session uses is
/// treated as stranded: its waiter has given up and its load has not returned.
const PROBE_STRANDED_AFTER: Duration = Duration::from_millis(UNLOAD_IDLE_MS);

fn probe_lock() -> MutexGuard<'static, Vec<std::sync::Arc<ProbeSlot>>> {
    PROBES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The same poison-degrading lock the rest of this module uses, for the plain
/// payloads in a probe slot.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unregister_probe(slot: &std::sync::Arc<ProbeSlot>) {
    probe_lock().retain(|entry| !std::sync::Arc::ptr_eq(entry, slot));
}

/// The loading half of a probe.
pub struct ProbeLease {
    slot: std::sync::Arc<ProbeSlot>,
}

/// The waiting half, which is the only one that knows the caller has timed out.
pub struct ProbeAbandon {
    slot: std::sync::Arc<ProbeSlot>,
}

impl ProbeLease {
    /// Hand the loaded model over. A probe whose waiter is still waiting drops
    /// it here, exactly as `Model::load(...).map(drop)` used to; an abandoned one
    /// leaves it for the idle watcher, which is then the only party that can
    /// release it.
    pub fn finish<T: Send + 'static>(self, resident: T) {
        if self.slot.abandoned.load(Ordering::Acquire) {
            *lock(&self.slot.resident) = Some(Box::new(resident));
            self.slot.keep.store(true, Ordering::Release);
        }
    }
}

impl Drop for ProbeLease {
    fn drop(&mut self) {
        if !self.slot.keep.load(Ordering::Acquire) {
            unregister_probe(&self.slot);
        }
    }
}

impl ProbeAbandon {
    pub fn abandon(&self) {
        self.slot.abandoned.store(true, Ordering::Release);
    }
}

/// Register a probe before its load starts. `hf.rs` is the only intended caller.
pub fn begin_probe(path: PathBuf) -> (ProbeLease, ProbeAbandon) {
    let slot = std::sync::Arc::new(ProbeSlot {
        path,
        started: Instant::now(),
        abandoned: AtomicBool::new(false),
        keep: AtomicBool::new(false),
        reported: AtomicBool::new(false),
        resident: Mutex::new(None),
    });
    probe_lock().push(slot.clone());
    (ProbeLease { slot: slot.clone() }, ProbeAbandon { slot })
}

/// Drop any model an abandoned probe left behind and report probes whose load
/// has still not returned after the idle window.
///
/// A load that is genuinely wedged cannot be freed from here: the engine's
/// in-progress allocation belongs to the probe thread, not to this module. The
/// event is the honest signal that the process is still holding it; the model a
/// completed probe deposits is the part this watcher can actually reap.
fn reap_probe_residency() -> usize {
    let slots: Vec<std::sync::Arc<ProbeSlot>> = probe_lock().iter().cloned().collect();
    let mut reaped = 0;
    let mut emptied = Vec::new();
    for slot in &slots {
        if let Some(resident) = lock(&slot.resident).take() {
            drop(resident);
            reaped += 1;
            emptied.push(slot.clone());
        }
        if slot.abandoned.load(Ordering::Acquire)
            && slot.started.elapsed() >= PROBE_STRANDED_AFTER
            && !slot.reported.swap(true, Ordering::AcqRel)
        {
            log::warn!(
                "model:probe-stranded path={} age_s={}",
                slot.path.display(),
                slot.started.elapsed().as_secs()
            );
            crate::crash::record_event("model:probe-stranded");
        }
    }
    if !emptied.is_empty() {
        probe_lock().retain(|entry| {
            !emptied
                .iter()
                .any(|done| std::sync::Arc::ptr_eq(entry, done))
        });
    }
    if reaped > 0 {
        crate::crash::record_event("model:probe-reaped");
    }
    reaped
}

/// Mark the engine as used, so the idle reaper can eventually reclaim it.
fn touch_last_use() {
    LAST_USE.store(crate::db::now_ms() as u64, Ordering::SeqCst);
}

/// Empty the shared slot and claim it, so a run that is holding the session in
/// its own hands will not put it back.
///
/// Every lock in this module degrades rather than dies. A poisoned lock here
/// used to `unwrap`, and because this is called from the idle-unload watcher a
/// panic there would take the watcher thread with it: no models would ever be
/// reclaimed again, memory would climb for the life of the process, and nothing
/// would be written anywhere. The `cancel` path and the transcription path
/// already used `into_inner`; these two were the outliers.
fn release_session() {
    let held = session_lock().take().is_some();
    if held {
        // The cancel token belongs to the session we just dropped. Clearing it
        // when the slot was already empty would disarm a run that is in flight
        // with the session in its own hands.
        *active_cancel_lock() = None;
    }
    SLOT_GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Whether a finished run may return its session to the shared slot. The caller
/// also requires the slot to be empty; this is the part that can be tested.
fn reclaim_session(slot_generation: u64, run_generation: u64) -> bool {
    slot_generation == run_generation
}

fn model_path(app: &AppHandle, def: &ModelDef) -> PathBuf {
    crate::hf::local_models_dir(app).join(def.filename)
}

fn find(id: &str) -> Option<ModelDef> {
    crate::catalog::find(id).map(def_of)
}

fn resolve_path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    find(id).map(|def| model_path(app, &def))
}

pub fn models(app: &AppHandle) -> Vec<ModelInfo> {
    let list: Vec<ModelInfo> = crate::catalog::ordered()
        .into_iter()
        .map(|model| {
            let path = crate::hf::local_models_dir(app).join(&model.file.filename);
            ModelInfo {
                id: model.id.clone(),
                name: model.name.clone(),
                description: model.description.clone(),
                size_mb: model.file.size_bytes / 1_048_576,
                size_bytes: model.file.size_bytes,
                downloaded: path.is_file(),
                architecture: model.architecture,
                family: model.family.clone(),
                parameters: Some(model.parameters.clone()),
                role: Some(model.role),
                quant: model.file.quant.clone(),
                languages: model.languages,
                capabilities: model.capabilities.clone(),
                accuracy: model.accuracy,
                speed: model.speed,
                value: model.value,
                recommended: model.recommended,
                recommended_rank: model.recommended_rank,
                source: model.source.clone(),
                host: model.host.clone(),
                compatibility: crate::hf::cached_compatibility(&path),
                local_path: Some(path.to_string_lossy().to_string()),
                owned: true,
                catalog: true,
            }
        })
        .collect();

    list
}

/// Whether the active model is present and can be used.
///
/// This is deliberately a cheap check, not a hash: `get_state` asks it on
/// every state refresh, and a state read must never wait on a multi-gigabyte
/// file. It answers the question the surfaces actually ask — is there a model
/// here that a take could use — by looking for the file and, for a catalogue
/// model, checking that it is the size the catalogue promised.
///
/// The *integrity* of that file is a separate question, and it is asked where
/// it matters: `ensure_model_ready` is the gate a take passes through, so a
/// corrupt or partially downloaded model is refused before anyone records a
/// word against it. Answering "not ready" here until that check had run
/// would tell a person with a perfectly good model that it was missing.
pub fn model_ready(app: &AppHandle) -> bool {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    let Some(path) = resolve_path(app, &id) else {
        return false;
    };
    if !path.is_file() {
        return false;
    }
    if let Some(model) = crate::catalog::find(&id) {
        // A catalogue model is downloaded to an exact name; a file of the
        // wrong size is a partial download, not the model.
        return match std::fs::metadata(&path) {
            Ok(metadata) => metadata.len() == model.file.size_bytes,
            Err(_) => false,
        };
    }
    true
}

/// Verify the active model before a take may start.
///
/// `model_ready` is the cheap state query; this is the gate that used to be
/// missing. It runs the same pinned size/SHA-256 check the take paid only after
/// recording — against a corrupt or partially downloaded file that meant
/// recording a whole take and then failing with nothing to show for it. The
/// answer is cached by file identity, so only the first take after launch (or
/// after the file changes) pays for it.
pub(crate) fn ensure_model_ready(app: &AppHandle) -> Result<(), String> {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    let Some(path) = resolve_path(app, &id) else {
        return Err("No voice model has been chosen yet. Install one in Settings › Models.".into());
    };
    if !path.is_file() {
        return Err(
            "The selected voice model is not downloaded. Install it in Settings › Models.".into(),
        );
    }
    verify_model_cached(app, &id, &path)
}

pub fn cancel_transcription() {
    if let Some(token) = active_cancel_lock().as_ref() {
        token.cancel();
    }
}

pub fn cancel_download() {
    CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
}

pub fn cancel_download_reset() {
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);
}

pub fn cancel_pending() -> bool {
    CANCEL_DOWNLOAD.load(Ordering::SeqCst)
}

pub async fn download_model(app: &AppHandle, id: &str) -> Result<(), String> {
    let Some(def) = find(id) else {
        return Err(format!("Unknown model: {id}"));
    };
    if !def.architecture.is_single_file() {
        return Err("This catalog model is not a single-file engine model.".into());
    }
    let path = model_path(app, &def);

    if path.is_file() {
        let pinned = crate::hf::verify_pinned_file(&path, def.size_bytes, def.sha256);
        if pinned.is_ok() && crate::hf::verify_load(&path.to_string_lossy()) {
            // The file the person already had is the file just checked; do not
            // make the first take hash it again.
            remember_verified(def.id, &path);
            crate::hf::emit_model_installed(app, def.id);
            return Ok(());
        }
        // The path is derived from Bard's models directory and filename, not
        // from user input. A file that fails the pinned integrity check or the
        // engine check is replaced rather than reported as ready.
        let _ = fs::remove_file(&path);
    }

    cancel_download_reset();
    let request = crate::hf::DownloadRequest {
        id: def.id.to_string(),
        url: def.url.to_string(),
        sha256: def.sha256.to_string(),
        size_bytes: def.size_bytes,
        destination: path.clone(),
    };
    crate::hf::download_resumable(app, request).await?;

    crate::hf::verify_pinned_file(&path, def.size_bytes, def.sha256)?;
    if !crate::hf::verify_load(&path.to_string_lossy()) {
        let _ = fs::remove_file(&path);
        let error = "The downloaded catalog model could not be loaded by Bard's engine.";
        crate::hf::emit_download_failure(app, def.id, error);
        return Err(error.into());
    }
    // The file on disk has just changed, so anything previously verified for this
    // model is no longer a statement about these bytes.
    invalidate_verification_for(&path);
    // This download was just checked against the catalogue checksum and opened
    // by the engine, so the same bytes do not need hashing again on the first
    // take.
    remember_verified(def.id, &path);

    // Installation is not selection. The UI receives a refresh signal and the
    // user activates this model explicitly with `set_model`.
    crate::hf::emit_model_installed(app, def.id);
    Ok(())
}

pub fn delete_model(app: &AppHandle, id: &str) -> Result<(), String> {
    // The verdict describes a file that is about to disappear. Forgetting it
    // here means a later file that happens to reuse the path and its size and
    // timestamp cannot inherit the deleted model's answer.
    if let Some(path) = resolve_path(app, id) {
        invalidate_verification_for(&path);
    }
    crate::hf::delete_model(app, id)?;
    // A non-active deletion cannot be the in-memory session, but clearing an
    // idle/stale session here preserves the old unload behavior for callers.
    // The slot is taken for an instant: a transcription in flight holds the
    // session in its own thread, so this no longer waits out the whole take, and
    // the generation bump keeps that run from putting the deleted model back.
    release_session();
    Ok(())
}

pub fn start_unload_watcher(_app: AppHandle) {
    if WATCHER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        reap_probe_residency();
        let last = LAST_USE.load(Ordering::SeqCst);
        if last > 0 && crate::db::now_ms() - last as i64 > UNLOAD_IDLE_MS as i64 {
            release_session();
            LAST_USE.store(0, Ordering::SeqCst);
        }
    });
}

pub fn transcribe(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    crate::crash::record_event("model:transcribe-start");
    let result = transcribe_inner(app, samples);
    crate::crash::record_event("model:transcribe-end");
    result
}

/// The identity of the exact model file a verification answer describes.
///
/// The path alone is not enough: a replacement at the same path, a fresh
/// download or a partially written file must not inherit the previous answer.
/// Size and modification time are the cheap pair that changes with the bytes,
/// and caching on them is what lets a state read ask for the answer without
/// touching the file's contents.
#[derive(Clone, Debug, PartialEq, Eq)]
struct VerificationKey {
    id: String,
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}

struct VerificationEntry {
    key: VerificationKey,
    outcome: Result<(), String>,
}

fn verification_cache() -> &'static Mutex<Vec<VerificationEntry>> {
    static CACHE: OnceLock<Mutex<Vec<VerificationEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Serializes the expensive check, not the cheap cache read. A warm-up and a
/// take arriving together must not each hash a multi-gigabyte file; the second
/// one waits here and then finds the first one's answer in the cache.
fn verification_in_progress() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn verification_key(id: &str, path: &Path) -> Result<VerificationKey, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect the voice model: {error}"))?;
    if !metadata.is_file() {
        return Err("The selected model path is not a regular file.".into());
    }
    Ok(VerificationKey {
        id: id.to_owned(),
        path: path.to_path_buf(),
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// The cached answer for `key`, if it describes the same file.
fn cached_model_verification(key: &VerificationKey) -> Option<Result<(), String>> {
    let cache = verification_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .iter()
        .find(|entry| entry.key == *key)
        .map(|entry| entry.outcome.clone())
}

fn remember_verification(key: VerificationKey, outcome: Result<(), String>) {
    let mut cache = verification_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.retain(|entry| entry.key.path != key.path);
    cache.push(VerificationEntry { key, outcome });
}

/// Run the pinned size/SHA-256 verification once per model file.
fn verify_model_cached(app: &AppHandle, id: &str, path: &Path) -> Result<(), String> {
    let key = verification_key(id, path)?;
    if let Some(outcome) = cached_model_verification(&key) {
        return outcome;
    }
    let _hashing = verification_in_progress()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // The warm-up may have finished while this caller waited for the hasher.
    if let Some(outcome) = cached_model_verification(&key) {
        return outcome;
    }
    let outcome = crate::hf::verify_model_file(app, id, path);
    remember_verification(key, outcome.clone());
    outcome
}

/// Record a file that was just verified by the download path, so the first take
/// after an install does not hash the same bytes again.
fn remember_verified(id: &str, path: &Path) {
    let Ok(key) = verification_key(id, path) else {
        return;
    };
    remember_verification(key, Ok(()));
}

/// Verify the file a take is about to load, reusing the cached answer only
/// while the file's size and modification time still describe the same bytes.
fn ensure_session_verified(app: &AppHandle, id: &str, path: &Path) -> Result<(), String> {
    verify_model_cached(app, id, path)
}

/// Forget the cached answer for one file, leaving the other models alone.
fn invalidate_verification_for(path: &Path) {
    verification_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|entry| entry.key.path.as_path() != path);
}

/// Set once the verification warm-up has claimed its thread for this process.
static WARMUP_STARTED: AtomicBool = AtomicBool::new(false);

/// `true` on the first call for a given flag, `false` after.
///
/// A `swap` rather than a load-then-store: two callers arriving together must
/// produce exactly one warm-up, not two threads hashing the same gigabytes.
fn claim_warmup(started: &AtomicBool) -> bool {
    !started.swap(true, Ordering::SeqCst)
}

/// Verify the active model's bytes once, on a background thread, shortly after
/// launch.
///
/// The first take after launch otherwise pays a full SHA-256 of the model file
/// — up to 1.69 GB — inside `ensure_session_verified` before the engine can
/// even be asked to load it, and the take's watchdog has to cover it. The hash
/// is the same work whenever it happens, so it may as well happen while the
/// person is still opening the panel.
///
/// The answer is cached either way, exactly as on the take path: the warm-up
/// exists to pay the hash once, and a failure it finds is the same failure a
/// take would report. The take gate asks again only when the file's identity
/// changes.
pub fn spawn_verification_warmup(app: AppHandle) {
    if !claim_warmup(&WARMUP_STARTED) {
        return;
    }
    std::thread::spawn(move || {
        let id = crate::db::get_setting(&app, "model_id").unwrap_or_default();
        if id.is_empty() {
            return;
        }
        let Some(path) = resolve_path(&app, &id) else {
            return;
        };
        if !path.is_file() {
            return;
        }
        match ensure_session_verified(&app, &id, &path) {
            Ok(()) => crate::crash::record_event("model:verification-warmup"),
            Err(error) => {
                // Not surfaced to anyone: the take that needs this model will
                // verify again and report the same failure where it belongs.
                log::warn!("model verification warm-up failed for {id}: {error}");
                crate::crash::record_event("model:verification-warmup-failed");
            }
        }
    });
}

fn transcribe_inner(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    let Some(path) = resolve_path(app, &id) else {
        return Err("No voice model downloaded.".into());
    };
    if !path.is_file() {
        return Err("Voice model not downloaded.".into());
    }
    // The take gate (`ensure_model_ready`) has already run this check before
    // the microphone opened; keeping it here covers a direct `transcribe`
    // caller and any file change in the gap. It is a cache read while the
    // file's size and modification time still describe the bytes that were
    // hashed, so a take does not wait on a file read it does not need; a miss
    // falls through to `verify_model_file` and its durable marker.
    ensure_session_verified(app, &id, &path)?;

    if !INIT_BACKENDS.load(Ordering::SeqCst) {
        // The flag used to be set with `swap` before the call and the result
        // discarded, so one failure — a busy GPU, a driver that was not ready
        // yet — disabled dictation for the rest of the process with nothing
        // anywhere saying why. The flag now records success, and a failure is
        // reported instead of swallowed.
        transcribe_cpp::init_backends_default()
            .map_err(|error| format!("Bard's transcription engine could not start: {error}"))?;
        INIT_BACKENDS.store(true, Ordering::SeqCst);
    }

    let saved_path = path.to_string_lossy().to_string();
    // The session is *taken out* of the shared slot for the duration of the run.
    // Holding the lock across `run` is what made this module's two other
    // lock-takers — the idle reaper and `delete_model` — block for the whole
    // transcription. Nothing else can start a run while this one is in flight,
    // because the state machine is not Idle until the run returns.
    let (session_path, mut session, token, generation) = {
        let mut guard = session_lock();
        if guard
            .as_ref()
            .map(|(saved, _, _)| saved != &saved_path)
            .unwrap_or(true)
        {
            let model = Model::load(&path).map_err(|e| e.to_string())?;
            let mut session = model.session().map_err(|e| e.to_string())?;
            let token = CancelToken::new();
            session.set_cancel_token(&token);
            *guard = Some((saved_path.clone(), session, token));
        }
        let Some((_, session, token)) = guard.take() else {
            return Err("Voice model session could not be created.".into());
        };
        // Read inside the same critical section as the take, so a release that
        // raced us is always visible afterwards as a different generation.
        (
            saved_path.clone(),
            session,
            token,
            SLOT_GENERATION.load(Ordering::SeqCst),
        )
    };

    token.reset();
    *active_cancel_lock() = Some(token.clone());
    let result = session
        .run(samples, &RunOptions::default())
        .map(|transcript| transcript.text.trim().to_string())
        .map_err(|e| e.to_string());
    *active_cancel_lock() = None;

    // The reaper counts from the last *attempt*, not the last success. A failed
    // run still leaves a 1.5 GB model resident, and a stamp written only on
    // success left `LAST_USE` at zero, which the reaper treats as "never used" —
    // so that model stayed loaded for the life of the process and no amount of
    // idling would reclaim it.
    touch_last_use();

    let mut guard = session_lock();
    if reclaim_session(SLOT_GENERATION.load(Ordering::SeqCst), generation) && guard.is_none() {
        *guard = Some((session_path, session, token));
    }
    // Otherwise a delete or the reaper claimed the slot while this run was in
    // flight, and the session is dropped here instead of being put back: the
    // model has been removed, or is due to be, and putting it back would pin
    // its weights with nothing left that refers to them.
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_keeps_the_session_only_if_the_slot_was_not_claimed() {
        assert!(reclaim_session(7, 7));
        // A delete or the reaper ran while the transcription was in flight.
        assert!(!reclaim_session(8, 7));
    }

    #[test]
    fn claiming_the_slot_is_always_visible_to_a_run() {
        let before = SLOT_GENERATION.load(Ordering::SeqCst);
        release_session();
        let after = SLOT_GENERATION.load(Ordering::SeqCst);
        assert_ne!(before, after, "release_session must bump the generation");
    }

    #[test]
    fn the_reaper_window_is_five_minutes() {
        assert_eq!(UNLOAD_IDLE_MS, 5 * 60 * 1000);
    }

    #[test]
    fn the_verification_warmup_claims_only_one_thread() {
        let started = AtomicBool::new(false);
        assert!(claim_warmup(&started), "the first caller starts it");
        assert!(!claim_warmup(&started), "a second caller starts nothing");
        assert!(!claim_warmup(&started));
    }

    #[test]
    fn releasing_the_slot_takes_no_lock_a_run_can_be_holding() {
        // A transcription holds the session in its own thread for the whole run,
        // so this is the shape the reaper and `delete_model` depend on. If
        // someone reintroduces a lock held across `session.run`, this is the
        // call that starts blocking for minutes.
        let started = std::time::Instant::now();
        release_session();
        release_session();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(50),
            "release_session blocked for {:?}",
            started.elapsed()
        );
    }

    /// A unique path for a stand-in model file. The real models are gigabytes
    /// and live in the user's data directory; these checks only need the
    /// metadata they are actually about.
    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "bard-dictation-model-{}-{tag}.bin",
            std::process::id()
        ))
    }

    fn probe_registered(path: &Path) -> bool {
        probe_lock().iter().any(|slot| slot.path == path)
    }

    struct DropFlag(std::sync::Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn a_verification_answer_is_bound_to_the_exact_file_it_describes() {
        let path = temp_path("verification-key");
        let other = temp_path("verification-key-other");
        fs::write(&path, b"first").expect("write the model stand-in");
        fs::write(&other, b"first").expect("write the second stand-in");

        let first = verification_key("model-a", &path).expect("a regular file");
        assert_eq!(
            first,
            verification_key("model-a", &path).expect("the same file"),
            "an unchanged file must compare equal"
        );
        assert!(
            first != verification_key("model-b", &path).expect("another model id"),
            "the same bytes under another id must not inherit the answer"
        );
        assert!(
            first != verification_key("model-a", &other).expect("another path"),
            "the same bytes at another path must not inherit the answer"
        );

        // Both halves of the cheap identity: a longer file and a touched file
        // must each produce a key that cannot match the previous answer.
        fs::write(&path, b"second").expect("replace the contents");
        let resized = verification_key("model-a", &path).expect("after the write");
        assert!(
            first != resized,
            "a size change must invalidate the cached answer"
        );

        let touched = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        fs::File::options()
            .write(true)
            .open(&path)
            .expect("open for the timestamp change")
            .set_modified(touched)
            .expect("set the modification time");
        let retimed = verification_key("model-a", &path).expect("after the touch");
        assert!(
            resized != retimed,
            "a modification-time change must invalidate the cached answer"
        );

        assert!(
            verification_key("model-a", &temp_path("verification-missing")).is_err(),
            "a path that does not exist has no identity to cache"
        );

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&other);
    }

    #[test]
    fn a_cached_verification_is_returned_only_for_the_file_it_describes() {
        let path = temp_path("verification-cache");
        fs::write(&path, b"cached").expect("write the model stand-in");
        let key = verification_key("cache-test", &path).expect("a regular file");

        assert!(
            cached_model_verification(&key).is_none(),
            "nothing cached yet"
        );
        remember_verification(key.clone(), Err("bad bytes".into()));
        assert_eq!(
            cached_model_verification(&key),
            Some(Err("bad bytes".into())),
            "a failure is cached as deliberately as a success: a corrupt file \
             must not be hashed again on every state read and every attempt"
        );

        // The same path with different bytes is a different question.
        fs::write(&path, b"cached and longer").expect("replace the contents");
        let replaced = verification_key("cache-test", &path).expect("after the write");
        assert!(
            cached_model_verification(&replaced).is_none(),
            "a changed file must not reuse the previous verdict"
        );

        invalidate_verification_for(&path);
        assert!(
            cached_model_verification(&key).is_none(),
            "invalidation must forget the file it was given"
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_probe_that_is_not_abandoned_drops_its_model_immediately() {
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let path = temp_path("probe-normal");
        let (lease, abandon) = begin_probe(path.clone());
        assert!(
            probe_registered(&path),
            "the lease registers before loading"
        );

        // The waiter is still there, so the loaded model is the waiter's to
        // drop; nothing may be left for the reaper.
        lease.finish(DropFlag(dropped.clone()));
        assert!(
            dropped.load(Ordering::SeqCst),
            "an answered probe must release its model at once"
        );
        assert!(
            !probe_registered(&path),
            "a finished probe must leave no slot behind"
        );
        drop(abandon);
    }

    #[test]
    fn an_abandoned_probe_leaves_its_model_for_the_reaper_to_release() {
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let path = temp_path("probe-abandoned");
        let (lease, abandon) = begin_probe(path.clone());

        // This is the timeout: the waiter has stopped waiting, but the load is
        // still going to return into a thread nobody is reading.
        abandon.abandon();
        lease.finish(DropFlag(dropped.clone()));
        assert!(
            !dropped.load(Ordering::SeqCst),
            "an abandoned probe's model must outlive the waiter that gave up"
        );
        assert!(probe_registered(&path), "the reaper needs to find it");

        assert!(reap_probe_residency() >= 1, "the resident model is reaped");
        assert!(
            dropped.load(Ordering::SeqCst),
            "reaping must actually drop the model"
        );
        assert!(
            !probe_registered(&path),
            "a reaped probe must not stay registered"
        );
        drop(abandon);
    }
}
