use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

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

static SESSION: Mutex<Option<(String, Session, CancelToken)>> = Mutex::new(None);
static ACTIVE_CANCEL: Mutex<Option<CancelToken>> = Mutex::new(None);
static INIT_BACKENDS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CANCEL_DOWNLOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LAST_USE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static WATCHER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const UNLOAD_IDLE_MS: u64 = 5 * 60 * 1000;

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
    if let Some(token) = ACTIVE_CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
    {
        token.cancel();
    }
}

pub fn cancel_download() {
    CANCEL_DOWNLOAD.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn cancel_download_reset() {
    CANCEL_DOWNLOAD.store(false, std::sync::atomic::Ordering::SeqCst);
}

pub fn cancel_pending() -> bool {
    CANCEL_DOWNLOAD.load(std::sync::atomic::Ordering::SeqCst)
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

    // Installation is not selection. The UI receives a refresh signal and the
    // user activates this model explicitly with `set_model`.
    crate::hf::emit_model_installed(app, def.id);
    Ok(())
}

pub fn delete_model(app: &AppHandle, id: &str) -> Result<(), String> {
    crate::hf::delete_model(app, id)?;
    // A non-active deletion cannot be the in-memory session, but clearing an
    // idle/stale session here preserves the old unload behavior for callers.
    *SESSION.lock().unwrap() = None;
    *ACTIVE_CANCEL.lock().unwrap() = None;
    Ok(())
}

pub fn start_unload_watcher(_app: AppHandle) {
    if WATCHER_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let last = LAST_USE.load(std::sync::atomic::Ordering::SeqCst);
        if last > 0 && crate::db::now_ms() - last as i64 > UNLOAD_IDLE_MS as i64 {
            *SESSION.lock().unwrap() = None;
            *ACTIVE_CANCEL.lock().unwrap() = None;
            LAST_USE.store(0, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

pub fn transcribe(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    crate::crash::record_event("model:transcribe-start");
    let result = transcribe_inner(app, samples);
    crate::crash::record_event("model:transcribe-end");
    result
}

fn transcribe_inner(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    let Some(path) = resolve_path(app, &id) else {
        return Err("No voice model downloaded.".into());
    };
    if !path.is_file() {
        return Err("Voice model not downloaded.".into());
    }
    crate::hf::verify_model_file(app, &id, &path)?;

    if !INIT_BACKENDS.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let _ = transcribe_cpp::init_backends_default();
    }

    let mut guard = SESSION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard
        .as_ref()
        .map(|(saved, _, _)| saved != &path.to_string_lossy().to_string())
        .unwrap_or(true)
    {
        let model = Model::load(&path).map_err(|e| e.to_string())?;
        let mut session = model.session().map_err(|e| e.to_string())?;
        let token = CancelToken::new();
        session.set_cancel_token(&token);
        *guard = Some((path.to_string_lossy().to_string(), session, token));
    }
    let Some((_, session, token)) = guard.as_mut() else {
        return Err("Voice model session could not be created.".into());
    };
    token.reset();
    *ACTIVE_CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token.clone());
    let result = session
        .run(samples, &RunOptions::default())
        .map_err(|e| e.to_string());
    *ACTIVE_CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let result = result?;
    LAST_USE.store(
        crate::db::now_ms() as u64,
        std::sync::atomic::Ordering::SeqCst,
    );
    Ok(result.text.trim().to_string())
}
