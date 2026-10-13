//! `ob provider` — endpoint probes.

use std::time::Instant;

use ob_providers::{ChatMessage, ClientConfig, OpenAiClient};

pub async fn test(
    base_url: String,
    model: String,
    api_key_env: Option<String>,
    asr: bool,
) -> anyhow::Result<()> {
    let api_key = api_key_env.and_then(|var| std::env::var(var).ok());
    let client = OpenAiClient::new(ClientConfig {
        base_url,
        model,
        api_key,
        connect_timeout_ms: 5_000,
        total_timeout_ms: 60_000,
    })
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    match client.list_models().await {
        Ok(ids) => println!("models: {}", ids.join(", ")),
        Err(e) => println!("models: unavailable ({e})"),
    }

    if asr {
        let wav = ob_audio::encode_wav_mono_i16(&vec![0.0f32; 16_000], 16_000);
        let started = Instant::now();
        let text = client
            .transcribe(wav, None)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let ms = started.elapsed().as_millis();
        println!("asr ok in {ms} ms: {text:?}");
    } else {
        let messages = [ChatMessage {
            role: "user".to_string(),
            content: "Reply with the single word: pong".to_string(),
        }];
        let started = Instant::now();
        let output = client
            .chat_with(&messages, 64, 0.0, true)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let text = ob_type::clean_model_output(&output);
        let ms = started.elapsed().as_millis();
        println!("chat ok in {ms} ms: {text:?}");
    }
    Ok(())
}
