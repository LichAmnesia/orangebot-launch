use std::str::FromStr;

use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[tauri::command]
pub fn get_hotkeys() -> Result<ob_ipc::HotkeySettings, String> {
    let settings = crate::settings::load();
    Ok(ob_ipc::HotkeySettings {
        dictate: settings.hotkey,
        translate: settings.translate_hotkey,
        ask: settings.ask_hotkey,
        rewrite: settings.rewrite_hotkey,
        activation: settings.activation,
        translate_target: settings.translate_target,
    })
}

#[tauri::command]
pub fn set_hotkeys(app: tauri::AppHandle, hotkeys: ob_ipc::HotkeySettings) -> Result<(), String> {
    let bindings = [
        ("dictate", &hotkeys.dictate),
        ("translate", &hotkeys.translate),
        ("ask", &hotkeys.ask),
        ("rewrite", &hotkeys.rewrite),
    ];
    for (which, hotkey) in &bindings {
        #[cfg(target_os = "macos")]
        if ob_platform_macos::keytap::SoloKey::parse(hotkey).is_some() {
            continue;
        }
        Shortcut::from_str(hotkey).map_err(|e| format!("{which}: {e}"))?;
    }
    let lowered: Vec<String> = bindings
        .iter()
        .map(|(_, hotkey)| hotkey.to_lowercase())
        .collect();
    for (i, first) in lowered.iter().enumerate() {
        if lowered[i + 1..].iter().any(|other| other == first) {
            return Err("Each mode needs its own hotkey".to_string());
        }
    }
    if hotkeys.activation != "hold" && hotkeys.activation != "toggle" {
        return Err("activation must be hold or toggle".to_string());
    }
    let translate_target = hotkeys.translate_target.trim().to_string();
    if translate_target.is_empty() {
        return Err("translate target is required".to_string());
    }

    let mut settings = crate::settings::load();
    settings.hotkey = hotkeys.dictate;
    settings.translate_hotkey = hotkeys.translate;
    settings.ask_hotkey = hotkeys.ask;
    settings.rewrite_hotkey = hotkeys.rewrite;
    settings.activation = hotkeys.activation;
    settings.translate_target = translate_target;
    crate::settings::save(&settings).map_err(|e| e.to_string())?;

    crate::voice::register_all(&app)
}

#[tauri::command]
pub fn hotkey_status() -> Vec<ob_ipc::HotkeyStatus> {
    crate::voice::status()
}

#[tauri::command]
pub fn suspend_hotkeys(app: tauri::AppHandle) -> Result<(), String> {
    let _ = app.global_shortcut().unregister_all();
    crate::voice::clear_solo();
    Ok(())
}

#[tauri::command]
pub fn resume_hotkeys(app: tauri::AppHandle) -> Result<(), String> {
    crate::voice::register_all(&app)
}
