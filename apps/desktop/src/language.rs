#[tauri::command]
pub fn get_ui_language() -> Result<String, String> {
    Ok(crate::settings::load().ui_language)
}

#[tauri::command]
pub fn set_ui_language(lang: String) -> Result<(), String> {
    if !matches!(lang.as_str(), "system" | "en" | "zh-CN") {
        return Err("Unsupported language".to_string());
    }
    let mut settings = crate::settings::load();
    settings.ui_language = lang;
    crate::settings::save(&settings).map_err(|e| e.to_string())
}
