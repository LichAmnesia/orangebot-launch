use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ob_platform::TargetToken;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[cfg(target_os = "macos")]
use ob_platform_macos::keytap::{self, KeyEvent, SoloKey};

const CANCEL_KEY: &str = "Escape";
const EDGE_DEBOUNCE: Duration = Duration::from_millis(350);
static LAST_EDGE: Mutex<Option<Instant>> = Mutex::new(None);

#[cfg(target_os = "macos")]
static SOLO: LazyLock<Mutex<Vec<(SoloKey, VoiceMode)>>> = LazyLock::new(|| Mutex::new(Vec::new()));
#[cfg(target_os = "macos")]
static TAP_STARTED: OnceLock<()> = OnceLock::new();
#[cfg(target_os = "macos")]
static PERMISSION_REQUESTED: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Vec<ob_ipc::HotkeyStatus>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    Dictate,
    Translate,
    Ask,
    Rewrite,
}

#[derive(Default)]
pub struct VoiceState {
    active: Mutex<Option<Active>>,
    generation: AtomicU64,
}

struct Active {
    handle: crate::recording::RecordingHandle,
    target: Option<TargetToken>,
    mode: VoiceMode,
    solo: bool,
    bar_shown: bool,
    started: Instant,
    selection: Option<CapturedSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedSelection {
    pub text: String,
    pub editable: bool,
}

#[cfg(target_os = "macos")]
pub fn capture_selection() -> Option<CapturedSelection> {
    ob_platform_macos::selection::selected_text().map(|s| CapturedSelection {
        text: s.text,
        editable: s.editable,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn capture_selection() -> Option<CapturedSelection> {
    None
}

fn should_ignore(last: Option<Instant>, now: Instant) -> bool {
    last.is_some_and(|last| {
        now.checked_duration_since(last)
            .is_none_or(|elapsed| elapsed < EDGE_DEBOUNCE)
    })
}

fn mark_edge() {
    if let Ok(mut guard) = LAST_EDGE.lock() {
        *guard = Some(Instant::now());
    }
}

pub fn on_press(app: &tauri::AppHandle, mode: VoiceMode, solo: bool) {
    let last_edge = LAST_EDGE.lock().ok().and_then(|guard| *guard);
    if should_ignore(last_edge, Instant::now()) {
        return;
    }
    let state = app.state::<VoiceState>();
    let toggle = settings_activation() == "toggle";
    let Ok(mut guard) = state.active.lock() else {
        return;
    };
    if guard.is_some() {
        drop(guard);
        if toggle {
            mark_edge();
            stop_recording(app);
        }
        return;
    }

    #[cfg(target_os = "macos")]
    let target = {
        use ob_platform::Platform;
        ob_platform_macos::MacPlatform::new().capture_target().ok()
    };
    #[cfg(not(target_os = "macos"))]
    let target: Option<TargetToken> = None;

    #[cfg(target_os = "macos")]
    let selection = match mode {
        VoiceMode::Ask => capture_selection(),
        VoiceMode::Rewrite => capture_selection().or_else(|| {
            ob_platform_macos::copy_selection().map(|text| CapturedSelection {
                text,
                editable: true,
            })
        }),
        _ => None,
    };
    #[cfg(not(target_os = "macos"))]
    let selection = (mode == VoiceMode::Ask).then(capture_selection).flatten();

    if mode == VoiceMode::Rewrite && selection.is_none() {
        drop(guard);
        state.generation.fetch_add(1, Ordering::SeqCst);
        show_voicebar(app);
        crate::pipeline::emit(
            app,
            "failed",
            0.0,
            None,
            Some("Select the text you want to rewrite first.".to_string()),
        );
        let generation = state.generation.load(Ordering::SeqCst);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1_500)).await;
            if app.state::<VoiceState>().generation.load(Ordering::SeqCst) == generation {
                hide_voicebar(&app);
            }
        });
        return;
    }

    state.generation.fetch_add(1, Ordering::SeqCst);
    if !solo {
        show_voicebar(app);
        crate::pipeline::emit(app, "recording", 0.0, None, None);
    }
    let app_for_level = app.clone();
    let handle =
        crate::recording::start_recording(crate::settings::load().input_device, move |level| {
            crate::pipeline::emit(&app_for_level, "recording", level, None, None);
        });
    mark_edge();
    *guard = Some(Active {
        handle,
        target,
        mode,
        solo,
        bar_shown: !solo,
        started: Instant::now(),
        selection,
    });
    drop(guard);
    register_cancel(app);
}

pub fn on_release(app: &tauri::AppHandle) {
    if settings_activation() == "toggle" {
        return;
    }
    let active = app
        .state::<VoiceState>()
        .active
        .lock()
        .is_ok_and(|guard| guard.is_some());
    if active {
        mark_edge();
    }
    stop_recording(app);
}

fn settings_activation() -> String {
    crate::settings::load().activation
}

