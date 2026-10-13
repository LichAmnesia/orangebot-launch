use std::time::SystemTime;

use ob_ipc::StyleInfo;
use ob_providers::{ClientConfig, OpenAiClient};

use crate::secrets;
use crate::settings::{self, Settings};

const NOTES_MAX_CHARS: usize = 2000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn info(settings: &Settings) -> StyleInfo {
    StyleInfo {
        enabled: settings.style_enabled,
        notes: settings.style_notes.clone(),
        learned_at_ms: settings.style_learned_at_ms,
    }
}

#[tauri::command]
pub fn get_style() -> Result<StyleInfo, String> {
    Ok(info(&settings::load()))
}

#[tauri::command]
pub fn save_style(style: StyleInfo) -> Result<(), String> {
    let mut settings = settings::load();
    settings.style_enabled = style.enabled;
    settings.style_notes = style.notes.trim().chars().take(NOTES_MAX_CHARS).collect();
    settings::save(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn learn_style() -> Result<StyleInfo, String> {
    let mut settings = settings::load();
    let store =
        ob_storage::Store::open(&ob_domain::paths::history_db()).map_err(|e| e.to_string())?;
    let sessions = store.list_sessions(60, None).map_err(|e| e.to_string())?;
    let samples: Vec<String> = sessions
        .into_iter()
        .filter(|record| {
            (record.mode == "dictate" || record.mode == "translate")
                && (record.delivery == "verified" || record.delivery == "attempted")
        })
        .map(|record| record.final_text)
        .collect();
    if samples.len() < 5 {
        return Err(
            "Dictate a few more times first — at least 5 accepted texts are needed.".to_string(),
        );
    }
    let text = settings
        .text
        .clone()
        .ok_or("Text model is not set up".to_string())?;
    let client = OpenAiClient::new(ClientConfig {
        base_url: text.base_url,
        model: text.model,
        api_key: secrets::get_key("text"),
        connect_timeout_ms: 4_000,
        total_timeout_ms: 120_000,
    })
    .map_err(|e| e.to_string())?;
    let out = client
        .chat_with(
            &ob_type::build_style_learn_messages(&samples),
            400,
            0.2,
            true,
        )
        .await
        .map_err(|e| e.to_string())?;
    settings.style_notes = ob_type::clean_model_output(&out);
    settings.style_learned_at_ms = Some(now_ms());
    settings.style_enabled = true;
    settings::save(&settings).map_err(|e| e.to_string())?;
    Ok(info(&settings))
}
