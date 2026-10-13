use ob_ipc::GeneralSettings;
use tauri_plugin_autostart::ManagerExt;

use crate::settings;

const RETENTION_ALLOWED_DAYS: [u32; 5] = [0, 7, 30, 90, 365];

#[tauri::command]
pub fn get_general() -> Result<GeneralSettings, String> {
    let settings = settings::load();
    Ok(GeneralSettings {
        polish: settings.polish,
        launch_at_login: settings.launch_at_login,
        retention_days: settings.retention_days,
        meetings_follow_retention: settings.meetings_follow_retention,
    })
}

#[tauri::command]
pub fn set_general(app: tauri::AppHandle, general: GeneralSettings) -> Result<(), String> {
    let mut settings = settings::load();
    if !RETENTION_ALLOWED_DAYS.contains(&general.retention_days) {
        return Err("Unsupported retention".to_string());
    }
    if general.launch_at_login != settings.launch_at_login {
        let result = if general.launch_at_login {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        result.map_err(|e| e.to_string())?;
    }
    settings.polish = general.polish;
    settings.launch_at_login = general.launch_at_login;
    settings.retention_days = general.retention_days;
    settings.meetings_follow_retention = general.meetings_follow_retention;
    settings::save(&settings).map_err(|e| e.to_string())
}