fn stop_recording(app: &tauri::AppHandle) {
    unregister_cancel(app);
    let state = app.state::<VoiceState>();
    let Some(active) = state.active.lock().ok().and_then(|mut guard| guard.take()) else {
        return;
    };
    let Active {
        handle,
        target,
        mode,
        selection,
        ..
    } = active;
    let generation = state.generation.load(Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking(move || handle.finish()).await;
        match result {
            Ok(Ok(captured)) => {
                let _ =
                    crate::pipeline::process(app.clone(), captured, target, mode, selection).await;
            }
            Ok(Err(e)) => crate::pipeline::emit(&app, "failed", 0.0, None, Some(e)),
            Err(_) => crate::pipeline::emit(
                &app,
                "failed",
                0.0,
                None,
                Some("recorder task failed".to_string()),
            ),
        }
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        if app.state::<VoiceState>().generation.load(Ordering::SeqCst) == generation {
            hide_voicebar(&app);
        }
    });
}

pub fn on_cancel(app: &tauri::AppHandle) {
    unregister_cancel(app);
    let state = app.state::<VoiceState>();
    let Some(active) = state.active.lock().ok().and_then(|mut guard| guard.take()) else {
        return;
    };
    active.handle.cancel();
    crate::pipeline::emit(app, "cancelled", 0.0, None, None);
    let generation = state.generation.load(Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        if app.state::<VoiceState>().generation.load(Ordering::SeqCst) == generation {
            hide_voicebar(&app);
        }
    });
}

fn show_voicebar(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("voicebar") {
        let cursor = app.cursor_position().ok();
        let monitor = cursor
            .and_then(|cursor| {
                let monitors = window.available_monitors().ok()?;
                monitors.into_iter().find(|m| {
                    let pos = m.position();
                    let size = m.size();
                    let cx = cursor.x as i64;
                    let cy = cursor.y as i64;
                    cx >= i64::from(pos.x)
                        && cx < i64::from(pos.x) + i64::from(size.width)
                        && cy >= i64::from(pos.y)
                        && cy < i64::from(pos.y) + i64::from(size.height)
                })
            })
            .or(window.primary_monitor().ok().flatten());
        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let pos = monitor.position().to_logical::<f64>(scale);
            let size = monitor.size().to_logical::<f64>(scale);
            let _ = window.set_position(tauri::LogicalPosition::new(
                pos.x + (size.width - 340.0) / 2.0,
                pos.y + size.height - 150.0,
            ));
        }
        let _ = window.show();
    }
}

fn hide_voicebar(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("voicebar") {
        let _ = window.hide();
    }
}

