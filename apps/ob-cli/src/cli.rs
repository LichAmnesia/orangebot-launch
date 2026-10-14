//! Command-line surface of the `ob` binary.

use std::path::PathBuf;

/// Entry point of the CLI.
#[derive(clap::Parser)]
#[command(
    name = "ob",
    version,
    about = "Orangebot Launch — local-model launcher and AI task tools"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Top level subcommands.
#[derive(clap::Subcommand)]
pub enum Command {
    /// List registered tools and the capabilities they need
    Tools,
    /// Model catalog and installs
    Models {
        #[command(subcommand)]
        cmd: ModelsCmd,
    },
    /// Test an OpenAI-compatible endpoint
    Provider {
        #[command(subcommand)]
        cmd: ProviderCmd,
    },
    /// List microphone input devices
    Devices,
    /// Record from the microphone into a 16 kHz mono WAV
    Record {
        #[arg(long, default_value_t = 5)]
        seconds: u64,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        device: Option<String>,
    },
    /// Record, transcribe, polish and print (optionally insert) text
    Dictate(DictateArgs),
    /// Show recent dictation history
    History {
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long)]
        query: Option<String>,
    },
    /// Transcribe a long recording as it happens (text only; audio is never saved)
    Meeting {
        #[arg(long, default_value_t = 60)]
        minutes: u64,
        #[arg(long)]
        wav: Option<PathBuf>,
        #[arg(long, env = "OB_ASR_URL")]
        asr_url: String,
        #[arg(long, env = "OB_ASR_MODEL", default_value = "whisper")]
        asr_model: String,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run a fixed evaluation suite against a text model
    Eval {
        suite: String,
        #[arg(long, env = "OB_LLM_URL")]
        llm_url: String,
        #[arg(long, env = "OB_LLM_MODEL")]
        llm_model: String,
        #[arg(long, default_value = "evals/cleanup.jsonl")]
        file: PathBuf,
        #[arg(long)]
        min_pass: Option<usize>,
    },
    /// End-to-end self-checks against real apps
    Selftest {
        #[command(subcommand)]
        cmd: SelftestCmd,
    },
}

/// `ob selftest` subcommands.
#[derive(clap::Subcommand)]
pub enum SelftestCmd {
    /// Insert text into real apps and verify it landed
    Insert {
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "textedit,notes,safari,chrome"
        )]
        apps: Vec<String>,
        #[arg(long, default_value_t = 5)]
        runs: u32,
    },
}

/// `ob models` subcommands.
#[derive(clap::Subcommand)]
pub enum ModelsCmd {
    List,
    Download {
        model_id: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

/// `ob provider` subcommands.
#[derive(clap::Subcommand)]
pub enum ProviderCmd {
    Test {
        #[arg(long)]
        base_url: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        asr: bool,
    },
}

/// Flags and environment-bound settings of `ob dictate`.
#[derive(clap::Args)]
pub struct DictateArgs {
    #[arg(long, default_value_t = 5)]
    pub seconds: u64,
    #[arg(long, default_value = "dictate")]
    pub mode: String,
    #[arg(long, default_value = "English")]
    pub target: String,
    #[arg(long, env = "OB_ASR_URL")]
    pub asr_url: String,
    #[arg(long, env = "OB_ASR_MODEL", default_value = "whisper")]
    pub asr_model: String,
    #[arg(long, env = "OB_LLM_URL")]
    pub llm_url: String,
    #[arg(long, env = "OB_LLM_MODEL")]
    pub llm_model: String,
    #[arg(long)]
    pub language: Option<String>,
    #[arg(long)]
    pub device: Option<String>,
    #[arg(long)]
    pub insert: bool,
    #[arg(long)]
    pub wav: Option<PathBuf>,
}
