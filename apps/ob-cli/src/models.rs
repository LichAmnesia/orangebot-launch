//! `ob models` — catalog listing and downloads.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};

const CATALOG_JSON: &str = include_str!("../../../crates/ob-models/data/model-catalog.json");

fn catalog() -> anyhow::Result<ob_models::Catalog> {
    Ok(ob_models::Catalog::from_json_str(CATALOG_JSON)?)
}

fn gb(bytes: u64) -> f64 {
    bytes as f64 / 1e9
}

pub fn list() -> anyhow::Result<()> {
    let catalog = catalog()?;
    println!("RECIPES");
    for recipe in &catalog.recipes {
        let size = catalog
            .recipe_total_bytes(&recipe.id)
            .with_context(|| format!("recipe {}", recipe.id))?;
        println!(
            "{:<24} asr={:<18} text={:<14} {:>6.2} GB  ram {} GiB  {}",
            recipe.id,
            recipe.asr,
            recipe.text,
            gb(size),
            recipe.target_ram_gib,
            recipe.qualification_status,
        );
    }
    println!();
    println!("MODELS");
    for model in &catalog.models {
        let size: u64 = model.assets.iter().map(|a| a.bytes).sum();
        println!(
            "{:<18} {:<5} {:<12} {:>6.2} GB  {}",
            model.id,
            model.role,
            model.license,
            gb(size),
            model.qualification_status,
        );
    }
    Ok(())
}

pub async fn download(model_id: String, dir: Option<PathBuf>) -> anyhow::Result<()> {
    let catalog = catalog()?;
    let Some(model) = catalog.model(&model_id) else {
        bail!("unknown model {model_id}");
    };
    let dest = dir.unwrap_or_else(|| crate::data_dir().join("models").join(model_id.clone()));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()?;
    for asset in &model.assets {
        eprintln!(
            "downloading {} ({:.2} GB)",
            asset.relative_path,
            gb(asset.bytes)
        );
        let mut last_pct: Option<u64> = None;
        let path = ob_models::download_asset(&client, asset, &dest, |p| {
            let pct = if p.total == 0 {
                100
            } else {
                (p.downloaded * 100 / p.total).min(100)
            };
            if last_pct != Some(pct) {
                last_pct = Some(pct);
                eprint!(
                    "\r  {:>3}%  {}/{} MB",
                    pct,
                    p.downloaded / 1_000_000,
                    p.total / 1_000_000
                );
            }
        })
        .await
        .with_context(|| format!("download {}", asset.relative_path))?;
        println!();
        std::io::stdout().flush()?;
        println!("verified {}", path.display());
    }
    Ok(())
}
