mod http;
mod wav;
mod whisper;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use tokio::net::TcpListener;

use crate::http::AppState;
use crate::whisper::WhisperTranscriber;

#[derive(clap::Parser)]
struct Args {
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    model_id: Option<String>,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 17860)]
    port: u16,
    #[arg(long, default_value_t = 4)]
    threads: i32,
    #[arg(long)]
    allow_lan: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if !matches!(args.host.as_str(), "127.0.0.1" | "localhost" | "::1") && !args.allow_lan {
        anyhow::bail!(
            "refusing to bind {}: pass --allow-lan to expose the ASR server",
            args.host
        );
    }
    let model_id = args
        .model_id
        .clone()
        .or_else(|| {
            args.model
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .context("could not derive model id from model path")?;

    eprintln!("loading {}", args.model.display());
    let model_path = args.model.clone();
    let threads = args.threads;
    let transcriber =
        tokio::task::spawn_blocking(move || WhisperTranscriber::load(&model_path, threads))
            .await
            .context("transcriber task panicked")?
            .map_err(anyhow::Error::msg)?;

    let state = AppState {
        model_id,
        transcriber: Arc::new(transcriber),
    };
    let listener = TcpListener::bind((args.host.as_str(), args.port)).await?;
    eprintln!(
        "ob-asr-server listening on http://{}:{}",
        args.host, args.port
    );
    axum::serve(listener, http::router(state)).await?;
    Ok(())
}
