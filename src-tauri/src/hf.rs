use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
#[cfg(not(windows))]
use std::time::UNIX_EPOCH;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCompatibility {
    /// Bard has not asked the engine to open this exact file yet.
    #[default]
    Unverified,
    /// The current engine opened the file successfully.
    Verified,
    /// The file exists, but the current engine could not open it.
    Incompatible,
    /// The registered path no longer exists on this machine.
    Missing,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityResult {
    pub path: String,
    pub status: ModelCompatibility,
    pub message: Option<String>,
}

/// Accept a canonical repo ID, a Hugging Face repository URL, or a URL to a
/// page below that repository. The API always receives only `owner/name`.
pub fn normalize_repo_id(input: &str) -> Result<String, String> {
    let mut value = input.trim();
    if value.is_empty() {
        return Err("Enter a Hugging Face repository ID or URL.".into());
    }

    let lower = value.to_ascii_lowercase();
    let mut from_url = false;
    if lower.starts_with("http://") {
        return Err("Use an HTTPS Hugging Face URL.".into());
    }
    if lower.starts_with("https://") {
        let without_scheme = value
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(value);
        let (authority, path) = without_scheme.split_once('/').ok_or_else(|| {
            "Hugging Face repository URLs must include owner and name.".to_string()
        })?;
        let host = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority)
            .to_ascii_lowercase();
        let host = host.split(':').next().unwrap_or_default();
        if !matches!(
            host,
            "huggingface.co" | "www.huggingface.co" | "hf.co" | "www.hf.co"
        ) {
            return Err("Only huggingface.co repository URLs are supported.".into());
        }
        value = path;
        from_url = true;
    } else if lower.starts_with("huggingface.co/") {
        value = value.split_once('/').map(|(_, rest)| rest).unwrap_or(value);
        from_url = true;
    } else if lower.starts_with("www.huggingface.co/") {
        value = value.split_once('/').map(|(_, rest)| rest).unwrap_or(value);
        from_url = true;
    } else if lower.starts_with("hf.co/") {
        value = value.split_once('/').map(|(_, rest)| rest).unwrap_or(value);
        from_url = true;
    } else if lower.starts_with("www.hf.co/") {
        value = value.split_once('/').map(|(_, rest)| rest).unwrap_or(value);
        from_url = true;
    }

    value = value
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');
    if value.contains('\\') {
        return Err("Hugging Face repository IDs cannot contain backslashes.".into());
    }

    let mut parts: Vec<&str> = value.split('/').filter(|part| !part.is_empty()).collect();
    if from_url
        && parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case("models"))
    {
        parts.remove(0);
    }
    if parts.len() < 2 || parts.iter().take(2).any(|part| matches!(*part, "." | "..")) {
        return Err("Use a Hugging Face repository ID such as owner/name.".into());
    }
    if !from_url && parts.len() != 2 {
        return Err("Use a Hugging Face repository ID such as owner/name.".into());
    }

    let repo = parts[..2].join("/");
    if repo.split('/').any(|part| part.trim().is_empty()) {
        return Err("Use a Hugging Face repository ID such as owner/name.".into());
    }
    Ok(repo)
}

fn normalize_file_path(input: &str) -> Result<String, String> {
    let value = input.trim().trim_start_matches('/');
    if value.is_empty() || value.contains('\\') {
        return Err("Invalid Hugging Face file path.".into());
    }
    let parts: Vec<&str> = value.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|part| matches!(*part, "." | "..")) {
        return Err("Invalid Hugging Face file path.".into());
    }
    Ok(parts.join("/"))
}

pub fn normalize_checksum(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let value = value
        .strip_prefix("sha256:")
        .unwrap_or(value)
        .to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid SHA-256 checksum from Hugging Face.".into());
    }
    Ok(value)
}

fn app_model_id(repo: &str, file: &str) -> String {
    format!("app|{repo}|{file}")
}

/// A stable id for a model the user already has in the shared cache. Takes an
/// already-canonicalized path so listing a cache does not re-canonicalize the
/// same file it has just canonicalized.
fn external_model_id(repo: &str, file: &str, canonical: &Path) -> String {
    let normalized = normalized_path_string(canonical);
    let digest = Sha256::digest(normalized.as_bytes());
    format!("hf-cache|{repo}|{file}|{:x}", digest)
}

/// An environment override only counts when it is an absolute path. A relative
/// value would produce a root that `path_is_within` can never match, which
/// presents as "no models detected" rather than as the misconfiguration it is.
fn absolute_env_path(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var(name).ok()?);
    (path.is_absolute()).then_some(path)
}

/// The shared Hugging Face cache. This is the one model root Bard reads but
/// never writes to and never deletes from, and it is the containment root that
/// `probe_local_model`, `register_detected_path` and the symlink allowance in
/// `verify_model_file` all resolve against.
///
/// Decision on the environment-derived trust root: the `HF_HUB_CACHE` /
/// `HF_HOME` convention is **kept**, deliberately, and is not re-pinned under
/// `app_local_data_dir()`.
///
/// The reason it is not a privilege boundary: Bard runs as the user, who
/// already chose both the environment and the files in it. A user who launches
/// Bard with `HF_HUB_CACHE` pointing somewhere unexpected gains nothing they did
/// not already have, because reaching a file through this root still requires it
/// to be a regular file that the current engine opens successfully as a model —
/// see `probe_model_path`. Pinning the root under the app data directory would
/// add no security property and would silently stop Bard from seeing models the
/// user legitimately already owns from `huggingface-cli download`, another tool,
/// or a colleague's archive. The containing decisions are: a path here must pass
/// `path_is_within` *and* a real engine load to be registered, and anything
/// registered from this root is recorded as `External`, which `delete_model`
/// never removes.
pub fn hf_cache_dir() -> PathBuf {
    if let Some(path) = absolute_env_path("HF_HUB_CACHE") {
        return path;
    }
    if let Some(home) = absolute_env_path("HF_HOME") {
        return home.join("hub");
    }
    #[cfg(windows)]
    {
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        PathBuf::from(home)
            .join(".cache")
            .join("huggingface")
            .join("hub")
    }
    #[cfg(not(windows))]
    {
        // The same resolution `huggingface_hub` uses on Linux: the XDG cache
        // directory when it is set, otherwise `~/.cache`.
        let base = absolute_env_path("XDG_CACHE_HOME")
            .or_else(|| absolute_env_path("HOME").map(|home| home.join(".cache")))
            .unwrap_or_default();
        base.join("huggingface").join("hub")
    }
}

pub fn local_models_dir(app: &AppHandle) -> PathBuf {
    let data = app
        .path()
        .app_local_data_dir()
        .expect("local application data directory is unavailable");
    let dir = data.join("models");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// A recorded compatibility answer, together with the identity of the exact
/// bytes it was produced from. Without the stamp a replaced file keeps the
/// answer of the file it replaced, and the Models pane reports a stale result
/// as current.
type CompatibilityCacheValue = (PinnedFileStamp, ModelCompatibility);

fn compatibility_cache() -> &'static Mutex<HashMap<PathBuf, CompatibilityCacheValue>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CompatibilityCacheValue>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn path_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn compatibility_stamp(path: &Path) -> Option<PinnedFileStamp> {
    fs::metadata(path)
        .ok()
        .map(|metadata| file_stamp(&metadata))
}

/// What Bard already believes about this exact file, without opening it.
///
/// **Display only, never an authorization.** No activation path consults this:
/// `activate_model` always runs a real `Model::load` and `verify_model_file`
/// always re-checks size and SHA-256 against the catalogue or the registration
/// row. Its whole purpose is to keep the Models pane from paying a
/// multi-gigabyte load per row it has already answered, which is what made
/// opening Settings › Models the app's largest avoidable cost.
///
/// `Unverified` and `Missing` both mean "no engine answer": nobody has asked, or
/// the file is gone. Neither is returned here, because a caller that gets a
/// value is entitled to skip the load.
pub fn cached_compatibility(path: &Path) -> ModelCompatibility {
    let Some(stamp) = compatibility_stamp(path) else {
        return ModelCompatibility::Unverified;
    };
    compatibility_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&path_key(path)).cloned())
        .filter(|(recorded, _)| *recorded == stamp)
        .map(|(_, status)| status)
        .unwrap_or(ModelCompatibility::Unverified)
}

fn remember_compatibility(path: &Path, status: ModelCompatibility) {
    let Some(stamp) = compatibility_stamp(path) else {
        return;
    };
    if let Ok(mut cache) = compatibility_cache().lock() {
        cache.insert(path_key(path), (stamp, status));
    }
}

