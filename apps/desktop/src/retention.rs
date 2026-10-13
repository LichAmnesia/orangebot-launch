use std::time::{SystemTime, UNIX_EPOCH};

use ob_storage::Store;

fn store() -> Result<Store, String> {
    let dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Store::open(&ob_domain::paths::history_db()).map_err(|e| e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tauri::command]
pub fn delete_all_history() -> Result<usize, String> {
    store()?
        .purge_sessions_before(i64::MAX)
        .map_err(|e| e.to_string())
}

pub fn purge_expired() -> Result<usize, String> {
    let settings = crate::settings::load();
    if settings.retention_days == 0 {
        return Ok(0);
    }
    let cutoff = now_ms() - i64::from(settings.retention_days) * 86_400_000;
    let store = store()?;
    let mut purged = store
        .purge_sessions_before(cutoff)
        .map_err(|e| e.to_string())?;
    if settings.meetings_follow_retention {
        purged += store
            .purge_meetings_before(cutoff)
            .map_err(|e| e.to_string())?;
    }
    Ok(purged)
}
