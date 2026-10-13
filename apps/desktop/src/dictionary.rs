//! Preferred-spelling dictionary commands.

use std::time::{SystemTime, UNIX_EPOCH};

use ob_ipc::DictEntryInfo;
use ob_storage::{DictEntry, Store};

fn store() -> Result<Store, String> {
    let dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Store::open(&ob_domain::paths::history_db()).map_err(|err| err.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn info_of(entry: DictEntry) -> DictEntryInfo {
    DictEntryInfo {
        id: entry.id,
        phrase: entry.phrase,
        aliases: entry.aliases,
        source: entry.source,
        enabled: entry.enabled,
    }
}

#[tauri::command]
pub fn list_dictionary() -> Result<Vec<DictEntryInfo>, String> {
    let entries = store()?.list_dict(false).map_err(|err| err.to_string())?;
    Ok(entries.into_iter().map(info_of).collect())
}

#[tauri::command]
pub fn add_dictionary_entry(phrase: String, aliases: Vec<String>) -> Result<i64, String> {
    let phrase = phrase.trim().to_string();
    if phrase.is_empty() {
        return Err("Enter a word or phrase".to_string());
    }
    let aliases: Vec<String> = aliases
        .into_iter()
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
        .collect();
    let store = store()?;
    store
        .add_dict_entry(&phrase, &aliases, "manual", now_ms())
        .map_err(|err| {
            let msg = err.to_string();
            if msg.contains("UNIQUE") {
                "Already in your dictionary".to_string()
            } else {
                msg
            }
        })
}

#[tauri::command]
pub fn set_dictionary_enabled(id: i64, enabled: bool) -> Result<bool, String> {
    store()?
        .set_dict_enabled(id, enabled)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn delete_dictionary_entry(id: i64) -> Result<bool, String> {
    store()?
        .delete_dict_entry(id)
        .map_err(|err| err.to_string())
}
