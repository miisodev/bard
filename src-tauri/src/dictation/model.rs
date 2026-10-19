use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, MutexGuard, OnceLock,
    },
    time::Duration,
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub size_mb: u64,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub architecture: crate::catalog::Architecture,
    pub role: Option<crate::catalog::ModelRole>,
    pub compatibility: crate::hf::ModelCompatibility,
    pub local_path: Option<String>,
    pub owned: bool,
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
    if let Some(def) = find(id) {
        return Some(model_path(app, &def));
    }
    crate::db::custom_model_by_id(app, id).map(|custom| PathBuf::from(custom.local_path))
}

pub fn models(app: &AppHandle) -> Vec<ModelInfo> {
    let mut list: Vec<ModelInfo> = crate::catalog::ordered()
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

    // A model installed from Hugging Face is not in the catalog, so it carries
    // no invented curation. It is listed with what Bard actually knows and its
    // verified/incompatible engine state.
    let custom_models = if crate::db::is_ready(app) {
        crate::db::list_custom_models(app)
    } else {
        Vec::new()
    };
    for custom in custom_models {
        let path = PathBuf::from(&custom.local_path);
        let measured_size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let size_bytes = if custom.size_bytes > 0 {
            custom.size_bytes
        } else {
            measured_size
        };
        list.push(ModelInfo {
            id: custom.id.clone(),
            name: custom.file.clone(),
            description: custom.repo.clone(),
            size_mb: size_bytes / 1_048_576,
            size_bytes,
            downloaded: path.is_file(),
            architecture: crate::hf::architecture_from_filename(&custom.file),
            family: "custom".into(),
            parameters: None,
            role: None,
            quant: crate::hf::quant_from_filename(&custom.file),
            languages: 0,
            capabilities: crate::catalog::Capabilities {
                streaming: false,
                translate: false,
                language_detect: false,
            },
            accuracy: 0,
            speed: 0,
            value: 0,
            recommended: false,
            recommended_rank: None,
            source: Some(crate::catalog::Source {
                repo: custom.repo.clone(),
                revision: String::new(),
                url: Some(format!("https://huggingface.co/{}", custom.repo)),
                upstream_repo: None,
                upstream_revision: None,
                upstream_url: None,
                license: None,
            }),
            host: None,
            compatibility: custom.compatibility,
            local_path: Some(custom.local_path.clone()),
            owned: custom.owned(),
            catalog: false,
        });
    }
    list
}

pub fn model_ready(app: &AppHandle) -> bool {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    resolve_path(app, &id)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

pub fn model_status(app: &AppHandle) -> ModelStatus {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    if let Some(model) = crate::catalog::find(&id) {
        let path = crate::hf::local_models_dir(app).join(&model.file.filename);
        return ModelStatus {
            id: model.id.clone(),
            name: model.name.clone(),
            size_mb: model.file.size_bytes / 1_048_576,
            size_bytes: model.file.size_bytes,
            downloaded: path.is_file(),
            architecture: model.architecture,
            role: Some(model.role),
            compatibility: crate::hf::cached_compatibility(&path),
            local_path: Some(path.to_string_lossy().to_string()),
            owned: true,
        };
    }
    if let Some(custom) = crate::db::custom_model_by_id(app, &id) {
        let path = PathBuf::from(&custom.local_path);
        let measured_size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let size_bytes = if custom.size_bytes > 0 {
            custom.size_bytes
        } else {
            measured_size
        };
        let owned = custom.owned();
        let architecture = crate::hf::architecture_from_filename(&custom.file);
        return ModelStatus {
            id: custom.id,
            name: custom.file,
            size_mb: size_bytes / 1_048_576,
            size_bytes,
            downloaded: path.is_file(),
            architecture,
            role: None,
            compatibility: custom.compatibility,
            local_path: Some(custom.local_path),
            owned,
        };
    }

    // Nothing chosen yet: report the top recommendation, so the UI has a
    // concrete thing to offer rather than an empty state.
    let model = crate::catalog::ordered()[0];
    let path = crate::hf::local_models_dir(app).join(&model.file.filename);
    ModelStatus {
        id: model.id.clone(),
        name: model.name.clone(),
        size_mb: model.file.size_bytes / 1_048_576,
        size_bytes: model.file.size_bytes,
        downloaded: path.is_file(),
        architecture: model.architecture,
        role: Some(model.role),
        compatibility: crate::hf::cached_compatibility(&path),
        local_path: Some(path.to_string_lossy().to_string()),
        owned: true,
    }
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
    invalidate_verification();

    // Installation is not selection. The UI receives a refresh signal and the
    // user activates this model explicitly with `set_model`.
    crate::hf::emit_model_installed(app, def.id);
    Ok(())
}

pub fn delete_model(app: &AppHandle, id: &str) -> Result<(), String> {
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

/// The model whose file has been fully verified in this process, if any.
///
/// Keyed by id and the resolved path together, so switching models, or the same
/// id pointing at a different file, re-verifies.
fn verified_this_process() -> &'static Mutex<Option<(String, PathBuf)>> {
    static VERIFIED: OnceLock<Mutex<Option<(String, PathBuf)>>> = OnceLock::new();
    VERIFIED.get_or_init(|| Mutex::new(None))
}

/// Run the full catalogue verification once per process for a given model file.
fn ensure_session_verified(app: &AppHandle, id: &str, path: &Path) -> Result<(), String> {
    let resolved = path.to_path_buf();
    {
        let guard = verified_this_process()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard
            .as_ref()
            .is_some_and(|(seen_id, seen_path)| seen_id == id && seen_path == &resolved)
        {
            return Ok(());
        }
    }

    crate::hf::verify_model_file(app, id, path)?;

    let mut guard = verified_this_process()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some((id.to_owned(), resolved));
    Ok(())
}

/// Forget the per-process verification, so the next capture checks again.
///
/// Called whenever the file behind the active model can have changed underneath
/// Bard — a fresh download, or a model being replaced.
pub fn invalidate_verification() {
    let mut guard = verified_this_process()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
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
/// The stamp `ensure_session_verified` sets is only set on success, exactly as
/// on the take path: a failed warm-up (no model, a missing file, a hash error)
/// changes nothing, so the take still verifies and reports in its own words.
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
    // Verified once per *session*, not once per capture.
    //
    // `verify_model_file` resolves the file and re-checks it against the
    // catalogue, and `verify_pinned_file` answers from the durable marker when
    // the file's `(size, last write, creation)` stamp and the catalogue's
    // checksum are both unchanged since the last successful hash — see that
    // function for exactly what that trusts. A stat is not free either:  this
    // runs before every take, and a take should not wait on a file read it does
    // not need. The session check is the cheap one, and the marker is what makes
    // the first check in a new process cheap as well.
    //
    // Verifying on session entry rather than per capture also keeps any change
    // to the file between sessions caught even when the stamp cannot see it:
    // a fresh process has no in-memory verdict, so it asks the marker, and the
    // marker's answer is the file's stamp against what was hashed.
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
}
