//! Result card commands.

use ob_providers::{ClientConfig, OpenAiClient};
use ob_type::{Mode, build_messages_with, clean_model_output};
use tauri::Manager;

use crate::secrets;
use crate::settings;

#[tauri::command]
pub fn result_insert(app: tauri::AppHandle, text: String) -> Result<String, String> {
    if let Some(window) = app.get_webview_window("result") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    {
        use std::time::Duration;

        use ob_platform::{Delivery, Platform};

        // Hiding the app returns focus to the previously active application.
        let _ = app.hide();
        std::thread::sleep(Duration::from_millis(250));
        let platform = ob_platform_macos::MacPlatform::new();
        match platform.insert_text(&text) {
            Ok(Delivery::Attempted) | Ok(Delivery::Verified) => Ok("inserted".to_string()),
            _ => {
                let _ = platform.copy_to_clipboard(&text);
                Ok("copied".to_string())
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        Ok("copied".to_string())
    }
}

#[tauri::command]
pub async fn result_retry(raw: String) -> Result<String, String> {
    let settings = settings::load();
    let text = settings
        .text
        .ok_or("Set up a text model first".to_string())?;
    let data_dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|err| err.to_string())?;
    let store =
        ob_storage::Store::open(&ob_domain::paths::history_db()).map_err(|err| err.to_string())?;
    let terms: Vec<String> = store
        .list_dict(true)
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(|entry| entry.phrase)
        .collect();
    let messages = build_messages_with(
        &Mode::Dictate,
        &raw,
        &terms,
        settings.prompt_dictate.as_deref(),
    );
    let client = OpenAiClient::new(ClientConfig {
        base_url: text.base_url.clone(),
        model: text.model.clone(),
        api_key: secrets::get_key("text"),
        connect_timeout_ms: 4_000,
        total_timeout_ms: 120_000,
    })
    .map_err(|err| err.to_string())?;
    let out = client
        .chat_with(&messages, 1024, 0.2, true)
        .await
        .map_err(|err| err.to_string())?;
    let polished = clean_model_output(&out);
    if polished.is_empty() {
        Ok(raw)
    } else {
        Ok(polished)
    }
}