pub fn register_all(app: &tauri::AppHandle) -> Result<(), String> {
    let _ = app.global_shortcut().unregister_all();
    clear_solo();
    let settings = crate::settings::load();
    let bindings = [
        (settings.hotkey, VoiceMode::Dictate),
        (settings.translate_hotkey, VoiceMode::Translate),
        (settings.ask_hotkey, VoiceMode::Ask),
        (settings.rewrite_hotkey, VoiceMode::Rewrite),
    ];
    let mut errors = Vec::new();
    let mut statuses = Vec::new();
    #[cfg(target_os = "macos")]
    let mut solo_bindings: Vec<(VoiceMode, String)> = Vec::new();
    for (hotkey, mode) in bindings {
        #[cfg(target_os = "macos")]
        if let Some(key) = SoloKey::parse(&hotkey) {
            if let Ok(mut solo) = SOLO.lock() {
                solo.push((key, mode));
            }
            solo_bindings.push((mode, hotkey));
            continue;
        }
        match Shortcut::from_str(&hotkey) {
            Ok(shortcut) => {
                let result =
                    app.global_shortcut()
                        .on_shortcut(shortcut, move |app, _shortcut, event| match event.state() {
                            ShortcutState::Pressed => on_press(app, mode, false),
                            ShortcutState::Released => on_release(app),
                        });
                match result {
                    Ok(()) => statuses.push(hotkey_status_entry(mode, hotkey, "active", "")),
                    Err(e) => {
                        errors.push(format!("{hotkey}: {e}"));
                        statuses.push(hotkey_status_entry(mode, hotkey, "error", &e.to_string()));
                    }
                }
            }
            Err(e) => {
                errors.push(format!("{hotkey}: {e}"));
                statuses.push(hotkey_status_entry(mode, hotkey, "error", &e.to_string()));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let solo_count = SOLO.lock().map(|solo| solo.len()).unwrap_or(0);
        if solo_count > 0 && TAP_STARTED.get().is_none() {
            if !keytap::input_monitoring_allowed()
                && !PERMISSION_REQUESTED.swap(true, Ordering::SeqCst)
            {
                keytap::request_input_monitoring();
            }
            let app_for_tap = app.clone();
            match keytap::start(move |event| handle_key(&app_for_tap, event)) {
                Ok(()) => {
                    let _ = TAP_STARTED.set(());
                }
                Err(e) => errors.push(e),
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let tap_running = TAP_STARTED.get().is_some();
        for (mode, hotkey) in solo_bindings {
            if tap_running {
                statuses.push(hotkey_status_entry(mode, hotkey, "active", ""));
            } else {
                statuses.push(hotkey_status_entry(
                    mode,
                    hotkey,
                    "needs_input_monitoring",
                    "Allow Launch in System Settings \u{2192} Privacy & Security \u{2192} Input Monitoring",
                ));
            }
        }
    }
    if let Ok(mut status) = STATUS.lock() {
        *status = statuses;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn mode_name(mode: VoiceMode) -> &'static str {
    match mode {
        VoiceMode::Dictate => "dictate",
        VoiceMode::Translate => "translate",
        VoiceMode::Ask => "ask",
        VoiceMode::Rewrite => "rewrite",
    }
}

fn hotkey_status_entry(
    mode: VoiceMode,
    hotkey: String,
    state: &str,
    detail: &str,
) -> ob_ipc::HotkeyStatus {
    ob_ipc::HotkeyStatus {
        mode: mode_name(mode).to_string(),
        hotkey,
        state: state.to_string(),
        detail: detail.to_string(),
    }
}

pub fn status() -> Vec<ob_ipc::HotkeyStatus> {
    STATUS
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default()
}

pub fn start_permission_watch(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if TAP_STARTED.get().is_some() {
                break;
            }
            let has_solo = SOLO.lock().is_ok_and(|solo| !solo.is_empty());
            if !has_solo {
                continue;
            }
            if keytap::input_monitoring_allowed() {
                let _ = register_all(&app);
                let _ = app.emit(ob_ipc::event::STATUS, ());
            }
        }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

pub fn clear_solo() {
    #[cfg(target_os = "macos")]
    if let Ok(mut solo) = SOLO.lock() {
        solo.clear();
    }
}

#[cfg(target_os = "macos")]
fn solo_mode_for(key: SoloKey) -> Option<VoiceMode> {
    SOLO.lock().ok().and_then(|solo| {
        solo.iter()
            .find(|(bound, _)| *bound == key)
            .map(|(_, mode)| *mode)
    })
}

#[cfg(target_os = "macos")]
fn handle_key(app: &tauri::AppHandle, event: KeyEvent) {
    match event {
        KeyEvent::SoloDown(key) => {
            let Some(mode) = solo_mode_for(key) else {
                return;
            };
            on_press(app, mode, true);
            let generation = app.state::<VoiceState>().generation.load(Ordering::SeqCst);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                let state = app.state::<VoiceState>();
                if state.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                let Ok(mut guard) = state.active.lock() else {
                    return;
                };
                let show = guard
                    .as_mut()
                    .is_some_and(|active| active.solo && !active.bar_shown);
                if show {
                    if let Some(active) = guard.as_mut() {
                        active.bar_shown = true;
                    }
                    drop(guard);
                    show_voicebar(&app);
                    crate::pipeline::emit(&app, "recording", 0.0, None, None);
                }
            });
        }
        KeyEvent::SoloUp(key) => {
            if solo_mode_for(key).is_some() {
                on_release(app);
            }
        }
        KeyEvent::OtherKeyDown => {
            let state = app.state::<VoiceState>();
            let Some(active) = state.active.lock().ok().and_then(|mut guard| {
                let cancel = guard.as_ref().is_some_and(|active| {
                    active.solo && active.started.elapsed() < Duration::from_millis(1_500)
                });
                if cancel { guard.take() } else { None }
            }) else {
                return;
            };
            unregister_cancel(app);
            active.handle.cancel();
            if active.bar_shown {
                hide_voicebar(app);
            }
        }
    }
}

fn register_cancel(app: &tauri::AppHandle) {
    let Ok(shortcut) = Shortcut::from_str(CANCEL_KEY) else {
        return;
    };
    let _ = app
        .global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                on_cancel(app);
            }
        });
}

fn unregister_cancel(app: &tauri::AppHandle) {
    if let Ok(shortcut) = Shortcut::from_str(CANCEL_KEY) {
        let _ = app.global_shortcut().unregister(shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_edge_within_window() {
        let now = Instant::now();
        let last = now - Duration::from_millis(100);
        assert!(should_ignore(Some(last), now));
    }

    #[test]
    fn allows_edge_at_or_after_window() {
        let now = Instant::now();
        let last = now - Duration::from_millis(350);
        assert!(!should_ignore(Some(last), now));
        let last = now - Duration::from_millis(400);
        assert!(!should_ignore(Some(last), now));
    }

    #[test]
    fn allows_first_edge() {
        assert!(!should_ignore(None, Instant::now()));
    }

    #[test]
    fn ignores_edge_when_clock_goes_backwards() {
        let now = Instant::now();
        let last = now + Duration::from_millis(10);
        assert!(should_ignore(Some(last), now));
    }
}
