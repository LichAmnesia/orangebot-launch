use ob_ipc::{SearchInput, SearchSettingsInfo};
use ob_providers::{SearchClient, SearchKind};

use crate::secrets;
use crate::settings::{self, SearchSettings};

fn kind_of(kind: &str) -> Option<SearchKind> {
    match kind {
        "searxng" => Some(SearchKind::Searxng),
        "brave" => Some(SearchKind::Brave),
        _ => None,
    }
}

fn client_from(settings: &SearchSettings) -> Option<SearchClient> {
    let kind = kind_of(&settings.kind)?;
    SearchClient::new(kind, settings.base_url.clone(), secrets::get_key("search")).ok()
}

pub fn configured_client() -> Option<SearchClient> {
    let settings = settings::load().search?;
    client_from(&settings)
}

#[tauri::command]
pub fn get_search() -> Result<Option<SearchSettingsInfo>, String> {
    Ok(settings::load().search.map(|s| SearchSettingsInfo {
        kind: s.kind,
        base_url: s.base_url,
        has_key: s.has_key,
    }))
}

#[tauri::command]
pub fn save_search(input: SearchInput) -> Result<(), String> {
    let kind = input.kind.trim().to_string();
    let mut settings = settings::load();
    if kind.is_empty() {
        settings.search = None;
        secrets::delete_key("search");
        return settings::save(&settings).map_err(|e| e.to_string());
    }
    let search_kind = kind_of(&kind).ok_or("kind must be searxng or brave".to_string())?;
    let base_url = input.base_url.trim().to_string();
    if search_kind == SearchKind::Searxng
        && !(base_url.starts_with("http://") || base_url.starts_with("https://"))
    {
        return Err("base URL must start with http:// or https://".to_string());
    }
    let provided = input
        .api_key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    if let Some(key) = &provided {
        secrets::set_key("search", key)?;
    }
    let has_key = provided.is_some() || secrets::get_key("search").is_some();
    if search_kind == SearchKind::Brave && !has_key {
        return Err("Brave Search needs an API key".to_string());
    }
    settings.search = Some(SearchSettings {
        kind,
        base_url,
        has_key,
    });
    settings::save(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_search(query: String) -> Result<Vec<ob_ipc::Source>, String> {
    let client = configured_client().ok_or("web search is not configured".to_string())?;
    let hits = client.search(&query, 5).await.map_err(|e| e.to_string())?;
    Ok(hits
        .into_iter()
        .map(|hit| ob_ipc::Source {
            title: hit.title,
            url: hit.url,
        })
        .collect())
}
