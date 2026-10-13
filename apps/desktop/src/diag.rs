use std::path::Path;

use ob_ipc::{DiagCheck, DiagReport};
use ob_providers::{ClientConfig, OpenAiClient};

use crate::settings::{self, Connection};

const MANAGED_ASR_PORT: u16 = 17860;
const MANAGED_LLM_PORT: u16 = 17861;
const LOG_TAIL_LINES: usize = 40;

#[cfg(target_os = "macos")]
fn accessibility_check() -> DiagCheck {
    let trusted = ob_platform_macos::accessibility_trusted();
    DiagCheck {
        name: "Accessibility".to_string(),
        ok: trusted,
        detail: if trusted {
            "Allowed".to_string()
        } else {
            "Not allowed — text can only be copied, not typed".to_string()
        },
    }
}

#[cfg(not(target_os = "macos"))]
fn accessibility_check() -> DiagCheck {
    DiagCheck {
        name: "Accessibility".to_string(),
        ok: true,
        detail: "Not applicable".to_string(),
    }
}

fn data_folder_check() -> DiagCheck {
    let dir = ob_domain::paths::data_dir();
    let probe = dir.join(".ob-diag-probe");
    let detail = dir.display().to_string();
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&probe, b"probe")) {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            DiagCheck {
                name: "Data folder".to_string(),
                ok: true,
                detail,
            }
        }
        Err(e) => DiagCheck {
            name: "Data folder".to_string(),
            ok: false,
            detail: format!("{detail} — {e}"),
        },
    }
}

async fn endpoint_check(name: &str, conn: Option<Connection>) -> DiagCheck {
    let Some(conn) = conn else {
        return DiagCheck {
            name: name.to_string(),
            ok: false,
            detail: "Not set up".to_string(),
        };
    };
    let detail_ok = format!("{} · {}", conn.base_url, conn.model);
    let client = OpenAiClient::new(ClientConfig {
        base_url: conn.base_url.clone(),
        model: conn.model.clone(),
        api_key: None,
        connect_timeout_ms: 1_500,
        total_timeout_ms: 3_000,
    });
    match client {
        Ok(client) => match client.list_models().await {
            Ok(_) => DiagCheck {
                name: name.to_string(),
                ok: true,
                detail: detail_ok,
            },
            Err(e) => DiagCheck {
                name: name.to_string(),
                ok: false,
                detail: e.to_string(),
            },
        },
        Err(e) => DiagCheck {
            name: name.to_string(),
            ok: false,
            detail: e.to_string(),
        },
    }
}

fn local_models_check(recipe: Option<String>) -> DiagCheck {
    let Some(recipe) = recipe else {
        return DiagCheck {
            name: "Local models".to_string(),
            ok: true,
            detail: "Not used".to_string(),
        };
    };
    let listening = !ob_runtime::is_port_free("127.0.0.1", MANAGED_ASR_PORT)
        && !ob_runtime::is_port_free("127.0.0.1", MANAGED_LLM_PORT);
    DiagCheck {
        name: "Local models".to_string(),
        ok: listening,
        detail: format!(
            "{recipe} · ports {MANAGED_ASR_PORT}/{MANAGED_LLM_PORT} {}",
            if listening {
                "listening"
            } else {
                "not listening"
            }
        ),
    }
}

fn tail_lines(path: &Path) -> String {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return String::new();
    };
    raw.lines()
        .rev()
        .take(LOG_TAIL_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

#[tauri::command]
pub async fn run_diagnostics() -> Result<DiagReport, String> {
    let settings = settings::load();
    let data_dir = ob_domain::paths::data_dir();
    let log_dir = data_dir.join("logs");
    let checks = vec![
        accessibility_check(),
        data_folder_check(),
        endpoint_check("Speech model", settings.asr.clone()).await,
        endpoint_check("Text model", settings.text.clone()).await,
        local_models_check(settings.managed_recipe.clone()),
        DiagCheck {
            name: "Hotkeys".to_string(),
            ok: true,
            detail: format!(
                "{} · {} · {} ({})",
                settings.hotkey,
                settings.translate_hotkey,
                settings.ask_hotkey,
                settings.activation
            ),
        },
    ];
    Ok(DiagReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        checks,
        asr_log_tail: tail_lines(&log_dir.join("asr.log")),
        llm_log_tail: tail_lines(&log_dir.join("llm.log")),
    })
}
