use ob_ipc::UpdateInfo;
use tauri_plugin_updater::UpdaterExt;

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            current,
            available: Some(update.version),
            notes: update.body,
        }),
        Ok(None) => Ok(UpdateInfo {
            current,
            available: None,
            notes: None,
        }),
        Err(e) => Err(format!("Could not check for updates: {e}")),
    }
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            update
                .download_and_install(|_chunk, _total| {}, || {})
                .await
                .map_err(|e| e.to_string())?;
            app.restart()
        }
        Ok(None) => Err("Already up to date".to_string()),
        Err(e) => Err(format!("Could not check for updates: {e}")),
    }
}