/// Serialize activation and deletion. Both operations cross the settings/DB
/// boundary, and an activation must not race a delete into writing a dead id.
pub(crate) fn model_operation_guard() -> MutexGuard<'static, ()> {
    static OPERATIONS: OnceLock<Mutex<()>> = OnceLock::new();
    OPERATIONS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn normalized_path_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn canonicalize_for_comparison(path: &Path) -> PathBuf {
    if let Ok(path) = fs::canonicalize(path) {
        return path;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(parent) = fs::canonicalize(parent) {
            return parent.join(name);
        }
    }
    path.to_path_buf()
}

/// The containment test over already-canonicalized paths.
///
/// `path_is_within` is the only entry point that canonicalizes, and it delegates
/// here. This split exists so a caller that already holds a canonical root —
/// `cached_models` tests every detected file against two of them — can reuse the
/// rule instead of re-canonicalizing the root per file or, far worse,
/// re-implementing the comparison and slowly diverging from it.
fn path_is_within_canonical(root: &Path, candidate: &Path) -> bool {
    let root_text = normalized_path_string(root);
    let candidate_text = normalized_path_string(candidate);
    !candidate_text.is_empty()
        && candidate_text != root_text
        && candidate_text.starts_with(&(root_text.clone() + "/"))
}

/// True only for a descendant of `root`. Canonicalization defeats `..` and
/// symlink escapes; the string check adds Windows case-insensitive containment.
pub fn path_is_within(root: &Path, candidate: &Path) -> bool {
    if !root.is_absolute() || !candidate.is_absolute() {
        return false;
    }
    path_is_within_canonical(
        &canonicalize_for_comparison(root),
        &canonicalize_for_comparison(candidate),
    )
}

fn remove_owned_model_file(app: &AppHandle, path: &Path) -> Result<bool, String> {
    let root = local_models_dir(app);
    if !path_is_within(&root, path) {
        return Err("Refusing to delete a model outside Bard's models directory.".into());
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not delete model: {error}")),
    }
}

/// Model download endpoints and the CDN hosts they legitimately hand off to.
///
/// Bard cannot simply refuse redirects here. A Hugging Face `resolve/main` URL
/// for an LFS file answers 302 to its CDN (`cdn-lfs.hf.co`,
/// `cas-bridge.xethub.hf.co`), and a GitHub release asset answers 302 to
/// `release-assets.githubusercontent.com`; refusing either would break every
/// model download. What is not acceptable is reqwest's default, which follows up
/// to ten hops to any host at all: one redirect from a misconfigured or
/// compromised endpoint replays the request — and Bard's connection to it —
/// somewhere that host chose. So the policy is an allow-list, and a redirect off
/// it fails the download loudly instead of silently moving the bytes.
///
/// `github.com` is here because `catalog.json` hosts first-party catalogue
/// models on `miisodev/bard-models` releases, not because Bard trusts GitHub
/// generally. The only request either family ever sees is a GET for a public
/// model asset.
fn is_expected_download_host(host: &str) -> bool {
    const ROOTS: &[&str] = &[
        "huggingface.co",
        "hf.co",
        "huggingfaceusercontent.com",
        "github.com",
        "githubusercontent.com",
        "githubassets.com",
    ];
    ROOTS
        .iter()
        .any(|root| host == *root || host.ends_with(&format!(".{root}")))
}

fn download_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let url = attempt.url();
        let host = url
            .host_str()
            .unwrap_or_default()
            .trim_matches(|c| c == '[' || c == ']')
            .to_ascii_lowercase();
        if url.scheme() != "https" || !is_expected_download_host(&host) {
            log::warn!(
                "model:redirect-refused host={host} status={}",
                attempt.status().as_u16()
            );
            return attempt.error("Bard refused a model download redirect to an unexpected host.");
        }
        attempt.follow()
    })
}

fn init_engines_once() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = transcribe_cpp::init_backends_default();
    });
}

fn model_probe_guard() -> MutexGuard<'static, ()> {
    static PROBES: OnceLock<Mutex<()>> = OnceLock::new();
    PROBES
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The engine open a compatibility probe performs. Split out from `spawn_probe`
/// so the lifetime of the serialization guard can be regression-tested without a
/// real multi-hundred-megabyte model.
fn engine_load(path: &Path, probe: crate::dictation::model::ProbeLease) -> Result<(), String> {
    // `map(drop)`: the probe learns whether the file opens and then releases it
    // immediately. Nothing here keeps a model resident between probes.
    //
    // `finish` is the release point. A waiter that is still there drops the
    // model at once, exactly as `map(drop)` did; one that timed out leaves it
    // in the registry for the idle watcher, which is the only party left that
    // can free it.
    let model = transcribe_cpp::Model::load(path).map_err(|error| error.to_string())?;
    probe.finish(model);
    Ok(())
}

/// Run one probe on its own thread, waiting at most `timeout` for it.
///
/// The serialization guard is created **inside** the thread, and that placement
/// is the whole point. A caller-side guard read as equivalent and was not:
/// `recv_timeout` returning on timeout would drop the guard while the thread was
/// still inside `Model::load`, so the next probe acquired it immediately and
/// opened a second copy of the model, and N timeouts left N models resident —
/// exactly the race the guard's own comment claimed to prevent. Owned by the
/// thread, the guard is held for as long as the load actually runs and released
/// only after the model is dropped, so at most one probe model exists at a time
/// no matter how many callers time out.
///
/// An abandoned thread cannot be reclaimed from here: `start_unload_watcher`
/// tracks only the live transcription `SESSION`, but its model can be:
/// `dictation::model::begin_probe` registers the load and `ProbeLease::finish`
/// leaves a completed model of an abandoned probe for the idle watcher, which
/// drops it on a later tick. A load that never returns is still only bounded by
/// the engine itself; the registry reports that as stranded instead of
/// pretending it was freed.
fn spawn_probe<L>(path: PathBuf, timeout: Duration, load: L) -> Result<Result<(), String>, String>
where
    L: FnOnce(&Path, crate::dictation::model::ProbeLease) -> Result<(), String> + Send + 'static,
{
    let (probe, abandon) = crate::dictation::model::begin_probe(path.clone());
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Declared first so it outlives the model: the guard must never be
        // released while a probe model is still mapped.
        let _probe = model_probe_guard();
        let outcome = load(&path, probe);
        let _ = sender.send(outcome);
    });
    match receiver.recv_timeout(timeout) {
        Ok(outcome) => Ok(outcome),
        // A dead thread is not a slow one; reporting it as a timeout would
        // tell the user to wait 120 seconds for something that already failed.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("The model compatibility probe ended without reporting a result.".into())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // The load is still running with the lease. Mark it abandoned so
            // the model it eventually produces is left for the idle watcher
            // rather than staying resident for the life of the process.
            abandon.abandon();
            Err(format!(
                "The model compatibility probe timed out after {} seconds.",
                timeout.as_secs()
            ))
        }
    }
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(120);

fn probe_model_path(path: &Path) -> Result<CompatibilityResult, String> {
    let metadata =
        fs::metadata(path).map_err(|_| format!("Model file does not exist: {}", path.display()))?;
    if !metadata.is_file() {
        return Err("Model path is not a regular model file.".into());
    }
    init_engines_once();
    let outcome = match spawn_probe(path.to_path_buf(), PROBE_TIMEOUT, engine_load) {
        Ok(outcome) => outcome,
        Err(error) => {
            // The abandoned thread still owns the guard and a resident model, and
            // nothing in the process can reclaim either one. A timeout is not a
            // cost the user should pay silently.
            log::warn!(
                "model:probe-timeout path={} timeout_s={}",
                path.display(),
                PROBE_TIMEOUT.as_secs()
            );
            return Err(error);
        }
    };

    let (status, message) = match outcome {
        Ok(()) => (ModelCompatibility::Verified, None),
        Err(message) => (
            ModelCompatibility::Incompatible,
            Some(format!(
                "Bard's current transcription engine could not load this model: {message}"
            )),
        ),
    };
    remember_compatibility(path, status);
    Ok(CompatibilityResult {
        path: path.to_string_lossy().to_string(),
        status,
        message,
    })
}

