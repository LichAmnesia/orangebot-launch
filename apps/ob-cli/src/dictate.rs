//! `ob dictate` — record, transcribe, polish, insert.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "macos")]
use ob_platform::Delivery;
#[cfg(target_os = "macos")]
use ob_platform::Platform as _;
use ob_providers::{ClientConfig, OpenAiClient};

use crate::cli::DictateArgs;

/// Run one dictate / translate / ask cycle.
pub async fn run(args: DictateArgs) -> anyhow::Result<()> {
    let mode = match args.mode.as_str() {
        "dictate" => ob_type::Mode::Dictate,
        "translate" => ob_type::Mode::Translate {
            target: args.target.clone(),
        },
        "ask" => ob_type::Mode::Ask,
        "rewrite" => anyhow::bail!("rewrite needs selected text; use the desktop app"),
        m => anyhow::bail!("unknown mode {m}: use dictate|translate|ask"),
    };

    #[cfg(target_os = "macos")]
    let target = if args.insert {
        let platform = ob_platform_macos::MacPlatform::new();
        let token = platform
            .capture_target()
            .map_err(|err| anyhow::anyhow!("capture target: {err}"))?;
        Some((platform, token))
    } else {
        None
    };

    #[cfg(not(target_os = "macos"))]
    if args.insert {
        anyhow::bail!("--insert is only supported on macOS");
    }

    let (bytes, duration_ms) = match &args.wav {
        Some(path) => (std::fs::read(path)?, 0_u64),
        None => {
            let captured = crate::audio::capture(args.seconds, args.device.as_deref())?;
            let duration_ms = captured.duration_ms;
            let bytes = ob_audio::encode_wav_mono_i16(&captured.samples_16k_mono, 16_000);
            (bytes, duration_ms)
        }
    };

    let store = crate::history::open_store()?;
    let terms: Vec<String> = store
        .list_dict(true)?
        .into_iter()
        .map(|entry| entry.phrase)
        .collect();
    let prompt = (!terms.is_empty()).then(|| terms.join(", "));

    let asr_client = OpenAiClient::new(ClientConfig {
        base_url: args.asr_url.clone(),
        model: args.asr_model.clone(),
        api_key: std::env::var("OB_ASR_API_KEY").ok(),
        connect_timeout_ms: 5_000,
        total_timeout_ms: 120_000,
    })?;
    let t0 = Instant::now();
    let raw = asr_client
        .transcribe_with_prompt(bytes, args.language.as_deref(), prompt.as_deref())
        .await
        .map_err(|err| anyhow::anyhow!("asr: {err}"))?;
    let asr_ms = t0.elapsed().as_millis() as u64;

    if raw.trim().is_empty() {
        println!("(no speech detected)");
        return Ok(());
    }

    let llm_client = OpenAiClient::new(ClientConfig {
        base_url: args.llm_url.clone(),
        model: args.llm_model.clone(),
        api_key: std::env::var("OB_LLM_API_KEY").ok(),
        connect_timeout_ms: 5_000,
        total_timeout_ms: 120_000,
    })?;
    let messages = ob_type::build_messages(&mode, &raw, &terms);
    let t1 = Instant::now();
    let out = llm_client
        .chat_with(&messages, 1024, 0.2, true)
        .await
        .map_err(|err| anyhow::anyhow!("llm: {err}"))?;
    let mut final_text = ob_type::clean_model_output(&out);
    let llm_ms = t1.elapsed().as_millis() as u64;
    if final_text.is_empty() {
        final_text = raw.trim().to_string();
        eprintln!("llm returned empty text; using the raw transcript");
    }

    println!("raw:   {raw}");
    println!("final: {final_text}");
    eprintln!("asr {asr_ms} ms · llm {llm_ms} ms");

    #[cfg(target_os = "macos")]
    let (delivery, app): (&str, Option<String>) = if args.insert {
        match target {
            Some((platform, token)) => {
                let outcome = match ob_platform::deliver(&platform, &token, &final_text) {
                    Ok(Delivery::Verified) => "verified",
                    Ok(Delivery::Attempted) => "attempted",
                    Ok(Delivery::CopiedOnly) => {
                        eprintln!("target changed or insert failed — text copied to clipboard");
                        "copied_only"
                    }
                    Err(err) => {
                        eprintln!("insert failed: {err}");
                        "failed"
                    }
                };
                (outcome, token.app_id.clone())
            }
            None => ("printed", None),
        }
    } else {
        ("printed", None)
    };

    #[cfg(not(target_os = "macos"))]
    let (delivery, app): (&str, Option<String>) = ("printed", None);

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default();
    let mode_label = match &mode {
        ob_type::Mode::Dictate => "dictate",
        ob_type::Mode::Translate { .. } => "translate",
        ob_type::Mode::Ask => "ask",
        ob_type::Mode::Rewrite => "rewrite",
    };
    store.insert_session(&ob_storage::SessionRecord {
        id: format!("{now_ms}-{}", std::process::id()),
        created_at_ms: now_ms as i64,
        mode: mode_label.to_string(),
        app,
        raw_text: raw.trim().to_string(),
        final_text,
        delivery: delivery.to_string(),
        duration_ms: duration_ms as i64,
    })?;
    Ok(())
}
