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

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HfFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub source_repo: String,
    pub source_revision: String,
    #[serde(default)]
    pub quant: Option<String>,
    pub compatibility: ModelCompatibility,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CachedHfModel {
    pub id: String,
    pub repo: String,
    pub source_revision: String,
    pub file: String,
    pub size: u64,
    pub local_path: String,
    pub quant: Option<String>,
    pub compatibility: ModelCompatibility,
    pub registered: bool,
    /// Cache files are shared external state and are always read-only to Bard.
    pub owned: bool,
}

#[derive(Deserialize)]
struct TreeItem {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    size: u64,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    oid: String,
}

fn is_transcript_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".gguf") || (lower.starts_with("ggml-") && lower.ends_with(".bin"))
}

/// Pull a quantization label out of common GGUF/GGML filenames. This is display
/// metadata only; the engine remains the authority on whether a file loads.
pub fn architecture_from_filename(name: &str) -> crate::catalog::Architecture {
    let lower = name.to_ascii_lowercase();
    if lower.contains("parakeet") {
        crate::catalog::Architecture::Parakeet
    } else if lower.contains("qwen3") || lower.contains("qwen-3") {
        crate::catalog::Architecture::Qwen3Asr
    } else if lower.contains("canary") {
        crate::catalog::Architecture::Canary
    } else if lower.contains("moonshine") {
        crate::catalog::Architecture::Moonshine
    } else if lower.contains("sensevoice") {
        crate::catalog::Architecture::SenseVoice
    } else if lower.contains("gigaam") {
        crate::catalog::Architecture::GigaAm
    } else if lower.contains("cohere") {
        crate::catalog::Architecture::Cohere
    } else {
        crate::catalog::Architecture::Whisper
    }
}

pub fn quant_from_filename(name: &str) -> Option<String> {
    let stem = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_ascii_uppercase();
    const QUANTS: &[&str] = &[
        "IQ4_XS", "Q4_K_M", "Q4_K_S", "Q5_K_M", "Q5_K_S", "Q6_K", "Q4_0", "Q4_1", "Q5_0", "Q5_1",
        "Q8_0", "BF16", "F16", "F32",
    ];
    QUANTS
        .iter()
        .find(|quant| {
            stem.contains(&format!("-{quant}"))
                || stem.ends_with(*quant)
                || stem.contains(&format!("_{quant}"))
        })
        .map(|quant| (*quant).to_string())
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

pub fn custom_model_id(repo: &str, file: &str) -> String {
    format!("hf|{repo}|{file}")
}

fn app_model_id(repo: &str, file: &str) -> String {
    format!("app|{repo}|{file}")
}

fn external_model_id(repo: &str, file: &str, path: &Path) -> String {
    let normalized = normalized_path_string(&canonicalize_for_comparison(path));
    let digest = Sha256::digest(normalized.as_bytes());
    format!("hf-cache|{repo}|{file}|{:x}", digest)
}

pub fn hf_cache_dir() -> PathBuf {
    if let Ok(path) = std::env::var("HF_HUB_CACHE") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(home) = std::env::var("HF_HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join("hub");
        }
    }
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(home)
        .join(".cache")
        .join("huggingface")
        .join("hub")
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

fn compatibility_cache() -> &'static Mutex<HashMap<PathBuf, ModelCompatibility>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, ModelCompatibility>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn path_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn cached_compatibility(path: &Path) -> ModelCompatibility {
    compatibility_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&path_key(path)).copied())
        .unwrap_or(ModelCompatibility::Unverified)
}