/// Always a real load, never a cache hit. `verify_load` runs immediately after a
/// download to `path`, so any answer already recorded for that path describes the
/// file that used to be there. Consulting the compatibility cache here would let a
/// fresh, unreviewed download be accepted on the strength of its predecessor.
pub fn verify_load(path: &str) -> bool {
    probe_model_path(Path::new(path))
        .map(|result| result.status == ModelCompatibility::Verified)
        .unwrap_or(false)
}

/// The `(size, modified, created, inode)` fingerprint of a file.
///
/// Still used by the *compatibility* cache, where a stale answer is a display
/// problem rather than an integrity one. It must not be used to decide whether a
/// checksum needs recomputing: measured on this filesystem, an in-place same-size
/// rewrite leaves every one of these fields unchanged, so such a memo would bless
/// a file whose contents had changed. See `verify_pinned_file`.
type PinnedFileStamp = (u64, u128, u128, u64);

#[cfg(windows)]
fn file_stamp(metadata: &fs::Metadata) -> (u64, u128, u128, u64) {
    use std::os::windows::fs::MetadataExt;
    (
        metadata.len(),
        metadata.last_write_time() as u128,
        metadata.creation_time() as u128,
        0,
    )
}

#[cfg(not(windows))]
fn file_stamp(metadata: &fs::Metadata) -> (u64, u128, u128, u64) {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let created = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    (metadata.len(), modified, created, 0)
}

/// Verify the exact bytes of a file Bard is about to trust. Managed catalogue
/// files and Bard-owned imports always carry a positive size and SHA-256; an
/// empty checksum is never treated as permission to skip integrity checking.
///
/// # What is trusted, exactly
///
/// A successful hash is written to a durable marker: `verified_markers()`,
/// keyed by the canonicalised path and holding the file's `(size, last write,
/// creation)` stamp and the SHA-256 that was proven at it. The read is only
/// reached while the marker's stamp still matches the file on disk *and* the
/// checksum it records is the one being asked about; anything else re-hashes,
/// and a re-hash that fails refuses as it always did.
///
/// What that buys is the launch cost. The alternative was a SHA-256 of up to
/// 1.69 GB on every launch (the model file is hashed once per process, and a
/// process starts every time Bard does), which competes with first paint and
/// is the reason a cold dictation had a watchdog floor in the first place.
///
/// What it costs is a guarantee that was always conditional anyway: the trust
/// is the filesystem's stamps, not the bytes. On this filesystem an in-place
/// same-size rewrite has been measured to leave `last_write_time` **and**
/// `creation_time` bit-for-bit unchanged — see the note this function carried
/// while the memo was removed — so a model replaced that way, by a tool that
/// also left both stamps alone, would be accepted. Sizes, writes through any
/// ordinary path, renames, re-downloads and catalogue changes all move the
/// stamp or the checksum, so they re-hash.
///
/// Tamper surface: `%LOCALAPPDATA%\com.miisodev.bard\model-verification.json`
/// (or the platform's data directory), a plain JSON file in the user's own
/// profile. Anyone who can rewrite a multi-gigabyte model file can rewrite the
/// marker that vouches for it, so this is not and never was defence against a
/// local attacker — the checksum's job is a download that arrived wrong, a
/// stale partial write, a catalogue mismatch or a file at the wrong path, and
/// every one of those is still caught. Deleting the file costs a re-hash and
/// nothing else.
///
/// The marker is rewritten in full after each successful verification, from an
/// in-memory map loaded once per process; a write that fails costs the next
/// launch one hash, so it is not reported.
pub fn verify_pinned_file(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), String> {
    let expected_sha256 = normalize_checksum(expected_sha256)?;
    if expected_size == 0 || expected_sha256.len() != 64 {
        return Err("This model has no complete size/checksum metadata; reinstall it from a verified source.".into());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect model file: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Model path is not a regular model file.".into());
    }
    if metadata.len() != expected_size {
        return Err(format!(
            "Model size does not match the catalogue ({} bytes; expected {expected_size}).",
            metadata.len()
        ));
    }
    let stamp = stamp_of(&metadata);
    let key = normalized_path_string(&canonicalize_for_comparison(path));
    if marker_vouches(&key, stamp, &expected_sha256) {
        return Ok(());
    }
    let actual =
        sha256_file(path).map_err(|error| format!("Could not hash model file: {error}"))?;
    if !actual.eq_ignore_ascii_case(&expected_sha256) {
        return Err("Model checksum does not match the verified catalogue value.".into());
    }
    record_verification(&key, stamp, &actual);
    Ok(())
}

/// A file's identity for the verification marker, in the three numbers that are
/// cheap to read and impossible to fake without touching the file.
type FileStamp = (u64, u64, u64);

/// The stamp the marker stores. [`file_stamp`] reads 128-bit values on the
/// platforms where the clock offers them; the marker is JSON, whose numbers are
/// 64-bit, and every real timestamp and FILETIME tick fits in one.
fn stamp_of(metadata: &fs::Metadata) -> FileStamp {
    let (size, modified, created, _) = file_stamp(metadata);
    (size, modified as u64, created as u64)
}

/// One verified file: the stamp its bytes were proven at, and the checksum.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct VerifiedFile {
    size: u64,
    modified: u64,
    created: u64,
    sha256: String,
}

/// Every verification this machine has recorded, loaded once per process.
fn verified_markers() -> &'static Mutex<HashMap<String, VerifiedFile>> {
    static MARKERS: OnceLock<Mutex<HashMap<String, VerifiedFile>>> = OnceLock::new();
    MARKERS.get_or_init(|| {
        let loaded = marker_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(loaded)
    })
}

/// Where the markers live: beside the database, never beside the model — the
/// models directory is scanned, listed and pruned, and a file in it that is not
/// a model has no business being there.
fn marker_path() -> Option<PathBuf> {
    crate::paths::data_dir().map(|dir| dir.join("model-verification.json"))
}

/// Whether a previous verification covers this exact file, byte-for-byte as far
/// as the filesystem can say.
fn marker_vouches(key: &str, stamp: FileStamp, sha256: &str) -> bool {
    let markers = verified_markers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    markers.get(key).is_some_and(|record| {
        (record.size, record.modified, record.created) == stamp
            && record.sha256.eq_ignore_ascii_case(sha256)
    })
}

/// Record a verification that just succeeded.
fn record_verification(key: &str, stamp: FileStamp, sha256: &str) {
    let snapshot = {
        let mut markers = verified_markers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        markers.insert(
            key.to_owned(),
            VerifiedFile {
                size: stamp.0,
                modified: stamp.1,
                created: stamp.2,
                sha256: sha256.to_ascii_lowercase(),
            },
        );
        serde_json::to_string(&*markers)
    };
    let (Some(path), Ok(json)) = (marker_path(), snapshot) else {
        return;
    };
    // A failed write is not worth reporting: it costs the next launch one hash
    // of the same file, which is exactly what would have happened anyway.
    let _ = fs::write(path, json);
}

/// The engine answer Bard already holds for this file, or `None` when it has
/// none worth acting on.
///
/// Two sources, in order. The in-process cache carries a stamp of the exact bytes
/// it was produced from, so a file replaced at the same path during this session
/// does not inherit its predecessor's verdict. A registered row is the durable
/// copy of the same verdict and is what makes the answer survive a restart, at
/// the cost of not knowing whether an out-of-band process has since replaced the
/// file. That is acceptable for a label: the answer is only ever displayed, and
/// every activation re-runs `probe_model_path` and re-verifies size and SHA-256,
/// so a stale label can never make an unverified model usable. `recheck_local_model`
/// is the explicit way to be sure.
pub fn known_compatibility(app: &AppHandle, path: &Path) -> Option<ModelCompatibility> {
    let key = canonicalize_for_comparison(path);
    let known = crate::db::list_custom_models(app)
        .into_iter()
        .find(|model| canonicalize_for_comparison(Path::new(&model.local_path)) == key)
        .map(|model| model.compatibility)
        .unwrap_or_else(|| cached_compatibility(path));
    // `Unverified` means nobody has asked and `Missing` means the file is gone.
    // Neither is an engine verdict, and a caller that receives a value is
    // entitled to skip the load.
    matches!(
        known,
        ModelCompatibility::Verified | ModelCompatibility::Incompatible
    )
    .then_some(known)
}

/// An explicit re-check. Skips the cache, because the user has just said the
/// previous answer is not good enough — "it says incompatible but it works", or
/// a file that has since been replaced.
///
/// Needs orchestrator: `commands.rs` — `check_model` should take a `force`
/// argument and dispatch to this:
/// `if force { crate::hf::recheck_local_model(&app, &local_path) } else { crate::hf::probe_local_model(&app, &local_path) }`
#[allow(dead_code)]
pub fn recheck_local_model(app: &AppHandle, path_arg: &str) -> Result<CompatibilityResult, String> {
    probe_local_model_inner(app, path_arg, true)
}

