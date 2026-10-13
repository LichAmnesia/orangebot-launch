use std::sync::Mutex;

use ob_platform::TargetToken;
use tauri::Manager;

static LAST: Mutex<Option<TargetToken>> = Mutex::new(None);

pub fn remember(target: Option<TargetToken>) {
    if let Ok(mut guard) = LAST.lock() {
        *guard = target;
    }
}

#[tauri::command]
pub fn replace_selection(app: tauri::AppHandle, text: String) -> Result<String, String> {
    if let Some(window) = app.get_webview_window("answer") {
        let _ = window.hide();
    }
    let stored = LAST.lock().ok().and_then(|mut guard| guard.take());
    #[cfg(target_os = "macos")]
    {
        use ob_platform::Platform;
        let _ = app.hide();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let platform = ob_platform_macos::MacPlatform::new();
        if let Some(target) = stored {
            match ob_platform::deliver(&platform, &target, &text) {
                Ok(_) => Ok("inserted".to_string()),
                Err(_) => {
                    let _ = platform.copy_to_clipboard(&text);
                    Ok("copied".to_string())
                }
            }
        } else {
            let _ = platform.copy_to_clipboard(&text);
            Ok("copied".to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (stored, app);
        Ok("copied".to_string())
    }
}
