use std::time::{SystemTime, UNIX_EPOCH};

use ob_ipc::{AnswerEvent, ResultEvent, VoiceEvent, event};
#[cfg(target_os = "macos")]
use ob_platform::Platform;
use ob_platform::{Delivery, TargetToken};
use ob_providers::{ChatMessage, ClientConfig, OpenAiClient, ProviderError};
use ob_storage::SessionRecord;
use ob_type::{
    Mode, build_ask_messages, build_messages_full, build_rewrite_messages,
    build_search_answer_messages, clean_model_output, parse_ask_output,
};
use tauri::Emitter;
use tauri::Manager;

use crate::secrets;
use crate::settings::{self, Connection};
use crate::voice::{CapturedSelection, VoiceMode};

pub fn emit(
    app: &tauri::AppHandle,
    phase: &str,
    level: f32,
    text: Option<String>,
    error: Option<String>,
) {
    let _ = app.emit(
        event::VOICE,
        VoiceEvent {
            phase: phase.to_string(),
            level,
            text,
            error,
        },
    );
}

fn fail(app: &tauri::AppHandle, msg: String) -> String {
    emit(app, "failed", 0.0, None, Some(msg.clone()));
    msg
}

fn client_for(conn: &Connection, api_key: Option<String>) -> Result<OpenAiClient, String> {
    OpenAiClient::new(ClientConfig {
        base_url: conn.base_url.clone(),
        model: conn.model.clone(),
        api_key,
        connect_timeout_ms: 4_000,
        total_timeout_ms: 120_000,
    })
    .map_err(|e| e.to_string())
}

fn is_retriable(err: &ProviderError) -> bool {
    matches!(err, ProviderError::Connect(_) | ProviderError::Timeout)
}

fn emit_fallback(app: &tauri::AppHandle) {
    emit(
        app,
        "fallback",
        0.0,
        Some("Primary server unreachable — using fallback".to_string()),
        None,
    );
}