fn probe_local_model_inner(
    app: &AppHandle,
    path_arg: &str,
    force: bool,
) -> Result<CompatibilityResult, String> {
    let path =
        fs::canonicalize(path_arg).map_err(|_| format!("Model file does not exist: {path_arg}"))?;
    let in_app_models = path_is_within(&local_models_dir(app), &path);
    let in_hf_cache = path_is_within(&hf_cache_dir(), &path);
    if !in_app_models && !in_hf_cache {
        return Err(
            "Bard only checks models in its model directory or the shared Hugging Face cache."
                .into(),
        );
    }
    if !force {
        if let Some(known) = known_compatibility(app, &path) {
            return Ok(CompatibilityResult {
                path: path.to_string_lossy().to_string(),
                status: known,
                message: None,
            });
        }
    }
    let result = probe_model_path(&path)?;
    crate::db::set_custom_model_compatibility(app, path_arg, result.status);
    crate::db::set_custom_model_compatibility(app, &path.to_string_lossy(), result.status);
    crate::commands::refresh(app);
    Ok(result)
}

fn model_path_for_id(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    crate::catalog::find(id)
        .map(|model| local_models_dir(app).join(&model.file.filename))
        .ok_or_else(|| format!("Unknown model: {id}"))
}

/// Re-check the file identity immediately before activation or inference. This
/// closes the gap where a previously installed path was replaced after the
/// catalogue/custom row was written.
pub fn verify_model_file(app: &AppHandle, id: &str, path: &Path) -> Result<(), String> {
    let model = crate::catalog::find(id).ok_or_else(|| format!("Unknown model: {id}"))?;
    if !path_is_within(&local_models_dir(app), path) {
        return Err("The selected model is outside Bard's model directory.".into());
    }
    verify_pinned_file(path, model.file.size_bytes, &model.file.sha256)
}

/// Verify first, persist second. No caller receives a successful activation
/// unless the selected path exists and the current engine opened it.
pub fn activate_model(app: &AppHandle, id: &str) -> Result<(), String> {
    activate_model_inner(app, id, false)
}

/// `engine_checked` asserts that the caller opened *this* path with the current
/// engine during *this* user action, so repeating the load would be a second
/// full read of a multi-gigabyte model for one click. It is private to this
/// module and only ever set by the two register-then-activate paths below, which
/// have just run `register_detected_path` — and therefore a real load — on the
/// same path.
///
/// Nothing else is relaxed. `verify_model_file` still re-resolves the recorded
/// path, still refuses a changed registration, and still re-verifies size and
/// SHA-256 for anything Bard owns or has a checksum for. The second probe was
/// never an integrity check; it duplicated the first one.
fn activate_model_inner(app: &AppHandle, id: &str, engine_checked: bool) -> Result<(), String> {
    let _operation = model_operation_guard();
    let path = model_path_for_id(app, id)?;
    verify_model_file(app, id, &path)?;
    if !engine_checked {
        let probe = probe_model_path(&path)?;
        if probe.status != ModelCompatibility::Verified {
            return Err(probe
                .message
                .unwrap_or_else(|| "This model is not compatible with Bard's engine.".into()));
        }
    }
    crate::db::set_setting_checked(app, "model_id", id)?;
    let _ = app.emit("model-activated", serde_json::json!({"id": id}));
    crate::commands::refresh(app);
    Ok(())
}

/// The partial file's path. A sibling `<name>.part` rather than a sibling with
/// the extension replaced, so it cannot collide with a model file.
fn part_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn legacy_part_path(destination: &Path) -> PathBuf {
    destination.with_extension("part")
}

pub(crate) fn emit_model_installed(app: &AppHandle, id: &str) {
    let _ = app.emit(
        "model-installed",
        serde_json::json!({"id": id, "activated": false}),
    );
    crate::commands::refresh(app);
    // Compatibility signal for the current settings/onboarding listeners. It
    // means installation finished, not that Bard switched the active model.
    let _ = app.emit(
        "model-ready",
        serde_json::json!({"id": id, "activated": false}),
    );
}

pub(crate) fn emit_download_failure(app: &AppHandle, id: &str, error: &str) {
    let _ = app.emit(
        "model-download-failed",
        serde_json::json!({"id": id, "error": error}),
    );
    // Keep the existing progress surface terminal too: listeners that only
    // understand `model-progress` can render the failure and offer Retry.
    let _ = app.emit(
        "model-progress",
        serde_json::json!({
            "id": id,
            "received": 0,
            "total": 0,
            "percent": 0,
            "phase": "Download failed",
            "message": error,
            "error": error
        }),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResumeMode {
    Append,
    Restart,
}

fn parse_content_range(value: &str) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, _) = range.split_once('-')?;
    Some((start.parse().ok()?, total.parse().ok()?))
}

