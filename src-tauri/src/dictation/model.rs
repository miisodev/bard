use std::{
    fs,
    io::Write as _,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use transcribe_cpp::{Model, RunOptions, Session};

pub struct ModelDef {
    pub id: &'static str,
    pub name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_mb: u64,
}

pub const MODELS: &[ModelDef] = &[
    ModelDef {
        id: "whisper-small",
        name: "Whisper Small",
        filename: "ggml-small.bin",
        url: "https://blob.handy.computer/ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        size_mb: 465,
    },
    ModelDef {
        id: "whisper-medium",
        name: "Whisper Medium",
        filename: "whisper-medium-q4_1.bin",
        url: "https://blob.handy.computer/whisper-medium-q4_1.bin",
        sha256: "79283fc1f9fe12ca3248543fbd54b73292164d8df5a16e095e2bceeaaabddf57",
        size_mb: 469,
    },
    ModelDef {
        id: "whisper-turbo",
        name: "Whisper Turbo",
        filename: "ggml-large-v3-turbo.bin",
        url: "https://blob.handy.computer/ggml-large-v3-turbo.bin",
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        size_mb: 1549,
    },
];

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size_mb: u64,
    pub downloaded: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub size_mb: u64,
    pub downloaded: bool,
}

static SESSION: Mutex<Option<(String, Session)>> = Mutex::new(None);
static INIT_BACKENDS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CANCEL_DOWNLOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LAST_USE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static WATCHER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const UNLOAD_IDLE_MS: u64 = 5 * 60 * 1000;

fn models_dir(app: &AppHandle) -> PathBuf {
    let data = app.path().app_data_dir().unwrap();
    let dir = data.join("models");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn model_path(app: &AppHandle, def: &ModelDef) -> PathBuf {
    models_dir(app).join(def.filename)
}

fn find(id: &str) -> Option<&'static ModelDef> {
    MODELS.into_iter().find(|d| d.id == id)
}

fn resolve_path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    if let Some(def) = find(id) {
        return Some(model_path(app, def));
    }
    crate::db::custom_model_by_id(app, id).map(|c| PathBuf::from(c.local_path))
}

pub fn models(app: &AppHandle) -> Vec<ModelInfo> {
    let mut list: Vec<ModelInfo> = MODELS
        .into_iter()
        .map(|d| ModelInfo {
            id: d.id.to_string(),
            name: d.name.to_string(),
            size_mb: d.size_mb,
            downloaded: model_path(app, d).exists(),
        })
        .collect();
    for c in crate::db::list_custom_models(app) {
        list.push(ModelInfo {
            id: c.id.clone(),
            name: c.file.clone(),
            size_mb: c.size_mb,
            downloaded: PathBuf::from(&c.local_path).exists(),
        });
    }
    list
}

pub fn model_ready(app: &AppHandle) -> bool {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    resolve_path(app, &id).map(|p| p.exists()).unwrap_or(false)
}

pub fn model_status(app: &AppHandle) -> ModelStatus {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    if let Some(def) = find(&id) {
        return ModelStatus {
            id: def.id.to_string(),
            name: def.name.to_string(),
            size_mb: def.size_mb,
            downloaded: model_path(app, def).exists(),
        };
    }
    if let Some(c) = crate::db::custom_model_by_id(app, &id) {
        return ModelStatus {
            id: c.id,
            name: c.file,
            size_mb: c.size_mb,
            downloaded: PathBuf::from(&c.local_path).exists(),
        };
    }
    let d = &MODELS[0];
    ModelStatus {
        id: d.id.to_string(),
        name: d.name.to_string(),
        size_mb: d.size_mb,
        downloaded: model_path(app, d).exists(),
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
    let path = model_path(app, def);
    if path.exists() {
        crate::db::set_setting(app, "model_id", def.id);
        let _ = app.emit("model-ready", ());
        return Ok(());
    }
    CANCEL_DOWNLOAD.store(false, std::sync::atomic::Ordering::SeqCst);
    let part = path.with_extension("part");
    let mut received = part.metadata().map(|m| m.len()).unwrap_or(0);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(0))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(def.url);
    if received > 0 {
        req = req.header("Range", format!("bytes={received}-"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        return Err(format!("Download failed: {}", resp.status()));
    }
    let total = resp
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split('/').nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(def.size_mb * 1024 * 1024);

    let mut file = fs::File::options()
        .append(true)
        .open(&part)
        .map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut last_emit = Instant::now();
    let mut last_progress: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if CANCEL_DOWNLOAD.load(std::sync::atomic::Ordering::SeqCst) {
            drop(file);
            return Err("Download cancelled.".into());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        received += chunk.len() as u64;
        if received - last_progress > 1024 * 1024 || last_emit.elapsed() > Duration::from_millis(120)
        {
            last_progress = received;
            last_emit = Instant::now();
            let _ = app.emit(
                "model-progress",
                serde_json::json!({
                    "id": def.id,
                    "received": received,
                    "total": total,
                    "percent": if total > 0 {
                        ((received as f64 / total as f64) * 100.0) as u32
                    } else {
                        0
                    }
                }),
            );
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if CANCEL_DOWNLOAD.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Download cancelled.".into());
    }

    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    if hex != def.sha256 {
        let _ = fs::remove_file(&part);
        return Err("Model verification failed (checksum mismatch).".into());
    }
    fs::rename(&part, &path).map_err(|e| e.to_string())?;
    crate::db::set_setting(app, "model_id", def.id);
    let _ = app.emit(
        "model-progress",
        serde_json::json!({"id": def.id, "received": total, "total": total, "percent": 100}),
    );
    let _ = app.emit("model-ready", ());
    Ok(())
}

pub fn delete_model(app: &AppHandle, id: &str) {
    let Some(def) = find(id) else {
        return;
    };
    let _ = fs::remove_file(&model_path(app, def));
    *SESSION.lock().unwrap() = None;
    if crate::db::get_setting(app, "model_id") == Some(def.id.to_string()) {
        crate::db::set_setting(app, "model_id", "");
    }
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
            LAST_USE.store(0, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

pub fn transcribe(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    let id = crate::db::get_setting(app, "model_id").unwrap_or_default();
    let Some(path) = resolve_path(app, &id) else {
        return Err("No voice model downloaded.".into());
    };
    if !path.exists() {
        return Err("Voice model not downloaded.".into());
    }

    if !INIT_BACKENDS.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let _ = transcribe_cpp::init_backends_default();
    }

    let mut guard = SESSION.lock().unwrap();
    if guard
        .as_ref()
        .map(|(saved, _)| saved != &path.to_string_lossy().to_string())
        .unwrap_or(true)
    {
        let model = Model::load(&path).map_err(|e| e.to_string())?;
        let session = model.session().map_err(|e| e.to_string())?;
        *guard = Some((path.to_string_lossy().to_string(), session));
    }
    let (_, session) = guard.as_mut().unwrap();
    let result = session.run(samples, &RunOptions::default()).map_err(|e| e.to_string())?;
    LAST_USE.store(crate::db::now_ms() as u64, std::sync::atomic::Ordering::SeqCst);
    Ok(result.text.trim().to_string())
}