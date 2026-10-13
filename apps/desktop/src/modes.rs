use ob_providers::{ClientConfig, OpenAiClient};
use ob_type::{Mode, build_messages_with, clean_model_output, default_system};

const IDS: [(&str, &str); 4] = [
    ("dictate", "Dictate"),
    ("translate", "Translate"),
    ("ask", "Ask"),
    ("rewrite", "Rewrite"),
];

fn mode_for(id: &str, settings: &crate::settings::Settings) -> Option<Mode> {
    match id {
        "dictate" => Some(Mode::Dictate),
        "translate" => Some(Mode::Translate {
            target: settings.translate_target.clone(),
        }),
        "ask" => Some(Mode::Ask),
        "rewrite" => Some(Mode::Rewrite),
        _ => None,
    }
}

fn override_for(id: &str, settings: &crate::settings::Settings) -> Option<String> {
    match id {
        "dictate" => settings.prompt_dictate.clone(),
        "translate" => settings.prompt_translate.clone(),
        "ask" => settings.prompt_ask.clone(),
        "rewrite" => settings.prompt_rewrite.clone(),
        _ => None,
    }
}

fn prompt_field<'a>(
    id: &str,
    settings: &'a mut crate::settings::Settings,
) -> Option<&'a mut Option<String>> {
    match id {
        "dictate" => Some(&mut settings.prompt_dictate),
        "translate" => Some(&mut settings.prompt_translate),
        "ask" => Some(&mut settings.prompt_ask),
        "rewrite" => Some(&mut settings.prompt_rewrite),
        _ => None,
    }
}

#[tauri::command]
pub fn get_modes() -> Result<Vec<ob_ipc::ModeInfo>, String> {
    let settings = crate::settings::load();
    let mut modes = Vec::new();
    for (id, name) in IDS {
        let mode = mode_for(id, &settings).ok_or_else(|| "unknown mode".to_string())?;
        let hotkey = match id {
            "dictate" => settings.hotkey.clone(),
            "translate" => settings.translate_hotkey.clone(),
            "rewrite" => settings.rewrite_hotkey.clone(),
            _ => settings.ask_hotkey.clone(),
        };
        let default_prompt = default_system(&mode);
        let customized = override_for(id, &settings).is_some();
        let prompt = override_for(id, &settings).unwrap_or_else(|| default_prompt.clone());
        modes.push(ob_ipc::ModeInfo {
            id: id.to_string(),
            name: name.to_string(),
            hotkey,
            default_prompt,
            prompt,
            customized,
        });
    }
    Ok(modes)
}

#[tauri::command]
pub fn save_mode_prompt(id: String, prompt: Option<String>) -> Result<(), String> {
    let mut settings = crate::settings::load();
    let mode = mode_for(&id, &settings).ok_or_else(|| "unknown mode".to_string())?;
    let field = prompt_field(&id, &mut settings).ok_or_else(|| "unknown mode".to_string())?;
    *field = match prompt {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed == default_system(&mode) {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => None,
    };
    crate::settings::save(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_mode(id: String, prompt: String, sample: String) -> Result<String, String> {
    if sample.trim().is_empty() {
        return Err("Type a sample first".to_string());
    }
    let settings = crate::settings::load();
    let mode = mode_for(&id, &settings).ok_or_else(|| "unknown mode".to_string())?;
    let text = settings
        .text
        .clone()
        .ok_or_else(|| "Set up a text model first".to_string())?;
    let client = OpenAiClient::new(ClientConfig {
        base_url: text.base_url,
        model: text.model,
        api_key: crate::secrets::get_key("text"),
        connect_timeout_ms: 4_000,
        total_timeout_ms: 120_000,
    })
    .map_err(|e| e.to_string())?;
    let messages = build_messages_with(
        &mode,
        &sample,
        &[],
        (!prompt.trim().is_empty()).then_some(prompt.as_str()),
    );
    let out = client
        .chat_with(&messages, 512, 0.2, true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(clean_model_output(&out))
}