async fn transcribe_any(
    primary: &Connection,
    fallback: Option<&Connection>,
    role_key: &str,
    wav: Vec<u8>,
    prompt: Option<&str>,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let mut client = client_for(primary, secrets::get_key(role_key))?;
    let err = match client
        .transcribe_with_prompt(wav.clone(), None, prompt)
        .await
    {
        Ok(text) => return Ok(text),
        Err(e) => e,
    };
    if !is_retriable(&err) {
        return Err(err.to_string());
    }
    let Some(fb) = fallback else {
        return Err(err.to_string());
    };
    emit_fallback(app);
    client = client_for(fb, secrets::get_key(&format!("{role_key}_fallback")))?;
    client
        .transcribe_with_prompt(wav, None, prompt)
        .await
        .map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
async fn chat_any(
    primary: &Connection,
    fallback: Option<&Connection>,
    role_key: &str,
    messages: &[ChatMessage],
    max_tokens: u32,
    temperature: f32,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let mut client = client_for(primary, secrets::get_key(role_key))?;
    let err = match client
        .chat_with(messages, max_tokens, temperature, true)
        .await
    {
        Ok(text) => return Ok(text),
        Err(e) => e,
    };
    if !is_retriable(&err) {
        return Err(err.to_string());
    }
    let Some(fb) = fallback else {
        return Err(err.to_string());
    };
    emit_fallback(app);
    client = client_for(fb, secrets::get_key(&format!("{role_key}_fallback")))?;
    client
        .chat_with(messages, max_tokens, temperature, true)
        .await
        .map_err(|e| e.to_string())
}

fn show_result(app: &tauri::AppHandle, raw: &str, text: &str, reason: &str) {
    let _ = app.emit(
        event::RESULT,
        ResultEvent {
            raw: raw.to_string(),
            text: text.to_string(),
            reason: reason.to_string(),
        },
    );
    if let Some(window) = app.get_webview_window("result") {
        let _ = window.show();
    }
}

#[allow(clippy::too_many_arguments)]
fn deliver_and_record(
    app: &tauri::AppHandle,
    store: &ob_storage::Store,
    target: Option<TargetToken>,
    final_text: String,
    mode_label: &str,
    card_raw: &str,
    raw_text: String,
    done_text: Option<&str>,
    duration_ms: i64,
) {
    emit(app, "inserting", 0.0, Some(final_text.clone()), None);
    #[cfg(target_os = "macos")]
    let (delivery, card_reason): (
        Result<Delivery, ob_platform::PlatformError>,
        Option<String>,
    ) = {
        let platform = ob_platform_macos::MacPlatform::new();
        match &target {
            Some(token) => match platform.same_target(token) {
                Ok(true) => match platform.insert_text(&final_text) {
                    Ok(delivery) => (Ok(delivery), None),
                    Err(e) => {
                        let reason = format!("Inserting failed: {e}");
                        match platform.copy_to_clipboard(&final_text) {
                            Ok(()) => (Ok(Delivery::CopiedOnly), Some(reason)),
                            Err(e) => (Err(e), None),
                        }
                    }
                },
                Ok(false) => {
                    let reason =
                        "The app or field changed before the text could be inserted.".to_string();
                    match platform.copy_to_clipboard(&final_text) {
                        Ok(()) => (Ok(Delivery::CopiedOnly), Some(reason)),
                        Err(e) => (Err(e), None),
                    }
                }
                Err(e) => (Err(e), None),
            },
            None => (
                platform
                    .copy_to_clipboard(&final_text)
                    .map(|_| Delivery::CopiedOnly),
                None,
            ),
        }
    };
    #[cfg(not(target_os = "macos"))]
    let (delivery, card_reason): (
        Result<Delivery, ob_platform::PlatformError>,
        Option<String>,
    ) = {
        let _ = &target;
        (Ok(Delivery::CopiedOnly), None)
    };

    let error = delivery.as_ref().err().map(|e| e.to_string());
    let (phase, delivery_label) = match &delivery {
        Ok(Delivery::Verified) => ("done", "verified"),
        Ok(Delivery::Attempted) => ("done", "attempted"),
        Ok(Delivery::CopiedOnly) => ("copied", "copied_only"),
        Err(_) => ("failed", "failed"),
    };
    let bar_text = match (phase, done_text) {
        ("done", Some(text)) => text.to_string(),
        _ => final_text.clone(),
    };
    emit(app, phase, 0.0, Some(bar_text), error);

    if let Some(reason) = &card_reason {
        show_result(app, card_raw, &final_text, reason);
    }

    let created_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let record = SessionRecord {
        id: format!("{created_at_ms}-desk"),
        created_at_ms,
        mode: mode_label.to_string(),
        app: target.and_then(|t| t.app_id),
        raw_text,
        final_text,
        delivery: delivery_label.to_string(),
        duration_ms,
    };
    let _ = store.insert_session(&record);
}

pub async fn process(
    app: tauri::AppHandle,
    captured: ob_audio::Captured,
    target: Option<TargetToken>,
    mode: VoiceMode,
    selection: Option<CapturedSelection>,
) -> Result<(), String> {
    let settings = settings::load();
    let asr = settings
        .asr
        .ok_or("Speech model is not set up".to_string())?;
    let text = settings
        .text
        .ok_or("Text model is not set up".to_string())?;
    let translate_target = settings.translate_target.clone();
    let polish = settings.polish;
    let prompt_override = match mode {
        VoiceMode::Ask => settings.prompt_ask.clone(),
        VoiceMode::Translate => settings.prompt_translate.clone(),
        VoiceMode::Dictate => settings.prompt_dictate.clone(),
        VoiceMode::Rewrite => settings.prompt_rewrite.clone(),
    };

    if captured.duration_ms < 300 {
        emit(&app, "cancelled", 0.0, None, None);
        return Ok(());
    }

    emit(&app, "transcribing", 0.0, None, None);
    let data_dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|e| fail(&app, e.to_string()))?;
    let store = ob_storage::Store::open(&ob_domain::paths::history_db())
        .map_err(|e| fail(&app, e.to_string()))?;
    let terms: Vec<String> = store
        .list_dict(true)
        .map_err(|e| fail(&app, e.to_string()))?
        .into_iter()
        .map(|entry| entry.phrase)
        .collect();
    let prompt = (!terms.is_empty()).then(|| terms.join(", "));

    let wav = ob_audio::encode_wav_mono_i16(&captured.samples_16k_mono, 16_000);
    let raw = transcribe_any(
        &asr,
        settings.asr_fallback.as_ref(),
        "asr",
        wav,
        prompt.as_deref(),
        &app,
    )
    .await
    .map_err(|e| fail(&app, e))?;

    if raw.trim().is_empty() && mode != VoiceMode::Rewrite {
        emit(
            &app,
            "empty",
            0.0,
            Some("No speech detected".to_string()),
            None,
        );
        return Ok(());
    }

    if mode == VoiceMode::Ask {
        emit(&app, "answering", 0.0, Some(raw.clone()), None);
        let sel_text = selection.as_ref().map(|s| s.text.as_str());
        let editable = selection.as_ref().is_some_and(|s| s.editable);
        let scope = if selection.is_some() {
            "selection".to_string()
        } else {
            "general".to_string()
        };
        let messages = build_ask_messages(&raw, sel_text, editable);
        let result = match chat_any(
            &text,
            settings.text_fallback.as_ref(),
            "text",
            &messages,
            1024,
            0.3,
            &app,
        )
        .await
        {
            Ok(out) => parse_ask_output(&out, editable),
            Err(e) => {
                if let Some(window) = app.get_webview_window("answer") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit(
                    event::ANSWER,
                    AnswerEvent {
                        question: raw.clone(),
                        answer: None,
                        error: Some(e.clone()),
                        done: true,
                        scope,
                        can_replace: false,
                        sources: Vec::new(),
                    },
                );
                fail(&app, e.clone());
                return Err(e);
            }
        };

        #[cfg(target_os = "macos")]
        let rewritten = {
            let platform = ob_platform_macos::MacPlatform::new();
            let unchanged = result.action == ob_type::AskAction::Rewrite
                && target
                    .as_ref()
                    .is_some_and(|t| platform.same_target(t).unwrap_or(false));
            if unchanged {
                if let Some(token) = &target {
                    let _ = ob_platform::deliver(&platform, token, &result.text);
                }
                emit(
                    &app,
                    "done",
                    0.0,
                    Some("Replaced the selection".to_string()),
                    None,
                );
                true
            } else {
                false
            }
        };
        #[cfg(not(target_os = "macos"))]
        let rewritten = false;
        if rewritten {
            return Ok(());
        }

        if let Some(window) = app.get_webview_window("answer") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        crate::ask::remember(target.clone());

        let mut answer = result.text.clone();
        let mut sources: Vec<ob_ipc::Source> = Vec::new();
        let mut answer_scope = scope;
        let mut can_replace = editable;
        if result.action == ob_type::AskAction::Answer
            && let Some(query) = result.search.clone()
        {
            match crate::websearch::configured_client() {
                Some(client) => {
                    let _ = app.emit(
                        event::ANSWER,
                        AnswerEvent {
                            question: raw.clone(),
                            answer: None,
                            error: None,
                            done: false,
                            scope: "web".to_string(),
                            can_replace: false,
                            sources: Vec::new(),
                        },
                    );
                    let unavailable = match client.search(&query, 5).await {
                        Ok(hits) if !hits.is_empty() => {
                            let messages = build_search_answer_messages(&raw, &hits);
                            match chat_any(
                                &text,
                                settings.text_fallback.as_ref(),
                                "text",
                                &messages,
                                1024,
                                0.3,
                                &app,
                            )
                            .await
                            {
                                Ok(out) => {
                                    answer = clean_model_output(&out);
                                    sources = hits
                                        .iter()
                                        .map(|hit| ob_ipc::Source {
                                            title: hit.title.clone(),
                                            url: hit.url.clone(),
                                        })
                                        .collect();
                                    answer_scope = "web".to_string();
                                    can_replace = false;
                                    None
                                }
                                Err(e) => Some(e),
                            }
                        }
                        Ok(_) => Some("no results".to_string()),
                        Err(e) => Some(e.to_string()),
                    };
                    if let Some(e) = unavailable {
                        answer = format!("_Web search was unavailable: {e}_\n\n{answer}");
                    }
                }
                None => {
                    answer = format!(
                        "{answer}\n\n_Set up web search in Connections for up-to-date answers._"
                    );
                }
            }
        }

        let _ = app.emit(
            event::ANSWER,
            AnswerEvent {
                question: raw.clone(),
                answer: Some(answer.clone()),
                error: None,
                done: true,
                scope: answer_scope,
                can_replace,
                sources,
            },
        );
        emit(&app, "done", 0.0, Some("Answer ready".to_string()), None);

        let created_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let record = SessionRecord {
            id: format!("{created_at_ms}-desk"),
            created_at_ms,
            mode: "ask".to_string(),
            app: target.and_then(|t| t.app_id),
            raw_text: raw.trim().to_string(),
            final_text: answer,
            delivery: "copied_only".to_string(),
            duration_ms: captured.duration_ms as i64,
        };
        let _ = store.insert_session(&record);
        return Ok(());
    }

    if mode == VoiceMode::Rewrite {
        let Some(selection) = &selection else {
            return Err(fail(
                &app,
                "Select the text you want to rewrite first.".to_string(),
            ));
        };
        emit(&app, "polishing", 0.0, Some(raw.clone()), None);
        let style = settings
            .style_enabled
            .then_some(settings.style_notes.as_str())
            .filter(|notes| !notes.is_empty());
        let messages = build_rewrite_messages(
            &raw,
            &selection.text,
            settings.prompt_rewrite.as_deref(),
            style,
        );
        let out = match chat_any(
            &text,
            settings.text_fallback.as_ref(),
            "text",
            &messages,
            2048,
            0.3,
            &app,
        )
        .await
        {
            Ok(out) => out,
            Err(e) => return Err(fail(&app, e)),
        };
        let rewritten = clean_model_output(&out);
        if rewritten.is_empty() {
            return Err(fail(&app, "The model returned nothing".to_string()));
        }
        let instruction = if raw.trim().is_empty() {
            "(default)".to_string()
        } else {
            raw.trim().to_string()
        };
        deliver_and_record(
            &app,
            &store,
            target,
            rewritten,
            "rewrite",
            &selection.text,
            instruction,
            Some("Rewritten"),
            captured.duration_ms as i64,
        );
        return Ok(());
    }

    let type_mode = match mode {
        VoiceMode::Translate => Mode::Translate {
            target: translate_target,
        },
        _ => Mode::Dictate,
    };
    let mode_label = match mode {
        VoiceMode::Translate => "translate",
        _ => "dictate",
    };

    let final_text = if mode == VoiceMode::Dictate && !polish {
        capitalize_first(raw.trim())
    } else {
        emit(&app, "polishing", 0.0, Some(raw.clone()), None);
        let style = settings
            .style_enabled
            .then_some(settings.style_notes.as_str())
            .filter(|notes| !notes.is_empty());
        let messages =
            build_messages_full(&type_mode, &raw, &terms, prompt_override.as_deref(), style);
        let out = match chat_any(
            &text,
            settings.text_fallback.as_ref(),
            "text",
            &messages,
            1024,
            0.2,
            &app,
        )
        .await
        {
            Ok(out) => out,
            Err(e) => {
                fail(&app, e.clone());
                let raw_trimmed = raw.trim().to_string();
                #[cfg(target_os = "macos")]
                {
                    let platform = ob_platform_macos::MacPlatform::new();
                    let _ = platform.copy_to_clipboard(&raw_trimmed);
                }
                show_result(
                    &app,
                    &raw_trimmed,
                    &raw_trimmed,
                    &format!("The text model failed: {e} — showing what was heard."),
                );
                let created_at_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let record = SessionRecord {
                    id: format!("{created_at_ms}-desk"),
                    created_at_ms,
                    mode: mode_label.to_string(),
                    app: target.as_ref().and_then(|t| t.app_id.clone()),
                    raw_text: raw_trimmed.clone(),
                    final_text: raw_trimmed,
                    delivery: "copied_only".to_string(),
                    duration_ms: captured.duration_ms as i64,
                };
                let _ = store.insert_session(&record);
                return Ok(());
            }
        };
        let mut polished = clean_model_output(&out);
        if polished.is_empty() {
            polished = raw.trim().to_string();
        }
        polished
    };

    deliver_and_record(
        &app,
        &store,
        target,
        final_text,
        mode_label,
        raw.trim(),
        raw.trim().to_string(),
        None,
        captured.duration_ms as i64,
    );
    Ok(())
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(text.len());
    out.extend(first.to_uppercase());
    out.push_str(chars.as_str());
    out
}
