mod audio;
mod cli;
mod dictate;
mod eval;
mod history;
mod meeting;
mod models;
mod provider;
mod selftest;

use std::path::PathBuf;

use clap::Parser;
use ob_tools::ToolRegistry;

use cli::{Cli, Command, ModelsCmd, ProviderCmd, SelftestCmd};

/// Root directory for the app's local data.
#[allow(dead_code)]
pub(crate) fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("OB_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/orangebot")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Tools => {
            let mut registry = ToolRegistry::new();
            registry.register(Box::new(ob_type::TypeTool))?;
            for manifest in registry.list() {
                let requires = manifest
                    .requires
                    .iter()
                    .map(|cap| format!("{cap:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "{}\t{} {}\trequires: {requires}",
                    manifest.id, manifest.name, manifest.version
                );
            }
            Ok(())
        }
        Command::Models { cmd } => match cmd {
            ModelsCmd::List => models::list(),
            ModelsCmd::Download { model_id, dir } => models::download(model_id, dir).await,
        },
        Command::Provider { cmd } => match cmd {
            ProviderCmd::Test {
                base_url,
                model,
                api_key_env,
                asr,
            } => provider::test(base_url, model, api_key_env, asr).await,
        },
        Command::Devices => audio::devices(),
        Command::Record {
            seconds,
            out,
            device,
        } => audio::record(seconds, &out, device.as_deref()),
        Command::Dictate(args) => dictate::run(args).await,
        Command::History { limit, query } => history::show(limit, query.as_deref()),
        Command::Meeting {
            minutes,
            wav,
            asr_url,
            asr_model,
            language,
            out,
        } => meeting::run(minutes, wav, asr_url, asr_model, language, out).await,
        Command::Eval {
            suite,
            llm_url,
            llm_model,
            file,
            min_pass,
        } => eval::run(suite, llm_url, llm_model, file, min_pass).await,
        Command::Selftest { cmd } => match cmd {
            SelftestCmd::Insert { apps, runs } => selftest::insert(apps, runs),
        },
    }
}
