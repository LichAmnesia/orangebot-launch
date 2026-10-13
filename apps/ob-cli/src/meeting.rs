//! `ob meeting` — streaming transcription from the microphone or a WAV file.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use ob_audio::Recorder;
use ob_meeting::{Chunk, Chunker, ChunkerConfig, Transcript};
use ob_providers::{ClientConfig, OpenAiClient};

async fn handle_chunk(
    client: &OpenAiClient,
    chunker_transcript: &mut Transcript,
    language: &Option<String>,
    chunk: Chunk,
) -> anyhow::Result<()> {
    if chunk.is_silent {
        return Ok(());
    }
    let wav = ob_audio::encode_wav_mono_i16(&chunk.samples, 16_000);
    let tail = chunker_transcript.tail(160);
    let prompt = (!tail.is_empty()).then_some(tail);
    let text = client
        .transcribe_with_prompt(wav, language.as_deref(), prompt.as_deref())
        .await
        .map_err(|err| anyhow::anyhow!("asr: {err}"))?;
    if let Some(seg) = chunker_transcript.push(chunk.start_ms, chunk.end_ms, &text) {
        let start_s = seg.start_ms / 1000;
        println!("[{:02}:{:02}] {}", start_s / 60, start_s % 60, seg.text);
    }
    Ok(())
}

async fn from_wav(
    client: &OpenAiClient,
    path: &PathBuf,
    language: &Option<String>,
) -> anyhow::Result<Transcript> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000
        || spec.channels != 1
        || spec.sample_format != hound::SampleFormat::Int
        || spec.bits_per_sample != 16
    {
        bail!("wav must be 16 kHz mono i16, got {spec:?}");
    }
    let mut chunker = Chunker::new(ChunkerConfig::default());
    let mut transcript = Transcript::default();
    let mut window: Vec<f32> = Vec::with_capacity(4_000);
    for sample in reader.samples::<i16>() {
        let s = sample.context("read wav samples")?;
        window.push(s as f32 / 32_768.0);
        if window.len() == 4_000 {
            chunker.push(&window);
            window.clear();
            while let Some(chunk) = chunker.next_chunk() {
                handle_chunk(client, &mut transcript, language, chunk).await?;
            }
        }
    }
    if !window.is_empty() {
        chunker.push(&window);
    }
    while let Some(chunk) = chunker.next_chunk() {
        handle_chunk(client, &mut transcript, language, chunk).await?;
    }
    if let Some(chunk) = chunker.flush() {
        handle_chunk(client, &mut transcript, language, chunk).await?;
    }
    Ok(transcript)
}

async fn from_mic(
    client: &OpenAiClient,
    minutes: u64,
    language: &Option<String>,
) -> anyhow::Result<Transcript> {
    let mut chunker = Chunker::new(ChunkerConfig::default());
    let mut transcript = Transcript::default();
    // Recorder holds a !Send cpal stream; the future is driven on the main
    // thread by block_on, so Send is not required.
    let recorder = Recorder::start(None).map_err(|err| anyhow::anyhow!("mic: {err}"))?;
    eprintln!(
        "recording from {} for {minutes} minutes…",
        recorder.device_name()
    );
    let t0 = Instant::now();
    let limit = Duration::from_secs(minutes * 60);
    while t0.elapsed() < limit {
        std::thread::sleep(Duration::from_millis(250));
        chunker.push(&recorder.take_chunk());
        let mut ready = Vec::new();
        while let Some(chunk) = chunker.next_chunk() {
            ready.push(chunk);
        }
        for chunk in ready {
            handle_chunk(client, &mut transcript, language, chunk).await?;
        }
    }
    drop(recorder);
    if let Some(chunk) = chunker.flush() {
        handle_chunk(client, &mut transcript, language, chunk).await?;
    }
    Ok(transcript)
}

pub async fn run(
    minutes: u64,
    wav: Option<PathBuf>,
    asr_url: String,
    asr_model: String,
    language: Option<String>,
    out: Option<PathBuf>,
) -> anyhow::Result<()> {
    let client = OpenAiClient::new(ClientConfig {
        base_url: asr_url,
        model: asr_model,
        api_key: std::env::var("OB_ASR_API_KEY").ok(),
        connect_timeout_ms: 4_000,
        total_timeout_ms: 120_000,
    })?;
    let transcript = match wav {
        Some(path) => from_wav(&client, &path, &language).await?,
        None => from_mic(&client, minutes, &language).await?,
    };
    let md = transcript.to_markdown("Meeting");
    if let Some(path) = out {
        std::fs::write(&path, md)?;
        eprintln!("wrote {}", path.display());
    }
    eprintln!("{} segments", transcript.segments.len());
    Ok(())
}