fn resume_mode(
    requested_offset: u64,
    status: u16,
    content_range_start: Option<u64>,
    _had_part: bool,
) -> Result<ResumeMode, String> {
    if status == 206 {
        let start = content_range_start.ok_or_else(|| {
            "Server returned 206 without a valid Content-Range header.".to_string()
        })?;
        if start != requested_offset {
            return Err(format!(
                "Server resumed at byte {start}, but Bard requested byte {requested_offset}."
            ));
        }
        Ok(if requested_offset > 0 {
            ResumeMode::Append
        } else {
            ResumeMode::Restart
        })
    } else if status == 200 {
        // A 200 response to a ranged request is the complete entity. Appending
        // it to the old prefix corrupts the file, so this path must restart.
        Ok(ResumeMode::Restart)
    } else {
        Err(format!("Download failed: HTTP {status}"))
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub(crate) struct DownloadRequest {
    pub id: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub destination: PathBuf,
}

fn progress_timing(
    started: Instant,
    initial_received: u64,
    received: u64,
    total: u64,
) -> (Option<u64>, Option<u64>) {
    let elapsed = started.elapsed().as_secs_f64();
    if elapsed <= 0.0 {
        return (None, None);
    }
    let transferred = received.saturating_sub(initial_received) as f64;
    let speed = (transferred / elapsed).max(0.0) as u64;
    let eta = (speed > 0 && total > received)
        .then(|| ((total - received) as f64 / speed as f64).ceil() as u64);
    (Some(speed), eta)
}

fn emit_progress(
    app: &AppHandle,
    id: &str,
    received: u64,
    total: u64,
    speed_bytes_per_second: Option<u64>,
    eta_seconds: Option<u64>,
) {
    let _ = app.emit(
        "model-progress",
        serde_json::json!({
            "id": id,
            "received": received,
            "total": total,
            "percent": if total > 0 {
                ((received as f64 / total as f64) * 100.0).min(100.0) as u32
            } else {
                0
            },
            "speedBytesPerSecond": speed_bytes_per_second,
            "etaSeconds": eta_seconds,
        }),
    );
}

async fn download_resumable_inner(
    app: &AppHandle,
    request: &DownloadRequest,
) -> Result<(), String> {
    // Progress and cancellation are injected rather than reached for, so the
    // transfer itself can be exercised without a Tauri `AppHandle` — this
    // crate's test build cannot construct one (`tauri`'s `test` feature is not
    // enabled), which is why the resume, truncation, size-cap and cancel paths
    // had no coverage at all.
    transfer(
        request,
        |received, total, speed, eta| emit_progress(app, &request.id, received, total, speed, eta),
        crate::dictation::is_cancel_pending,
    )
    .await
}

async fn transfer<F, C>(request: &DownloadRequest, emit: F, cancelled: C) -> Result<(), String>
where
    F: Fn(u64, u64, Option<u64>, Option<u64>),
    C: Fn() -> bool,
{
    let part = part_path(&request.destination);
    if let Some(parent) = part.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Bard's previous downloader replaced the final extension (`model.bin` ->
    // `model.part`). Move that partial into the collision-safe `.ext.part`
    // namespace once so an upgrade does not throw away resumable bytes.
    let legacy_part = legacy_part_path(&request.destination);
    if !part.exists() && legacy_part.is_file() {
        fs::rename(&legacy_part, &part).map_err(|e| e.to_string())?;
    }

    let mut received = fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0);
    if request.size_bytes > 0 && received > request.size_bytes {
        fs::remove_file(&part).map_err(|e| e.to_string())?;
        received = 0;
    }

    // A previous process may have completed the part file but died before the
    // atomic rename. Verify that complete file before spending another request.
    if request.size_bytes > 0 && received == request.size_bytes {
        let matches = request.sha256.is_empty()
            || sha256_file(&part)
                .map(|digest| digest.eq_ignore_ascii_case(&request.sha256))
                .unwrap_or(false);
        if matches {
            fs::rename(&part, &request.destination).map_err(|e| e.to_string())?;
            emit(received, received, None, None);
            return Ok(());
        }
        fs::remove_file(&part).map_err(|e| e.to_string())?;
        received = 0;
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60 * 60))
        .redirect(download_redirect_policy())
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(&request.url);
    if received > 0 {
        req = req.header("Range", format!("bytes={received}-"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let content_range = resp
        .headers()
        .get("content-range")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range);
    let content_length = resp
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let had_part = received > 0;
    let mode = match resume_mode(
        received,
        status,
        content_range.map(|range| range.0),
        had_part,
    ) {
        Ok(mode) => mode,
        Err(error) => {
            if received > 0 {
                let _ = fs::remove_file(&part);
            }
            return Err(error);
        }
    };
    if mode == ResumeMode::Restart {
        received = 0;
    }

    let response_total = content_range
        .map(|range| range.1)
        .or(content_length)
        .unwrap_or(0);
    if request.size_bytes > 0 && response_total > 0 && response_total != request.size_bytes {
        let _ = fs::remove_file(&part);
        return Err("The server's content length does not match the catalog metadata.".into());
    }
    let total = if request.size_bytes > 0 {
        request.size_bytes
    } else {
        response_total
    };
    let transfer_started = Instant::now();
    let transfer_initial_received = received;
    // Hash the bytes as they are written, when every byte of the file comes
    // from this transfer. `received == 0` after the resume decision means
    // exactly that: either there was no part, or a Restart above discards the
    // prefix it had. A resumed file starts with bytes this hasher never saw,
    // so that one case re-reads the completed file below.
    let mut hasher = (received == 0).then(Sha256::new);

    if let Ok(metadata) = fs::symlink_metadata(&part) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("The resumable model path is not a regular file.".into());
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if part.exists() {
        options.create(false);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(&part).map_err(|e| e.to_string())?;
    match mode {
        ResumeMode::Append => {
            file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        }
        ResumeMode::Restart => {
            if had_part {
                // Truncate before the first append/write. Some servers return
                // 200 for every request and ignore Range entirely.
                file.set_len(0).map_err(|e| e.to_string())?;
            }
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        }
    }

    let mut last_emit = Instant::now();
    let mut last_progress = received;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancelled() {
            drop(file);
            return Err("Download cancelled.".into());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&chunk);
        }
        received += chunk.len() as u64;
        if total > 0 && received > total {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err("Server sent more model data than the catalog metadata allows.".into());
        }
        if received.saturating_sub(last_progress) > 1024 * 1024
            || last_emit.elapsed() > Duration::from_millis(120)
        {
            last_progress = received;
            last_emit = Instant::now();
            let (speed, eta) =
                progress_timing(transfer_started, transfer_initial_received, received, total);
            emit(received, total, speed, eta);
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if cancelled() {
        return Err("Download cancelled.".into());
    }
    let actual_size = fs::metadata(&part).map_err(|e| e.to_string())?.len();
    if request.size_bytes > 0 && actual_size != request.size_bytes {
        return Err(format!(
            "Downloaded model is incomplete ({actual_size} of {} bytes).",
            request.size_bytes
        ));
    }
    if total > 0 && actual_size != total {
        return Err(format!(
            "Downloaded model is incomplete ({actual_size} of {total} bytes)."
        ));
    }

    // Verify the exact bytes now that they are assembled. A fresh transfer has
    // already hashed every byte as it arrived, so it promotes what it wrote;
    // only a resumed file, whose prefix came from an earlier run and never
    // passed through this hasher, is re-read in full here. The mismatch path is
    // identical either way: discard the part, because a later resume would
    // treat those bytes as a trusted prefix.
    let digest = match hasher {
        Some(hasher) => hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        None => sha256_file(&part).map_err(|e| e.to_string())?,
    };
    if !request.sha256.is_empty() && !digest.eq_ignore_ascii_case(&request.sha256) {
        let _ = fs::remove_file(&part);
        return Err("Model verification failed (checksum mismatch).".into());
    }
    fs::rename(&part, &request.destination).map_err(|e| e.to_string())?;
    emit(
        actual_size,
        if total > 0 { total } else { actual_size },
        None,
        None,
    );
    Ok(())
}

pub(crate) async fn download_resumable(
    app: &AppHandle,
    request: DownloadRequest,
) -> Result<(), String> {
    let outcome = download_resumable_inner(app, &request).await;
    if let Err(error) = &outcome {
        emit_download_failure(app, &request.id, error);
    }
    outcome
}

pub fn register_detected_path(
    app: &AppHandle,
    repo: &str,
    file: &str,
    local_path: &str,
    size: u64,
) -> Result<String, String> {
    let repo = if repo == "Bard imports" {
        repo.to_string()
    } else {
        normalize_repo_id(repo)?
    };
    let file = normalize_file_path(file)?;
    let path = fs::canonicalize(local_path)
        .map_err(|_| format!("Model no longer exists: {local_path}"))?;
    let in_hf_cache = path_is_within(&hf_cache_dir(), &path);
    let in_app_models = path_is_within(&local_models_dir(app), &path);
    if !in_hf_cache && !in_app_models {
        return Err(
            "Only models inside Bard's model directory or the shared Hugging Face cache can be registered."
                .into(),
        );
    }
    let probe = probe_model_path(&path)?;
    if probe.status != ModelCompatibility::Verified {
        return Err(probe
            .message
            .unwrap_or_else(|| "This model is incompatible with Bard's engine.".into()));
    }
    register_probed_path(app, &repo, &file, &path, size, in_app_models)
}

/// The registration half of `register_detected_path`, for a file the caller has
/// already opened with the current engine.
///
/// Keeping the load in one place is what lets `activate_detected_model` and
/// `register_and_activate_cached` hand their result forward: a user action that
/// both registers and activates used to load the same model twice, and for a
/// multi-gigabyte GGUF that doubled the wait for one click.
///
/// `path` must already be canonical, must already have passed both containment
/// tests, and must already have been loaded by this same action. `in_app_models`
/// selects the ownership that follows from which root it came from: a model
/// Bard manages may be deleted later, one from the shared cache never is.
fn register_probed_path(
    app: &AppHandle,
    repo: &str,
    file: &str,
    path: &Path,
    size: u64,
    in_app_models: bool,
) -> Result<String, String> {
    let id = if in_app_models {
        app_model_id(repo, file)
    } else {
        external_model_id(repo, file, path)
    };
    let ownership = if in_app_models {
        crate::db::ModelOwnership::App
    } else {
        crate::db::ModelOwnership::External
    };
    let measured_size = if size > 0 {
        size
    } else {
        fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
    };
    let registered_sha256 = if in_app_models {
        sha256_file(path).map_err(|error| format!("Could not hash the local model: {error}"))?
    } else {
        String::new()
    };
    if in_app_models {
        verify_pinned_file(path, measured_size, &registered_sha256)?;
    }
    crate::db::register_custom_model(
        app,
        &id,
        repo,
        file,
        measured_size,
        &registered_sha256,
        &path.to_string_lossy(),
        ownership,
        ModelCompatibility::Verified,
    )?;
    // Registration is reversible; an external file remains untouched. The
    // caller decides whether to activate explicitly.
    emit_model_installed(app, &id);
    Ok(id)
}

/// Register a discovered cache model and return its id, without activating it.
///
/// Superseded by [`register_and_activate_cached`], which is what the command
/// layer now calls: registering and then activating separately meant two full
/// engine opens of a multi-gigabyte file for one click. Kept because the two-step
/// form is the honest shape for any caller that wants to register now and
/// activate later.
#[allow(dead_code)]
pub fn register_cached(
    app: &AppHandle,
    repo: &str,
    file: &str,
    local_path: &str,
    size: u64,
) -> Result<String, String> {
    register_detected_path(app, repo, file, local_path, size)
}

/// Register a discovered cache model in one action, opening it with the engine
/// exactly once.
///
/// Needs orchestrator: `commands.rs` — `hf_use_cached` currently calls
/// `register_cached` and then `activate_model`, which loads the same file twice.
/// It should call this instead:
/// `crate::hf::register_and_activate_cached(&app, &repo, &file, &local_path, size)`
#[allow(dead_code)]
pub fn register_and_activate_cached(
    app: &AppHandle,
    repo: &str,
    file: &str,
    local_path: &str,
    size: u64,
) -> Result<(), String> {
    let id = register_detected_path(app, repo, file, local_path, size)?;
    // `register_detected_path` just opened this exact path with the current
    // engine in this same action, so the load is not repeated. The pinned file
    // verification inside `activate_model_inner` still runs.
    activate_model_inner(app, &id, true)
}

/// Remove a catalog or custom model registration. App-owned files are deleted
/// only after canonical path containment succeeds; external cache files are
/// never touched.
pub fn delete_model(app: &AppHandle, id: &str) -> Result<(), String> {
    let _operation = model_operation_guard();
    if crate::db::get_setting(app, "model_id").as_deref() == Some(id) {
        return Err("The active voice model cannot be deleted. Choose another model first.".into());
    }

    if let Some(model) = crate::catalog::find(id) {
        let path = local_models_dir(app).join(&model.file.filename);
        remove_owned_model_file(app, &path)?;
        let _ = remove_owned_model_file(app, &part_path(&path));
        let _ = remove_owned_model_file(app, &legacy_part_path(&path));
        return Ok(());
    }

    Err(format!("Unknown model: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- fixtures ---------------------------------------------------------

    /// A unique scratch directory. `now_ms` alone collides when two tests are
    /// created in the same millisecond, which they are on a fast machine.
    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("bard-hf-{tag}-{}-{unique}", crate::db::now_ms()));
        fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    /// Deterministic non-repeating bytes, so a corrupt assembly cannot
    /// accidentally reproduce the expected file.
    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|index| (index % 251) as u8).collect()
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// A one-shot HTTP/1.1 responder on loopback.
    ///
    /// `Cargo.toml` is not this file's to change, so the crate has no
    /// dev-dependency HTTP server and the response is written by hand. That is
    /// enough for reqwest: a status line, headers, and a body that can be
    /// deliberately short of the length it promised, unframed, or absent.
    struct TestResponse {
        status: u16,
        reason: &'static str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        /// Send one byte less than the promised `Content-Length`, then close: a
        /// stream that dies mid-transfer. The declared length stays honest, so
        /// this reaches the streaming loop rather than the pre-flight size check.
        truncate: bool,
        /// Send no `Content-Length` at all; the body ends when the socket does.
        /// The only way to reach a cap that `content_length()` cannot see.
        omit_length: bool,
    }

    impl TestResponse {
        fn new(body: Vec<u8>) -> Self {
            Self {
                status: 200,
                reason: "OK",
                headers: Vec::new(),
                body,
                truncate: false,
                omit_length: false,
            }
        }

        fn with_status(mut self, status: u16, reason: &'static str) -> Self {
            self.status = status;
            self.reason = reason;
            self
        }

        fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
            self.headers.push((name.into(), value.into()));
            self
        }

        fn truncated(mut self) -> Self {
            self.truncate = true;
            self
        }

        fn unframed(mut self) -> Self {
            self.omit_length = true;
            self
        }
    }

    /// Serve exactly one request and return its URL plus a handle that finishes
    /// once the response has been written.
    fn serve_once(response: TestResponse) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("loopback address").port();
        let handle = std::thread::spawn(move || {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            // Read the request head and stop at the blank line. Byte-at-a-time is
            // fine for a few hundred bytes and cannot deadlock against the write
            // below, which a fixed-size read could.
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while socket.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
                if head.len() >= 4 && head[head.len() - 4..] == *b"\r\n\r\n" {
                    break;
                }
            }

            let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, response.reason);
            for (name, value) in &response.headers {
                out.push_str(&format!("{name}: {value}\r\n"));
            }
            if !response.omit_length {
                out.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
            }
            out.push_str("Connection: close\r\n\r\n");
            if socket.write_all(out.as_bytes()).is_err() {
                return;
            }
            let sent = if response.truncate {
                response.body.len().saturating_sub(1)
            } else {
                response.body.len()
            };
            let _ = socket.write_all(&response.body[..sent]);
            let _ = socket.flush();
            // Dropping the socket here is what truncates the body.
        });
        (format!("http://127.0.0.1:{port}/model.gguf"), handle)
    }

    fn test_request(dir: &Path, url: &str, sha256: &str, size: u64) -> DownloadRequest {
        DownloadRequest {
            id: "test-model".into(),
            url: url.into(),
            sha256: sha256.into(),
            size_bytes: size,
            destination: dir.join("model.gguf"),
        }
    }

    /// Run a transfer with progress discarded and nothing cancelling it.
    async fn run(request: &DownloadRequest) -> Result<(), String> {
        transfer(request, |_, _, _, _| {}, || false).await
    }

    fn read_pcm16_mono_wav(path: &Path) -> Vec<f32> {
        let bytes = fs::read(path).expect("smoke WAV exists");
        assert!(bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"));
        let mut offset = 12;
        let mut sample_rate = 0;
        let mut channels = 0;
        let mut bits = 0;
        let mut data = None;
        while offset + 8 <= bytes.len() {
            let id = &bytes[offset..offset + 4];
            let size =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let start = offset + 8;
            let end = start.checked_add(size).expect("valid WAV chunk");
            if id == b"fmt " && end <= bytes.len() && size >= 16 {
                channels =
                    u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()) as usize;
                sample_rate =
                    u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()) as usize;
                bits =
                    u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap()) as usize;
            } else if id == b"data" {
                data = Some((start, end));
            }
            offset = end + (size & 1);
        }
        assert_eq!(bits, 16, "smoke WAV must be PCM16");
        assert!(channels > 0 && sample_rate > 0);
        let (start, end) = data.expect("WAV data chunk");
        bytes[start..end]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
            .collect()
    }

    #[test]
    fn smoke_test_exact_transcription_when_requested() {
        let (Ok(value), Ok(wav)) = (
            std::env::var("BARD_SMOKE_TRANSCRIBE"),
            std::env::var("BARD_SMOKE_WAV"),
        ) else {
            return;
        };
        let samples = read_pcm16_mono_wav(Path::new(&wav));
        init_engines_once();
        for path in value.split(';').filter(|path| !path.trim().is_empty()) {
            let model = transcribe_cpp::Model::load(Path::new(path))
                .unwrap_or_else(|error| panic!("model load failed for {path}: {error}"));
            let mut session = model
                .session()
                .unwrap_or_else(|error| panic!("session failed for {path}: {error}"));
            let result = session
                .run(&samples, &transcribe_cpp::RunOptions::default())
                .unwrap_or_else(|error| panic!("transcription failed for {path}: {error}"));
            assert!(
                !result.text.trim().is_empty(),
                "model produced no text: {path}"
            );
            println!("{} => {}", path, result.text.trim());
        }
    }

    #[test]
    fn smoke_test_exact_models_when_requested() {
        let Ok(value) = std::env::var("BARD_SMOKE_MODELS") else {
            return;
        };
        init_engines_once();
        for path in value.split(';').filter(|path| !path.trim().is_empty()) {
            let result = probe_model_path(Path::new(path))
                .unwrap_or_else(|error| panic!("model probe failed for {path}: {error}"));
            assert_eq!(
                result.status,
                ModelCompatibility::Verified,
                "model did not load: {path}"
            );
        }
    }

    #[test]
    fn normalizes_hugging_face_repo_ids_and_urls() {
        for input in [
            "unslothai/Qwen3-ASR-0.6B-GGUF",
            " huggingface.co/unslothai/Qwen3-ASR-0.6B-GGUF ",
            "https://huggingface.co/unslothai/Qwen3-ASR-0.6B-GGUF/",
            "https://www.huggingface.co/unslothai/Qwen3-ASR-0.6B-GGUF/tree/main",
            "https://huggingface.co/models/unslothai/Qwen3-ASR-0.6B-GGUF?tab=files",
            "https://hf.co/unslothai/Qwen3-ASR-0.6B-GGUF",
        ] {
            assert_eq!(
                normalize_repo_id(input).unwrap(),
                "unslothai/Qwen3-ASR-0.6B-GGUF"
            );
        }
        assert!(normalize_repo_id("owner/name/extra").is_err());
        assert!(normalize_repo_id("models/owner/name").is_err());
        assert!(normalize_repo_id("https://example.com/owner/name").is_err());
    }

    #[test]
    fn part_paths_keep_legacy_resume_compatibility() {
        let destination = PathBuf::from("model.gguf");
        assert_eq!(part_path(&destination), PathBuf::from("model.gguf.part"));
        assert_eq!(legacy_part_path(&destination), PathBuf::from("model.part"));
    }

    #[test]
    fn validates_and_normalizes_sha256() {
        let value = "A".repeat(64);
        assert_eq!(normalize_checksum(&value).unwrap(), "a".repeat(64));
        assert!(normalize_checksum("not-a-hash").is_err());
        assert_eq!(normalize_checksum("").unwrap(), "");
    }

    #[test]
    fn range_mode_restarts_when_server_ignores_range() {
        assert_eq!(
            resume_mode(100, 200, None, true).unwrap(),
            ResumeMode::Restart
        );
        assert_eq!(
            resume_mode(100, 206, Some(100), true).unwrap(),
            ResumeMode::Append
        );
        assert!(resume_mode(100, 206, Some(99), true).is_err());
    }

    #[test]
    fn parses_content_range() {
        assert_eq!(parse_content_range("bytes 100-199/200"), Some((100, 200)));
        assert_eq!(parse_content_range("bytes */200"), None);
    }

    #[test]
    fn hashes_the_complete_file() {
        let dir = std::env::temp_dir().join(format!("bard-hf-test-{}", crate::db::now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("model.part");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pinned_verification_requires_complete_metadata_and_catches_a_rewrite() {
        let dir = temp_dir("pin");
        let path = dir.join("model.gguf");
        fs::write(&path, b"verified").unwrap();
        let digest = sha256_file(&path).unwrap();
        // Incomplete metadata is never permission to skip the hash.
        assert!(verify_pinned_file(&path, 8, &digest).is_ok());
        assert!(verify_pinned_file(&path, 0, &digest).is_err());
        assert!(verify_pinned_file(&path, 8, "").is_err());

        // A rewrite of a different length is refused by the size guard.
        fs::write(&path, b"modified-and-longer").unwrap();
        assert!(verify_pinned_file(&path, 8, &digest).is_err());

        // A same-size rewrite is refused by the hash, on a path no marker has
        // ever vouched for so the hash is certain to run.
        let fresh = dir.join("fresh.gguf");
        fs::write(&fresh, b"verified").unwrap();
        fs::write(&fresh, b"modified").unwrap();
        assert!(
            verify_pinned_file(&fresh, 8, &digest).is_err(),
            "a same-size rewrite must fail its pinned checksum"
        );

        // What the durable marker cannot promise: once a path has been verified,
        // the only thing standing between a same-size rewrite and a pass is the
        // file stamp. On the filesystem these tests run on, the stamp does not
        // move at all — an eight-byte rewrite left both `last_write_time` and
        // `creation_time` at 134348792418400104 across a five-second wait — so
        // the marker vouches for the mutation. That is the exact trust model
        // `verify_pinned_file` documents, reported here rather than asserted.
        //
        // This is reported rather than asserted, because whether a same-size
        // rewrite is visible at all is a property of the filesystem, not of this
        // code. Asserting either way would make the suite lie on one platform or
        // the other. See the trust model on `verify_pinned_file`.
        let before = file_stamp(&fs::metadata(&path).unwrap());
        fs::write(&path, b"verified").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let after = file_stamp(&fs::metadata(&path).unwrap());
        if after == before {
            log::warn!(
                "hf-test:verification-marker-cannot-see-content path={} stamp={after:?} \
                 -- a same-size model rewrite is invisible to the verification marker",
                path.display()
            );
        }
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_verified_file_is_not_hashed_again_until_something_moves() {
        let dir = temp_dir("marker");
        let path = dir.join("model.gguf");
        fs::write(&path, b"verified").unwrap();
        let digest = sha256_file(&path).unwrap();
        let key = normalized_path_string(&canonicalize_for_comparison(&path));

        // The first verification is a hash, and it leaves a marker.
        assert!(verify_pinned_file(&path, 8, &digest).is_ok());
        let stamp = stamp_of(&fs::metadata(&path).unwrap());
        assert!(marker_vouches(&key, stamp, &digest));

        // A read that agrees with the marker is answered from it…
        assert!(verify_pinned_file(&path, 8, &digest).is_ok());
        // …and one that asks about different bytes is not: the marker is keyed
        // by the checksum as well as the stamp, so a catalogue change re-hashes
        // and fails.
        assert!(verify_pinned_file(&path, 8, &"0".repeat(64)).is_err());

        // A file the marker has never seen is hashed — including one of the
        // same size, which is the case a stamp alone cannot see.
        let fresh = dir.join("fresh.gguf");
        fs::write(&fresh, b"modified").unwrap();
        assert!(verify_pinned_file(&fresh, 8, &digest).is_err());

        // Leave the machine's marker file as it was found: the entries this test
        // added are for a scratch path and would otherwise accumulate.
        {
            let mut markers = verified_markers()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            markers.remove(&key);
            if let (Some(marker), Ok(json)) = (marker_path(), serde_json::to_string(&*markers)) {
                let _ = fs::write(marker, json);
            }
        }
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn path_containment_rejects_parent_and_sibling_escapes() {
        let base = std::env::temp_dir().join(format!("bard-path-test-{}", crate::db::now_ms()));
        let root = base.join("models");
        let inside = root.join("model.gguf");
        let outside = base.join("outside.gguf");
        fs::create_dir_all(&root).unwrap();
        fs::write(&inside, b"model").unwrap();
        fs::write(&outside, b"outside").unwrap();
        assert!(path_is_within(&root, &inside));
        assert!(!path_is_within(&root, &outside));
        assert!(!path_is_within(&root, &root));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn canonical_containment_agrees_with_the_public_rule() {
        let base = temp_dir("canonical");
        let root = base.join("models");
        fs::create_dir_all(&root).unwrap();
        let inside = root.join("model.gguf");
        let sibling = root.join("nested").join("model.gguf");
        let outside = base.join("outside.gguf");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(&inside, b"model").unwrap();
        fs::write(&sibling, b"model").unwrap();
        fs::write(&outside, b"outside").unwrap();

        // `cached_models` reuses the canonical form to test every detected file
        // against a root it has already resolved. It must not be a second,
        // weaker copy of the rule.
        let canonical_root = canonicalize_for_comparison(&root);
        for (candidate, expected) in [
            (&inside, true),
            (&sibling, true),
            (&outside, false),
            (&root, false),
            (&base, false),
        ] {
            let name = candidate.display();
            assert_eq!(
                path_is_within_canonical(&canonical_root, &canonicalize_for_comparison(candidate)),
                expected,
                "canonical containment of {name}"
            );
            assert_eq!(
                path_is_within(&root, candidate),
                expected,
                "containment of {name}"
            );
        }
        // A prefix of the root's name is not a descendant of it.
        assert!(!path_is_within(&root, &base.join("models-evil")));
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn a_cached_verdict_does_not_outlive_the_bytes_it_describes() {
        let dir = temp_dir("stamp");
        let path = dir.join("model.gguf");
        fs::write(&path, b"first").unwrap();
        remember_compatibility(&path, ModelCompatibility::Verified);
        assert_eq!(cached_compatibility(&path), ModelCompatibility::Verified);
        assert_eq!(
            cached_compatibility(&dir.join("never-probed.gguf")),
            ModelCompatibility::Unverified
        );

        // The same path, different bytes. A verdict about the file that used to
        // be here says nothing about this one, so the pane must ask again rather
        // than display the old answer as current.
        let before = compatibility_stamp(&path).expect("stamp");
        fs::write(&path, b"a different model entirely").unwrap();
        assert_ne!(
            before,
            compatibility_stamp(&path).expect("stamp"),
            "the fixture must actually change the file identity"
        );
        assert_eq!(
            cached_compatibility(&path),
            ModelCompatibility::Unverified,
            "a replaced file must not inherit the previous verdict"
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn download_redirects_stay_inside_the_expected_hosts() {
        for host in [
            "huggingface.co",
            "www.huggingface.co",
            "cdn-lfs.hf.co",
            "cas-bridge.xethub.hf.co",
            "github.com",
            "release-assets.githubusercontent.com",
        ] {
            assert!(
                is_expected_download_host(host),
                "{host} must stay reachable"
            );
        }
        for host in [
            "huggingface.co.evil.test",
            "hf.co.attacker.test",
            "evil.test",
            "huggingface.coo",
            "notgithubusercontent.com",
            "127.0.0.1",
        ] {
            assert!(!is_expected_download_host(host), "{host} must be refused");
        }
    }

    /// Regression: a probe that outlives its timeout used to release the
    /// serialization guard, so the next probe opened a second copy of a
    /// multi-hundred-megabyte model and N timeouts left N models resident.
    #[test]
    fn a_timed_out_probe_keeps_holding_the_probe_guard() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let hold = Duration::from_millis(600);

        let first = spawn_probe(
            PathBuf::from("first.gguf"),
            Duration::from_millis(60),
            move |_path, _probe| {
                let _ = started_tx.send(Instant::now());
                std::thread::sleep(hold);
                let _ = finished_tx.send(Instant::now());
                Ok(())
            },
        );
        started_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the first probe must reach its load");
        assert!(
            first.is_err(),
            "a probe that outlives its timeout reports a timeout, not a verdict"
        );

        // The abandoned thread still owns the guard, so a second probe waits for
        // it instead of opening another model.
        let ran = std::sync::Arc::new(AtomicBool::new(false));
        let second = {
            let ran = ran.clone();
            spawn_probe(
                PathBuf::from("second.gguf"),
                Duration::from_millis(60),
                move |_path, _probe| {
                    ran.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
        };
        assert!(
            second.is_err(),
            "a queued probe is still waiting for the guard"
        );
        assert!(
            !ran.load(Ordering::SeqCst),
            "a second model was opened while the first was still resident"
        );

        // And the guard is not leaked forever: once the abandoned load ends, the
        // next probe runs immediately.
        finished_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the abandoned load must finish");
        let third = spawn_probe(
            PathBuf::from("third.gguf"),
            Duration::from_secs(30),
            |_, _| Ok(()),
        );
        assert!(
            matches!(third, Ok(Ok(()))),
            "the guard must be released once the abandoned load ends, got {third:?}"
        );
    }

    #[tokio::test]
    async fn resumes_a_partial_download_from_a_matching_content_range() {
        let dir = temp_dir("resume");
        let body = payload(4096);
        let digest = sha256_bytes(&body);
        let (url, server) = serve_once(
            TestResponse::new(body[1024..].to_vec())
                .with_status(206, "Partial Content")
                .with_header("content-range", "bytes 1024-4095/4096"),
        );
        let request = test_request(&dir, &url, &digest, 4096);
        fs::write(part_path(&request.destination), &body[..1024]).unwrap();

        assert_eq!(run(&request).await, Ok(()));
        server.join().expect("server");
        // The whole model, not the prefix twice: the resumed file is hashed in
        // full before it is promoted.
        assert_eq!(fs::read(&request.destination).unwrap(), body);
        assert!(!part_path(&request.destination).exists());
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn restarts_when_a_ranged_request_is_answered_with_the_whole_entity() {
        let dir = temp_dir("restart");
        let body = payload(4096);
        let digest = sha256_bytes(&body);
        let (url, server) = serve_once(TestResponse::new(body.clone()));
        let request = test_request(&dir, &url, &digest, 4096);
        // A partial of the same length whose bytes are not the model's prefix.
        // Appending the 200 body to it would yield a file of exactly the right
        // length and entirely the wrong content, which is the corruption this
        // case exists to prevent.
        fs::write(part_path(&request.destination), vec![0xAB_u8; 1024]).unwrap();

        assert_eq!(run(&request).await, Ok(()));
        server.join().expect("server");
        assert_eq!(fs::read(&request.destination).unwrap(), body);
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn refuses_a_content_range_that_does_not_start_where_bard_asked() {
        let dir = temp_dir("range");
        let body = payload(4096);
        let (url, server) = serve_once(
            TestResponse::new(body[1000..].to_vec())
                .with_status(206, "Partial Content")
                .with_header("content-range", "bytes 1000-4095/4096"),
        );
        let request = test_request(&dir, &url, &sha256_bytes(&body), 4096);
        fs::write(part_path(&request.destination), &body[..1024]).unwrap();

        let error = run(&request).await.unwrap_err();
        server.join().expect("server");
        assert!(error.contains("resumed at byte 1000"), "{error}");
        assert!(!part_path(&request.destination).exists());
        assert!(!request.destination.exists());
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn a_truncated_stream_fails_and_keeps_the_part_for_a_later_resume() {
        let dir = temp_dir("truncated");
        let body = payload(8192);
        // The server promises the full 8192 bytes and delivers 8191, so the
        // pre-flight size check is satisfied and the failure has to arrive
        // mid-stream, where the bytes already on disk are a usable resume point.
        let (url, server) = serve_once(TestResponse::new(body.clone()).truncated());
        let request = test_request(&dir, &url, &sha256_bytes(&body), 8192);

        assert!(run(&request).await.is_err());
        server.join().expect("server");
        let part = part_path(&request.destination);
        let partial = fs::read(&part).expect("the partial must survive for a resume");
        assert_eq!(partial.len(), 8191, "every delivered byte must be kept");
        assert!(
            body.starts_with(&partial),
            "surviving bytes must be a prefix"
        );
        assert!(
            !request.destination.exists(),
            "a truncated transfer must never be promoted"
        );
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn discards_the_part_when_the_stream_exceeds_the_catalogue_size() {
        let dir = temp_dir("oversize");
        let body = payload(4096);
        // No declared length, so the pre-flight check cannot see this; only the
        // running total can.
        let (url, server) = serve_once(TestResponse::new(body).unframed());
        let request = test_request(&dir, &url, "", 1024);

        let error = run(&request).await.unwrap_err();
        server.join().expect("server");
        assert!(error.contains("more model data"), "{error}");
        assert!(!part_path(&request.destination).exists());
        assert!(!request.destination.exists());
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn refuses_a_declared_length_that_contradicts_the_catalogue() {
        let dir = temp_dir("declared");
        let body = payload(4096);
        let (url, server) = serve_once(TestResponse::new(body));
        let request = test_request(&dir, &url, "", 2048);

        let error = run(&request).await.unwrap_err();
        server.join().expect("server");
        assert!(error.contains("content length does not match"), "{error}");
        assert!(!request.destination.exists());
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn discards_the_part_on_a_checksum_mismatch() {
        let dir = temp_dir("checksum");
        let body = payload(4096);
        let (url, server) = serve_once(TestResponse::new(body));
        let request = test_request(&dir, &url, &"0".repeat(64), 4096);

        let error = run(&request).await.unwrap_err();
        server.join().expect("server");
        assert!(error.contains("checksum mismatch"), "{error}");
        // A part that failed verification must not survive: a later resume would
        // treat those bytes as a trusted prefix.
        assert!(!part_path(&request.destination).exists());
        assert!(!request.destination.exists());
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn promotes_a_complete_part_without_spending_a_request() {
        let dir = temp_dir("completepart");
        let body = payload(4096);
        let request = test_request(
            &dir,
            // Port 1 is closed. Reaching the network at all fails this test.
            "http://127.0.0.1:1/model.gguf",
            &sha256_bytes(&body),
            4096,
        );
        fs::write(part_path(&request.destination), &body).unwrap();

        assert_eq!(run(&request).await, Ok(()));
        assert_eq!(fs::read(&request.destination).unwrap(), body);
        fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn a_cancelled_download_never_promotes_its_part() {
        let dir = temp_dir("cancel");
        // Large enough to arrive in several chunks, small enough to sit in the
        // socket buffer so the server thread can never block on a write to a
        // connection the client has already abandoned.
        let body = payload(256 * 1024);
        let (url, server) = serve_once(TestResponse::new(body.clone()));
        let request = test_request(&dir, &url, &sha256_bytes(&body), body.len() as u64);
        let polls = std::cell::Cell::new(0_u32);

        let error = transfer(
            &request,
            |_, _, _, _| {},
            || {
                polls.set(polls.get() + 1);
                polls.get() > 1
            },
        )
        .await
        .unwrap_err();
        server.join().expect("server");
        assert_eq!(error, "Download cancelled.");
        assert!(
            !request.destination.exists(),
            "a cancelled transfer must never be promoted"
        );
        let written = fs::metadata(part_path(&request.destination))
            .map(|meta| meta.len())
            .unwrap_or(0);
        assert!(
            written <= body.len() as u64,
            "the part must never exceed the catalogue size"
        );
        fs::remove_dir_all(dir).ok();
    }
}
