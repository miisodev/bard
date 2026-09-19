use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HfFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CachedHfModel {
    pub repo: String,
    pub file: String,
    pub size: u64,
    pub local_path: String,
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
    let lower = name.to_lowercase();
    lower.ends_with(".gguf") || (lower.starts_with("ggml-") && lower.ends_with(".bin"))
}

pub fn hf_cache_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(home)
        .join(".cache")
        .join("huggingface")
        .join("hub")
}

pub fn local_models_dir(app: &AppHandle) -> PathBuf {
    let data = app.path().app_data_dir().unwrap();
    let dir = data.join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn cached_models() -> Vec<CachedHfModel> {
    let root = hf_cache_dir();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    for repo_dir in entries.flatten() {
        if !repo_dir.path().is_dir() {
            continue;
        }
        let name = repo_dir.file_name().to_string_lossy().to_string();
        let Some(repo) = name.strip_prefix("models--").map(|s| s.replace("--", "/")) else {
            continue;
        };
        let snapshots = repo_dir.path().join("snapshots");
        let Ok(snap_entries) = std::fs::read_dir(&snapshots) else {
            continue;
        };
        for rev in snap_entries.flatten() {
            if !rev.path().is_dir() {
                continue;
            }
            let Ok(file_entries) = std::fs::read_dir(rev.path()) else {
                continue;
            };
            for file in file_entries.flatten() {
                let fname = file.file_name().to_string_lossy().to_string();
                if !is_transcript_file(&fname) {
                    continue;
                }
                if let Ok(meta) = file.metadata() {
                    out.push(CachedHfModel {
                        repo: repo.clone(),
                        file: fname,
                        size: meta.len(),
                        local_path: file.path().to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    out
}

pub async fn list_repo_files(repo: &str) -> Result<Vec<HfFile>, String> {
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 401 {
        return Err("Repo not found or access gated.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Listing failed: {}", resp.status()));
    }
    let items: Vec<TreeItem> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(items
        .into_iter()
        .filter(|i| i.kind == "file" && is_transcript_file(&i.path))
        .filter_map(|i| {
            let sha = i.lfs.map(|l| l.oid).unwrap_or_default();
            if sha.is_empty() {
                None
            } else {
                Some(HfFile { path: i.path, size: i.size, sha256: sha })
            }
        })
        .collect())
}

pub fn verify_load(path: &str) -> bool {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = transcribe_cpp::init_backends_default();
    });
    let p = path.to_string();
    std::thread::spawn(move || transcribe_cpp::Model::load(&p).is_ok())
        .join()
        .unwrap_or(false)
}

pub fn custom_storage_name(repo: &str, file: &str) -> String {
    format!("hf-{repo}-{file}")
        .replace('/', "--")
        .replace(['\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

pub async fn install_custom_model(
    app: &AppHandle,
    repo: &str,
    file: &str,
    size: u64,
    sha256: &str,
) -> Result<(), String> {
    let id = format!("hf|{repo}|{file}");
    if let Some(m) = crate::db::custom_model_by_id(app, &id) {
        crate::db::set_setting(app, "model_id", &m.id);
        let _ = app.emit("model-ready", ());
        return Ok(());
    }

    let dir = local_models_dir(app);
    let fname = custom_storage_name(repo, file);
    let final_path = dir.join(&fname);
    if final_path.exists() {
        crate::db::register_custom_model(app, &id, repo, file, size, &final_path.to_string_lossy());
        crate::db::set_setting(app, "model_id", &id);
        let _ = app.emit("model-ready", ());
        return Ok(());
    }

    crate::dictation::reset_cancel();
    let url = format!("https://huggingface.co/{repo}/resolve/main/{file}");
    let part = final_path.with_extension("part");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(0))
        .build()
        .map_err(|e| e.to_string())?;

    let mut received = part.metadata().map(|m| m.len()).unwrap_or(0);
    let total = size;

    let mut req = client.get(&url);
    if received > 0 {
        req = req.header("Range", format!("bytes={received}-"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        return Err(format!("Download failed: {}", resp.status()));
    }

    let mut file_out = std::fs::File::options()
        .append(true)
        .open(&part)
        .map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut last_emit = Instant::now();
    let mut last_progress: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if crate::dictation::is_cancel_pending() {
            drop(file_out);
            return Err("Download cancelled.".into());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file_out.write_all(&chunk).map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        received += chunk.len() as u64;
        if received - last_progress > 1024 * 1024 || last_emit.elapsed() > Duration::from_millis(120)
        {
            last_progress = received;
            last_emit = Instant::now();
            let _ = app.emit(
                "model-progress",
                serde_json::json!({
                    "id": id,
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
    file_out.flush().map_err(|e| e.to_string())?;
    drop(file_out);

    if crate::dictation::is_cancel_pending() {
        return Err("Download cancelled.".into());
    }

    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    if !sha256.is_empty() && hex != sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("Model verification failed (checksum mismatch).".into());
    }
    std::fs::rename(&part, &final_path).map_err(|e| e.to_string())?;
    if !verify_load(&final_path.to_string_lossy()) {
        let _ = std::fs::remove_file(&final_path);
        return Err(
            "Unsupported model architecture — Bard's engine runs whisper-family GGUFs only.".into(),
        );
    }
    crate::db::register_custom_model(app, &id, repo, file, size, &final_path.to_string_lossy());
    crate::db::set_setting(app, "model_id", &id);
    let _ = app.emit("model-ready", ());
    Ok(())
}

pub fn register_cached(app: &AppHandle, repo: &str, file: &str, local_path: &str, size: u64) -> Result<(), String> {
    if !verify_load(local_path) {
        return Err(
            "Unsupported model architecture — Bard's engine runs whisper-family GGUFs only.".into(),
        );
    }
    let id = format!("hf|{repo}|{file}");
    crate::db::register_custom_model(app, &id, repo, file, size, local_path);
    crate::db::set_setting(app, "model_id", &id);
    let _ = app.emit("model-ready", ());
    let _ = app.emit("refresh", ());
    Ok(())
}