fn remember_compatibility(path: &Path, status: ModelCompatibility) {
    if let Ok(mut cache) = compatibility_cache().lock() {
        cache.insert(path_key(path), status);
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

/// True only for a descendant of `root`. Canonicalization defeats `..` and
/// symlink escapes; the string check adds Windows case-insensitive containment.
pub fn path_is_within(root: &Path, candidate: &Path) -> bool {
    if !root.is_absolute() || !candidate.is_absolute() {
        return false;
    }
    let root = canonicalize_for_comparison(root);
    let candidate = canonicalize_for_comparison(candidate);
    let root_text = normalized_path_string(&root);
    let candidate_text = normalized_path_string(&candidate);
    !candidate_text.is_empty()
        && candidate_text != root_text
        && candidate_text.starts_with(&(root_text.clone() + "/"))
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

fn is_catalog_filename(filename: &str) -> bool {
    crate::catalog::models()
        .iter()
        .any(|model| model.file.filename.eq_ignore_ascii_case(filename))
}

fn detected_path_id(path: &Path) -> String {
    format!(
        "local|{}",
        normalized_path_string(&canonicalize_for_comparison(path))
    )
}

fn custom_for_path(app: &AppHandle, path: &Path) -> Option<crate::db::CustomModel> {
    let canonical = canonicalize_for_comparison(path);
    crate::db::list_custom_models(app)
        .into_iter()
        .find(|model| canonicalize_for_comparison(Path::new(&model.local_path)) == canonical)
}

fn append_detected_file(
    app: &AppHandle,
    out: &mut Vec<CachedHfModel>,
    path: PathBuf,
    repo: String,
    source_revision: String,
    file_name: String,
    size: u64,
) {
    let custom = custom_for_path(app, &path);
    let id = custom
        .as_ref()
        .map(|model| model.id.clone())
        .unwrap_or_else(|| {
            if repo == "Bard imports" {
                detected_path_id(&path)
            } else if path_is_within(&hf_cache_dir(), &path) {
                external_model_id(&repo, &file_name, &path)
            } else {
                custom_model_id(&repo, &file_name)
            }
        });
    let registered = custom.is_some();
    let compatibility = custom
        .as_ref()
        .map(|model| model.compatibility)
        .unwrap_or_else(|| cached_compatibility(&path));
    out.push(CachedHfModel {
        id,
        repo,
        source_revision,
        quant: quant_from_filename(&file_name),
        file: file_name,
        size,
        local_path: path.to_string_lossy().to_string(),
        compatibility,
        registered,
        owned: custom.as_ref().is_some_and(|model| model.owned()),
    });
}

pub fn cached_models(app: &AppHandle) -> Vec<CachedHfModel> {
    let root = hf_cache_dir();
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for repo_dir in entries.flatten() {
            if !repo_dir.path().is_dir() {
                continue;
            }
            let name = repo_dir.file_name().to_string_lossy().to_string();
            let Some(repo) = name.strip_prefix("models--").map(|s| s.replace("--", "/")) else {
                continue;
            };
            let snapshots = repo_dir.path().join("snapshots");
            let Ok(snap_entries) = fs::read_dir(&snapshots) else {
                continue;
            };
            for rev in snap_entries.flatten() {
                if !rev.path().is_dir() {
                    continue;
                }
                let source_revision = rev.file_name().to_string_lossy().to_string();
                let Ok(file_entries) = fs::read_dir(rev.path()) else {
                    continue;
                };
                for file in file_entries.flatten() {
                    let file_name = file.file_name().to_string_lossy().to_string();
                    if !is_transcript_file(&file_name) {
                        continue;
                    }
                    // fs::metadata follows symlinks; DirEntry::metadata does not.
                    // A Hugging Face snapshot entry is a link into blobs/, so the
                    // latter measures the link and every model reports as 0 bytes.
                    let path = file.path();
                    let Ok(meta) = fs::metadata(&path) else {
                        continue;
                    };
                    append_detected_file(
                        app,
                        &mut out,
                        path,
                        repo.clone(),
                        source_revision.clone(),
                        file_name,
                        meta.len(),
                    );
                }
            }
        }
    }

    // Bard-managed imports live in the app data directory. Catalog assets are
    // deliberately omitted because the first group already owns those rows.
    let managed_root = local_models_dir(app);
    if let Ok(entries) = fs::read_dir(&managed_root) {
        for file in entries.flatten() {
            if !file.path().is_file() {
                continue;
            }
            let file_name = file.file_name().to_string_lossy().to_string();
            if !is_transcript_file(&file_name) || is_catalog_filename(&file_name) {
                continue;
            }
            let path = file.path();
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            append_detected_file(
                app,
                &mut out,
                path,
                "Bard imports".into(),
                "local".into(),
                file_name,
                meta.len(),
            );
        }
    }

    // Keep a row for a registered file that has disappeared. The UI can then
    // explain the missing state and offer Forget/Remove instead of silently
    // making the registration vanish from the inventory.
    for custom in crate::db::list_custom_models(app) {
        let path = PathBuf::from(&custom.local_path);
        if path.exists() {
            continue;
        }
        let owned = custom.owned();
        out.push(CachedHfModel {
            id: custom.id,
            repo: custom.repo,
            source_revision: "local".into(),
            quant: crate::hf::quant_from_filename(&custom.file),
            file: custom.file,
            size: custom.size_bytes,
            local_path: custom.local_path,
            compatibility: ModelCompatibility::Missing,
            registered: true,
            owned,
        });
    }

    out.sort_by(|a, b| (&a.repo, &a.file, &a.local_path).cmp(&(&b.repo, &b.file, &b.local_path)));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

fn hf_tree_url(repo: &str) -> String {
    format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=true&expand=true")
}

fn hf_resolve_url(repo: &str, file: &str) -> String {
    let mut url = reqwest::Url::parse("https://huggingface.co").expect("static HF URL is valid");
    {
        let mut segments = url.path_segments_mut().expect("HF URL has a path");
        segments.pop_if_empty();
        for part in repo.split('/') {
            segments.push(part);
        }
        segments.push("resolve").push("main");
        for part in file.split('/') {
            segments.push(part);
        }
    }
    url.to_string()
}

pub async fn list_repo_files(repo: &str) -> Result<Vec<HfFile>, String> {
    let repo = normalize_repo_id(repo)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(hf_tree_url(&repo))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 401 {
        return Err("Repo not found or access gated.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Listing failed: {}", resp.status()));
    }
    let items: Vec<TreeItem> = resp.json().await.map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for item in items {
        if item.kind != "file" || !is_transcript_file(&item.path) {
            continue;
        }
        let Some(lfs) = item.lfs else {
            continue;
        };
        let Ok(sha256) = normalize_checksum(&lfs.oid) else {
            continue;
        };
        if sha256.is_empty() {
            continue;
        }
        let quant = quant_from_filename(&item.path);
        files.push(HfFile {
            path: item.path,
            size: item.size,
            sha256,
            source_repo: repo.clone(),
            source_revision: "main".into(),
            quant,
            compatibility: ModelCompatibility::Unverified,
        });
    }
    Ok(files)
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

fn probe_model_path(path: &Path) -> Result<CompatibilityResult, String> {
    let metadata =
        fs::metadata(path).map_err(|_| format!("Model file does not exist: {}", path.display()))?;
    if !metadata.is_file() {
        return Err("Model path is not a regular model file.".into());
    }
    // The settings surface may request several checks at once. Serializing
    // probes prevents multiple multi-hundred-MB model opens from racing.
    let _probe = model_probe_guard();
    init_engines_once();
    let probe_path = path.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = match transcribe_cpp::Model::load(&probe_path) {
            Ok(_) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        let _ = sender.send(outcome);
    });
    let outcome = receiver
        .recv_timeout(Duration::from_secs(120))
        .map_err(|_| "The model compatibility probe timed out after 120 seconds.".to_string())?;

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

pub fn verify_load(path: &str) -> bool {
    probe_model_path(Path::new(path))
        .map(|result| result.status == ModelCompatibility::Verified)
        .unwrap_or(false)
}

type PinnedFileStamp = (u64, u128, u128, u64);
type PinnedFileCacheValue = (PinnedFileStamp, String);

fn pinned_file_cache() -> &'static Mutex<HashMap<PathBuf, PinnedFileCacheValue>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, PinnedFileCacheValue>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

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
    let key = path_key(path);
    let stamp = file_stamp(&metadata);
    if pinned_file_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
        .is_some_and(|cached| {
            (cached.0).0 == stamp.0
                && (cached.0).1 == stamp.1
                && (cached.0).2 == stamp.2
                && (cached.0).3 == stamp.3
                && cached.1 == expected_sha256
        })
    {
        return Ok(());
    }
    let actual =
        sha256_file(path).map_err(|error| format!("Could not hash model file: {error}"))?;
    if !actual.eq_ignore_ascii_case(&expected_sha256) {
        return Err("Model checksum does not match the verified catalogue value.".into());
    }
    if let Ok(mut cache) = pinned_file_cache().lock() {
        cache.insert(key, (stamp, expected_sha256));
    }
    Ok(())
}

pub fn probe_local_model(app: &AppHandle, path_arg: &str) -> Result<CompatibilityResult, String> {
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
    let result = probe_model_path(&path)?;
    crate::db::set_custom_model_compatibility(app, path_arg, result.status);
    crate::db::set_custom_model_compatibility(app, &path.to_string_lossy(), result.status);
    let _ = app.emit("refresh", ());
    Ok(result)
}

fn model_path_for_id(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if let Some(model) = crate::catalog::find(id) {
        return Ok(local_models_dir(app).join(&model.file.filename));
    }
    crate::db::custom_model_by_id(app, id)
        .map(|model| PathBuf::from(model.local_path))
        .ok_or_else(|| format!("Unknown model: {id}"))
}

/// Re-check the file identity immediately before activation or inference. This
/// closes the gap where a previously installed path was replaced after the
/// catalogue/custom row was written.
pub fn verify_model_file(app: &AppHandle, id: &str, path: &Path) -> Result<(), String> {
    if let Some(model) = crate::catalog::find(id) {
        if !path_is_within(&local_models_dir(app), path) {
            return Err("The selected catalogue model is outside Bard's model directory.".into());
        }
        return verify_pinned_file(path, model.file.size_bytes, &model.file.sha256);
    }

    let custom =
        crate::db::custom_model_by_id(app, id).ok_or_else(|| format!("Unknown model: {id}"))?;
    let link_metadata = fs::symlink_metadata(path)
        .map_err(|_| "The selected model file is no longer available.".to_string())?;
    let target_metadata = fs::metadata(path)
        .map_err(|_| "The selected model file is no longer available.".to_string())?;
    if !target_metadata.is_file() {
        return Err("The selected model path is not a regular file.".into());
    }
    let canonical = fs::canonicalize(path)
        .map_err(|_| "The selected model file is no longer available.".to_string())?;
    if link_metadata.file_type().is_symlink() && !path_is_within(&hf_cache_dir(), &canonical) {
        return Err("Bard-managed model paths may not be symbolic links.".into());
    }
    let recorded = fs::canonicalize(&custom.local_path)
        .map_err(|_| "The selected model registration is stale.".to_string())?;
    if normalized_path_string(&canonical) != normalized_path_string(&recorded) {
        return Err("The selected model path changed after registration; rescan it first.".into());
    }
    if custom.owned() || !custom.sha256.is_empty() {
        verify_pinned_file(&canonical, custom.size_bytes, &custom.sha256)?;
    }
    Ok(())
}

/// Verify first, persist second. No caller receives a successful activation
/// unless the selected path exists and the current engine opened it.
pub fn activate_model(app: &AppHandle, id: &str) -> Result<(), String> {
    let _operation = model_operation_guard();
    let path = model_path_for_id(app, id)?;
    verify_model_file(app, id, &path)?;
    let probe = probe_model_path(&path)?;
    if probe.status != ModelCompatibility::Verified {
        return Err(probe
            .message
            .unwrap_or_else(|| "This model is not compatible with Bard's engine.".into()));
    }
    crate::db::set_setting_checked(app, "model_id", id)?;
    let _ = app.emit("model-activated", serde_json::json!({"id": id}));
    let _ = app.emit("refresh", ());
    Ok(())
}

pub fn custom_storage_name(repo: &str, file: &str) -> String {
    format!("hf-{repo}-{file}")
        .replace('/', "--")
        .replace(['\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

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
    let _ = app.emit("refresh", ());
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
            emit_progress(app, &request.id, received, received, None, None);
            return Ok(());
        }
        fs::remove_file(&part).map_err(|e| e.to_string())?;
        received = 0;
    }

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60 * 60))
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
        if crate::dictation::is_cancel_pending() {
            drop(file);
            return Err("Download cancelled.".into());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
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
            emit_progress(app, &request.id, received, total, speed, eta);
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if crate::dictation::is_cancel_pending() {
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

    // Always hash the complete .part, including any bytes carried over from an
    // earlier run. Hashing only newly received chunks cannot prove the resumed
    // file matches the pinned checksum.
    let digest = sha256_file(&part).map_err(|e| e.to_string())?;
    if !request.sha256.is_empty() && !digest.eq_ignore_ascii_case(&request.sha256) {
        let _ = fs::remove_file(&part);
        return Err("Model verification failed (checksum mismatch).".into());
    }
    fs::rename(&part, &request.destination).map_err(|e| e.to_string())?;
    emit_progress(
        app,
        &request.id,
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

pub async fn install_custom_model(
    app: &AppHandle,
    repo: &str,
    file: &str,
    size: u64,
    sha256: &str,
) -> Result<String, String> {
    let repo = normalize_repo_id(repo)?;
    let file = normalize_file_path(file)?;
    let sha256 = normalize_checksum(sha256)?;
    if size == 0 || sha256.len() != 64 {
        return Err("Hugging Face did not provide a complete size/checksum for this model; choose a file with checksum metadata.".into());
    }
    let id = app_model_id(&repo, &file);
    let destination = local_models_dir(app).join(custom_storage_name(&repo, &file));

    if destination.exists() {
        let probe = match verify_pinned_file(&destination, size, &sha256) {
            Ok(()) => probe_model_path(&destination),
            Err(error) => Err(error),
        };
        if let Ok(probe) = probe {
            if probe.status == ModelCompatibility::Verified {
                crate::db::register_custom_model(
                    app,
                    &id,
                    &repo,
                    &file,
                    size,
                    &sha256,
                    &destination.to_string_lossy(),
                    crate::db::ModelOwnership::App,
                    ModelCompatibility::Verified,
                )?;
                emit_model_installed(app, &id);
                return Ok(id);
            }
        }
        let _ = remove_owned_model_file(app, &destination);
    }

    crate::dictation::reset_cancel();
    let request = DownloadRequest {
        id: id.clone(),
        url: hf_resolve_url(&repo, &file),
        sha256: sha256.clone(),
        size_bytes: size,
        destination: destination.clone(),
    };
    download_resumable(app, request).await?;
    if let Err(error) = verify_pinned_file(&destination, size, &sha256) {
        let _ = remove_owned_model_file(app, &destination);
        emit_download_failure(app, &id, &error);
        return Err(error);
    }

    let probe = probe_model_path(&destination)?;
    if probe.status != ModelCompatibility::Verified {
        let _ = remove_owned_model_file(app, &destination);
        let error = probe
            .message
            .unwrap_or_else(|| "This model is incompatible with Bard's engine.".into());
        emit_download_failure(app, &id, &error);
        return Err(error);
    }
    crate::db::register_custom_model(
        app,
        &id,
        &repo,
        &file,
        size,
        &sha256,
        &destination.to_string_lossy(),
        crate::db::ModelOwnership::App,
        ModelCompatibility::Verified,
    )?;
    // Installation never changes `model_id`; the UI must activate explicitly.
    emit_model_installed(app, &id);
    Ok(id)
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
    let id = if in_app_models {
        app_model_id(&repo, &file)
    } else {
        external_model_id(&repo, &file, &path)
    };
    let ownership = if in_app_models {
        crate::db::ModelOwnership::App
    } else {
        crate::db::ModelOwnership::External
    };
    let measured_size = if size > 0 {
        size
    } else {
        fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0)
    };
    let registered_sha256 = if in_app_models {
        sha256_file(&path).map_err(|error| format!("Could not hash the local model: {error}"))?
    } else {
        String::new()
    };
    if in_app_models {
        verify_pinned_file(&path, measured_size, &registered_sha256)?;
    }
    crate::db::register_custom_model(
        app,
        &id,
        &repo,
        &file,
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

pub fn register_cached(
    app: &AppHandle,
    repo: &str,
    file: &str,
    local_path: &str,
    size: u64,
) -> Result<String, String> {
    register_detected_path(app, repo, file, local_path, size)
}

/// Register and activate a discovered cache model in one explicit user action.
/// The source file remains external and read-only to Bard.
pub fn activate_detected_model(app: &AppHandle, id: &str) -> Result<(), String> {
    let detected = cached_models(app)
        .into_iter()
        .find(|model| model.id == id)
        .ok_or_else(|| format!("Unknown detected model: {id}"))?;
    let registered_id = register_detected_path(
        app,
        &detected.repo,
        &detected.file,
        &detected.local_path,
        detected.size,
    )?;
    activate_model(app, &registered_id)
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

    let custom =
        crate::db::custom_model_by_id(app, id).ok_or_else(|| format!("Unknown model: {id}"))?;
    if custom.owned() {
        let path = Path::new(&custom.local_path);
        remove_owned_model_file(app, path)?;
        let _ = remove_owned_model_file(app, &part_path(path));
        let _ = remove_owned_model_file(app, &legacy_part_path(path));
    }
    crate::db::delete_custom_model(app, id).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn live_huggingface_listing_when_requested() {
        if std::env::var_os("BARD_TEST_HF_LIVE").is_none() {
            return;
        }
        let files = list_repo_files("handy-computer/whisper-small-gguf")
            .await
            .expect("live Hugging Face listing succeeds");
        assert!(files
            .iter()
            .any(|file| file.path == "whisper-small-Q6_K.gguf"));
        assert!(files
            .iter()
            .all(|file| file.compatibility == ModelCompatibility::Unverified));
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
    fn extracts_common_quant_labels() {
        assert_eq!(
            quant_from_filename("Qwen3-ASR-0.6B-Q8_0.gguf").as_deref(),
            Some("Q8_0")
        );
        assert_eq!(
            quant_from_filename("ggml-medium-q5_0.bin").as_deref(),
            Some("Q5_0")
        );
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
    fn pinned_verification_rejects_mutation_and_requires_metadata() {
        let dir = std::env::temp_dir().join(format!("bard-pin-test-{}", crate::db::now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("model.gguf");
        fs::write(&path, b"verified").unwrap();
        let digest = sha256_file(&path).unwrap();
        assert!(verify_pinned_file(&path, 8, &digest).is_ok());
        assert!(verify_pinned_file(&path, 0, &digest).is_err());
        fs::write(&path, b"modified").unwrap();
        assert!(verify_pinned_file(&path, 8, &digest).is_err());
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
}
