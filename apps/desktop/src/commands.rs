use ob_ipc::{AppStatus, EndpointStatus, HistoryItem, ModelInfo, ReadyState, RecipeInfo, ToolInfo};
use ob_models::{builtin_catalog, model_installed, model_total_bytes};
use ob_providers::{ClientConfig, OpenAiClient};
use ob_tools::ToolRegistry;

use crate::settings;

async fn endpoint_status(role: &str, conn: Option<settings::Connection>) -> EndpointStatus {
    let Some(conn) = conn else {
        return EndpointStatus {
            role: role.to_string(),
            base_url: None,
            model: None,
            state: ReadyState::NotConfigured,
        };
    };
    let state = match probe(&conn).await {
        Ok(()) => ReadyState::Ready,
        Err(_) => ReadyState::Unreachable,
    };
    EndpointStatus {
        role: role.to_string(),
        base_url: Some(conn.base_url),
        model: Some(conn.model),
        state,
    }
}

async fn probe(conn: &settings::Connection) -> Result<(), String> {
    let client = OpenAiClient::new(ClientConfig {
        base_url: conn.base_url.clone(),
        model: conn.model.clone(),
        api_key: None,
        connect_timeout_ms: 1_500,
        total_timeout_ms: 2_500,
    })
    .map_err(|e| e.to_string())?;
    client
        .list_models()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn app_status() -> Result<AppStatus, String> {
    let mut registry = ToolRegistry::new();
    registry
        .register(Box::new(ob_type::TypeTool))
        .map_err(|e| e.to_string())?;
    let tools = registry
        .list()
        .into_iter()
        .map(|manifest| ToolInfo {
            id: manifest.id.to_string(),
            name: manifest.name.to_string(),
            version: manifest.version.to_string(),
            requires: manifest
                .requires
                .iter()
                .map(|cap| format!("{cap:?}"))
                .collect(),
        })
        .collect();

    let settings = settings::load();
    Ok(AppStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        tools,
        asr: endpoint_status("asr", settings.asr).await,
        text: endpoint_status("text", settings.text).await,
        hotkey: settings.hotkey,
    })
}

#[tauri::command]
pub fn list_recipes() -> Vec<RecipeInfo> {
    let catalog = builtin_catalog();
    let models_root = ob_domain::paths::models_dir();
    catalog
        .recipes
        .iter()
        .map(|recipe| {
            let installed = [recipe.asr.as_str(), recipe.text.as_str()]
                .into_iter()
                .all(|id| {
                    catalog
                        .model(id)
                        .is_some_and(|model| model_installed(&models_root, model))
                });
            RecipeInfo {
                id: recipe.id.clone(),
                asr_model: recipe.asr.clone(),
                text_model: recipe.text.clone(),
                total_bytes: catalog.recipe_total_bytes(&recipe.id).unwrap_or(0),
                target_ram_gib: recipe.target_ram_gib,
                qualification: recipe.qualification_status.clone(),
                installed,
            }
        })
        .collect()
}

#[tauri::command]
pub fn list_models() -> Vec<ModelInfo> {
    let catalog = builtin_catalog();
    let models_root = ob_domain::paths::models_dir();
    catalog
        .models
        .iter()
        .map(|model| ModelInfo {
            id: model.id.clone(),
            role: model.role.clone(),
            license: model.license.clone(),
            total_bytes: model_total_bytes(model),
            installed: model_installed(&models_root, model),
        })
        .collect()
}

#[tauri::command]
pub fn list_history(limit: u32, query: Option<String>) -> Result<Vec<HistoryItem>, String> {
    let dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let store =
        ob_storage::Store::open(&ob_domain::paths::history_db()).map_err(|e| e.to_string())?;
    let sessions = store
        .list_sessions(limit, query.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(sessions
        .into_iter()
        .map(|session| HistoryItem {
            id: session.id,
            created_at_ms: session.created_at_ms,
            mode: session.mode,
            app: session.app,
            raw_text: session.raw_text,
            final_text: session.final_text,
            delivery: session.delivery,
            duration_ms: session.duration_ms,
        })
        .collect())
}
