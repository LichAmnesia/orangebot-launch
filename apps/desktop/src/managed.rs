use std::path::PathBuf;

use ob_models::{builtin_catalog, model_dir, model_installed};
use ob_runtime::{ManagedService, ServiceSpec};
use tauri::Emitter;

pub const ASR_PORT: u16 = 17_860;
pub const LLM_PORT: u16 = 17_861;

#[derive(Default)]
pub struct Managed {
    asr: tokio::sync::Mutex<Option<ManagedService>>,
    llm: tokio::sync::Mutex<Option<ManagedService>>,
}

pub fn asr_args(model: &str) -> Vec<String> {
    vec![
        "--model".to_string(),
        model.to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        ASR_PORT.to_string(),
    ]
}

pub fn llm_args(model: &str) -> Vec<String> {
    vec![
        "-m".to_string(),
        model.to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        LLM_PORT.to_string(),
        "-c".to_string(),
        "8192".to_string(),
        "-ngl".to_string(),
        "99".to_string(),
        "--jinja".to_string(),
    ]
}

fn sidecar(name: &str) -> Result<PathBuf, String> {
    let path = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or_else(|| "cannot resolve app directory".to_string())?
        .join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{name} is missing next to the app — run scripts/build-sidecars.sh"
        ))
    }
}

async fn start_service(
    slot: &tokio::sync::Mutex<Option<ManagedService>>,
    spec: ServiceSpec,
) -> Result<(), String> {
    let mut guard = slot.lock().await;
    if let Some(old) = guard.take() {
        old.stop().await;
    }
    let service = ManagedService::start(spec)
        .await
        .map_err(|e| e.to_string())?;
    *guard = Some(service);
    Ok(())
}

impl Managed {
    pub async fn start_recipe(
        &self,
        app: &tauri::AppHandle,
        recipe_id: &str,
    ) -> Result<(), String> {
        let catalog = builtin_catalog();
        let recipe = catalog
            .recipe(recipe_id)
            .ok_or_else(|| "unknown recipe".to_string())?;
        let asr_model = catalog
            .model(&recipe.asr)
            .ok_or_else(|| "unknown recipe".to_string())?;
        let text_model = catalog
            .model(&recipe.text)
            .ok_or_else(|| "unknown recipe".to_string())?;
        let root = ob_domain::paths::models_dir();

        if !model_installed(&root, asr_model) || !model_installed(&root, text_model) {
            return Err("Download the recipe's models first".to_string());
        }
        if asr_model.runtime_id != "whisper_cpp" {
            return Err("this speech model needs a runtime that is not available yet".to_string());
        }

        let asr_path = model_dir(&root, &asr_model.id)
            .join(&asr_model.assets[0].relative_path)
            .to_string_lossy()
            .into_owned();
        let text_path = model_dir(&root, &text_model.id)
            .join(&text_model.assets[0].relative_path)
            .to_string_lossy()
            .into_owned();

        start_service(
            &self.asr,
            ServiceSpec {
                name: "asr".to_string(),
                program: sidecar("ob-asr-server")?,
                args: asr_args(&asr_path),
                host: "127.0.0.1".to_string(),
                port: ASR_PORT,
                health_path: "/health".to_string(),
                startup_timeout_ms: 60_000,
                log_path: Some(ob_domain::paths::data_dir().join("logs").join("asr.log")),
            },
        )
        .await?;

        start_service(
            &self.llm,
            ServiceSpec {
                name: "llm".to_string(),
                program: sidecar("llama-server")?,
                args: llm_args(&text_path),
                host: "127.0.0.1".to_string(),
                port: LLM_PORT,
                health_path: "/health".to_string(),
                startup_timeout_ms: 180_000,
                log_path: Some(ob_domain::paths::data_dir().join("logs").join("llm.log")),
            },
        )
        .await?;

        let mut settings = crate::settings::load();
        settings.asr = Some(crate::settings::Connection {
            base_url: format!("http://127.0.0.1:{ASR_PORT}/v1"),
            model: asr_model.id.clone(),
            has_key: false,
        });
        settings.text = Some(crate::settings::Connection {
            base_url: format!("http://127.0.0.1:{LLM_PORT}/v1"),
            model: text_model.id.clone(),
            has_key: false,
        });
        settings.managed_recipe = Some(recipe_id.to_string());
        crate::settings::save(&settings).map_err(|e| e.to_string())?;
        let _ = app.emit(ob_ipc::event::STATUS, ());
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn stop_all(&self) {
        if let Some(service) = self.asr.lock().await.take() {
            service.stop().await;
        }
        if let Some(service) = self.llm.lock().await.take() {
            service.stop().await;
        }
    }
}

#[tauri::command]
pub async fn use_recipe(
    app: tauri::AppHandle,
    state: tauri::State<'_, Managed>,
    recipe_id: String,
) -> Result<(), String> {
    state.start_recipe(&app, &recipe_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asr_args_pass_the_port_flag() {
        let args = asr_args("/models/asr.ggml");
        let idx = args.iter().position(|a| a == "--port").unwrap();
        assert_eq!(args[idx + 1], "17860");
        assert_eq!(args[5], "17860");
        assert_eq!(args[1], "/models/asr.ggml");
    }

    #[test]
    fn llm_args_pass_the_port_flag() {
        let args = llm_args("/models/llm.gguf");
        let idx = args.iter().position(|a| a == "--port").unwrap();
        assert_eq!(args[idx + 1], "17861");
        assert_eq!(args[5], "17861");
        assert_eq!(args[1], "/models/llm.gguf");
        assert_eq!(args.last().unwrap(), "--jinja");
    }
}
