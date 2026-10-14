//! HTTP surface of the local ASR server: health, model listing, transcription.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::wav;

pub trait Transcriber: Send + Sync + 'static {
    fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<String, String>;
}

/// RMS below which audio is treated as silence and skipped.
pub const SILENCE_RMS: f32 = 0.004;

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    mean_square.sqrt()
}

#[derive(Clone)]
pub struct AppState {
    pub model_id: String,
    pub transcriber: Arc<dyn Transcriber>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/audio/transcriptions", post(transcriptions))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn models(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "object": "list",
        "data": [{ "id": state.model_id, "object": "model" }]
    }))
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

async fn transcriptions(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut file: Option<Vec<u8>> = None;
    let mut language: Option<String> = None;
    let mut prompt: Option<String> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "file" => {
                if let Ok(bytes) = field.bytes().await {
                    file = Some(bytes.to_vec());
                }
            }
            "language" => {
                if let Ok(text) = field.text().await {
                    language = Some(text);
                }
            }
            "prompt" => {
                if let Ok(text) = field.text().await {
                    prompt = Some(text);
                }
            }
            _ => {}
        }
    }

    let Some(file_bytes) = file else {
        return error_response(StatusCode::BAD_REQUEST, "missing file");
    };
    let samples = match wav::decode_wav_16k_mono(&file_bytes) {
        Ok(samples) => samples,
        Err(msg) => return error_response(StatusCode::BAD_REQUEST, &msg),
    };
    let language = match language {
        Some(l) if !l.is_empty() && l != "auto" => Some(l),
        _ => None,
    };
    let prompt = prompt.filter(|p| !p.is_empty());

    if rms(&samples) < SILENCE_RMS {
        return Json(json!({ "text": "" })).into_response();
    }

    let transcriber = state.transcriber.clone();
    let result = tokio::task::spawn_blocking(move || {
        transcriber.transcribe(&samples, language.as_deref(), prompt.as_deref())
    })
    .await;
    match result {
        Ok(Ok(text)) => Json(json!({ "text": text.trim() })).into_response(),
        Ok(Err(msg)) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &msg),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::TcpListener;

    struct EchoTranscriber;

    impl Transcriber for EchoTranscriber {
        fn transcribe(
            &self,
            samples: &[f32],
            _language: Option<&str>,
            prompt: Option<&str>,
        ) -> Result<String, String> {
            Ok(format!(
                "{} samples{}",
                samples.len(),
                prompt.map(|p| format!(" prompt={p}")).unwrap_or_default()
            ))
        }
    }

    fn tone_samples(count: usize) -> Vec<i16> {
        (0..count)
            .map(|i| {
                let t = i as f32 / 16000.0;
                (0.3f32 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 32767.0) as i16
            })
            .collect()
    }

    fn write_wav(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ob-asr-http-test-{}-{}.wav",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        {
            let mut writer = WavWriter::create(&path, spec).unwrap();
            for &s in samples {
                writer.write_sample(s).unwrap();
            }
            writer.finalize().unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        bytes
    }

    async fn spawn_server() -> String {
        let state = AppState {
            model_id: "test-model".to_string(),
            transcriber: Arc::new(EchoTranscriber),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn health_ok() {
        let base = spawn_server().await;
        let resp = reqwest::get(format!("{base}/health")).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn models_lists_model_id() {
        let base = spawn_server().await;
        let resp = reqwest::get(format!("{base}/v1/models")).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["data"][0]["id"], "test-model");
        assert_eq!(body["object"], "list");
    }

    #[tokio::test]
    async fn transcribe_returns_sample_count() {
        let base = spawn_server().await;
        let wav = write_wav(16000, &tone_samples(1600));
        let part = reqwest::multipart::Part::bytes(wav).file_name("a.wav");
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp = reqwest::Client::new()
            .post(format!("{base}/v1/audio/transcriptions"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["text"], "1600 samples");
    }

    #[tokio::test]
    async fn transcribe_without_file_is_400() {
        let base = spawn_server().await;
        let form = reqwest::multipart::Form::new().text("language", "en");
        let resp = reqwest::Client::new()
            .post(format!("{base}/v1/audio/transcriptions"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["error"]["message"], "missing file");
    }

    #[tokio::test]
    async fn transcribe_44k_wav_is_400() {
        let base = spawn_server().await;
        let wav = write_wav(44100, &[0i16; 1600]);
        let part = reqwest::multipart::Part::bytes(wav).file_name("a.wav");
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp = reqwest::Client::new()
            .post(format!("{base}/v1/audio/transcriptions"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("expected 16 kHz mono")
        );
    }

    #[tokio::test]
    async fn silence_returns_empty_text_without_transcribing() {
        let base = spawn_server().await;
        let wav = write_wav(16000, &[0i16; 16000]);
        let part = reqwest::multipart::Part::bytes(wav).file_name("a.wav");
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp = reqwest::Client::new()
            .post(format!("{base}/v1/audio/transcriptions"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["text"], "");
    }

    #[tokio::test]
    async fn prompt_field_is_passed_through() {
        let base = spawn_server().await;
        let wav = write_wav(16000, &tone_samples(1600));
        let part = reqwest::multipart::Part::bytes(wav).file_name("a.wav");
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("prompt", "Launch");
        let resp = reqwest::Client::new()
            .post(format!("{base}/v1/audio/transcriptions"))
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["text"], "1600 samples prompt=Launch");
    }
}
