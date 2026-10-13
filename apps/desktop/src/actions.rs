use ob_ipc::{ConnectionInput, StepState, TestReport, TestStep};
use ob_providers::{ChatMessage, ClientConfig, OpenAiClient, ProviderError};
use tauri::Emitter;

use crate::secrets;
use crate::settings::{self, Connection};

#[tauri::command]
pub fn save_connection(app: tauri::AppHandle, input: ConnectionInput) -> Result<(), String> {
    let role = input.role.as_str();
    if !matches!(role, "asr" | "text" | "asr_fallback" | "text_fallback") {
        return Err("unknown role".to_string());
    }
    let base_url = input.base_url.trim().to_string();
    let is_fallback = role.ends_with("_fallback");

    if base_url.is_empty() && is_fallback {
        secrets::delete_key(role);
        let mut current = settings::load();
        match role {
            "asr_fallback" => current.asr_fallback = None,
            _ => current.text_fallback = None,
        }
        settings::save(&current).map_err(|e| e.to_string())?;
        let _ = app.emit(ob_ipc::event::STATUS, ());
        return Ok(());
    }

    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("base URL must start with http:// or https://".to_string());
    }
    let model = input.model.trim().to_string();
    if model.is_empty() {
        return Err("model is required".to_string());
    }

    let provided_key = input.api_key.filter(|key| !key.is_empty());
    if let Some(key) = &provided_key {
        secrets::set_key(role, key)?;
    }
    let has_key = provided_key.is_some() || secrets::get_key(role).is_some();

    let mut current = settings::load();
    let connection = Connection {
        base_url,
        model,
        has_key,
    };
    match role {
        "asr" => current.asr = Some(connection),
        "asr_fallback" => current.asr_fallback = Some(connection),
        "text_fallback" => current.text_fallback = Some(connection),
        _ => current.text = Some(connection),
    }
    settings::save(&current).map_err(|e| e.to_string())?;
    let _ = app.emit(ob_ipc::event::STATUS, ());
    Ok(())
}

fn is_text_role(role: &str) -> bool {
    role == "text" || role == "text_fallback"
}

fn friendly(e: &ProviderError, base_url: &str) -> String {
    match e {
        ProviderError::Connect(_) => {
            if base_url.contains("127.0.0.1:17860") || base_url.contains("127.0.0.1:17861") {
                "Nothing is running here yet. Start local models in Models → Use on this Mac."
                    .to_string()
            } else if base_url.contains("127.0.0.1") || base_url.contains("localhost") {
                "Nothing is answering on this port. Is the server running?".to_string()
            } else {
                "Can't reach this server. Check the address and that it is reachable from this Mac."
                    .to_string()
            }
        }
        ProviderError::Timeout => "The server did not answer in time.".to_string(),
        ProviderError::Auth(code) => format!("The server rejected the API key ({code})."),
        ProviderError::NotFound(_) => {
            "This server does not offer this endpoint — check the base URL (usually ends in /v1)."
                .to_string()
        }
        ProviderError::RateLimited => {
            "The server is rate-limiting requests. Try again shortly.".to_string()
        }
        ProviderError::Http { status, .. } => {
            format!("The server answered with HTTP {status}.")
        }
        ProviderError::BadResponse(m) => format!("Unexpected answer: {m}"),
    }
}

#[tauri::command]
pub async fn test_connection(input: ConnectionInput) -> TestReport {
    let api_key = input
        .api_key
        .filter(|key| !key.is_empty())
        .or_else(|| secrets::get_key(&input.role));
    let cfg = ClientConfig {
        base_url: input.base_url.clone(),
        model: input.model.clone(),
        api_key,
        connect_timeout_ms: 4_000,
        total_timeout_ms: 60_000,
    };
    let endpoint = cfg.url(if is_text_role(&input.role) {
        "chat/completions"
    } else {
        "audio/transcriptions"
    });
    let report = TestReport {
        endpoint,
        steps: Vec::new(),
    };

    let client = match OpenAiClient::new(cfg) {
        Ok(client) => client,
        Err(e) => {
            return TestReport {
                steps: vec![TestStep {
                    name: "client".to_string(),
                    state: StepState::Failed,
                    detail: e.to_string(),
                }],
                ..report
            };
        }
    };

    let mut steps = Vec::new();
    let list_models = client.list_models().await;
    let mut unreachable = false;
    match &list_models {
        Ok(ids) => {
            let mut detail = format!("{} models", ids.len());
            if !ids.iter().any(|id| id == &input.model) {
                detail.push_str(" · model id not listed");
            }
            steps.push(TestStep {
                name: "list models".to_string(),
                state: StepState::Ok,
                detail,
            });
        }
        Err(e @ ProviderError::Auth(_)) => steps.push(TestStep {
            name: "list models".to_string(),
            state: StepState::Failed,
            detail: friendly(e, &input.base_url),
        }),
        Err(e @ ProviderError::Connect(_)) => {
            unreachable = true;
            steps.push(TestStep {
                name: "list models".to_string(),
                state: StepState::Failed,
                detail: friendly(e, &input.base_url),
            });
        }
        Err(e) => steps.push(TestStep {
            name: "list models".to_string(),
            state: StepState::Skipped,
            detail: friendly(e, &input.base_url),
        }),
    }

    if unreachable {
        steps.push(TestStep {
            name: "inference".to_string(),
            state: StepState::Skipped,
            detail: "Skipped — the server is not reachable.".to_string(),
        });
        return TestReport { steps, ..report };
    }

    let inference = if is_text_role(&input.role) {
        let messages = [ChatMessage {
            role: "user".to_string(),
            content: "Reply with the single word: pong".to_string(),
        }];
        match client.chat_with(&messages, 16, 0.0, true).await {
            Ok(output) => TestStep {
                name: "inference".to_string(),
                state: StepState::Ok,
                detail: format!("replied {:?}", ob_type::clean_model_output(&output)),
            },
            Err(e) => TestStep {
                name: "inference".to_string(),
                state: StepState::Failed,
                detail: friendly(&e, &input.base_url),
            },
        }
    } else {
        let wav = ob_audio::encode_wav_mono_i16(&vec![0.0f32; 16_000], 16_000);
        match client.transcribe(wav, None).await {
            Ok(_) => TestStep {
                name: "inference".to_string(),
                state: StepState::Ok,
                detail: "transcription endpoint answered".to_string(),
            },
            Err(e) => TestStep {
                name: "inference".to_string(),
                state: StepState::Failed,
                detail: friendly(&e, &input.base_url),
            },
        }
    };
    steps.push(inference);

    TestReport { steps, ..report }
}

#[tauri::command]
pub fn download_model(app: tauri::AppHandle, model_id: String) -> Result<(), String> {
    crate::downloads::enqueue(&app, &model_id)
}
