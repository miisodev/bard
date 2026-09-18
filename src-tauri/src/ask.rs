use tauri::AppHandle;

use crate::db;

const MAX_CONTEXT_ITEMS: usize = 8;
const MAX_CONTEXT_CHARS: usize = 8000;

pub async fn ask(app: &AppHandle, prompt: &str, context_item_ids: &[i64]) -> Result<String, String> {
    let base = db::get_setting(app, "provider_base_url")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.openai.com/v1".into());
    let api_key = db::get_setting(app, "provider_api_key").filter(|s| !s.is_empty());
    let model = db::get_setting(app, "provider_model").filter(|s| !s.is_empty());

    let (Some(api_key), Some(model)) = (api_key, model) else {
        return Err("No AI provider configured. Open Settings to connect one.".into());
    };

    let context = build_context(app, context_item_ids);

    let user_content = match context {
        Some(ctx) => format!("{prompt}\n\n---\nContext:\n{ctx}"),
        None => prompt.to_string(),
    };

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are Bard, a concise and precise assistant embedded in a desktop workspace. Answer directly, use markdown sparingly, no preamble."
            },
            { "role": "user", "content": user_content }
        ],
        "temperature": 0.4,
        "stream": false
    });

    let base = base.trim_end_matches('/');
    let url = if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let snippet: String = text.chars().take(300).collect();
        return Err(format!("Provider returned {status}: {snippet}"));
    }

    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let content = json
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Provider returned an unexpected response".to_string())?;

    Ok(content)
}

fn build_context(app: &AppHandle, ids: &[i64]) -> Option<String> {
    let all = db::list_items(app);
    let mut parts: Vec<String> = all
        .into_iter()
        .filter(|i| i.kind == "text" && (ids.is_empty() || ids.contains(&i.id)))
        .take(MAX_CONTEXT_ITEMS)
        .map(|i| i.content)
        .collect();
    if ids.is_empty() {
        parts.truncate(3);
    }
    if parts.is_empty() {
        return None;
    }
    let mut joined = parts.join("\n\n");
    if joined.chars().count() > MAX_CONTEXT_CHARS {
        joined = joined.chars().take(MAX_CONTEXT_CHARS).collect();
    }
    Some(joined)
}