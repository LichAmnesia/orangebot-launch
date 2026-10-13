use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ob_ipc::{MicInfo, MicTest};

fn guarded_input_devices() -> Result<Vec<String>, String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(ob_audio::list_input_devices());
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(devices)) => Ok(devices),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(
            "The microphone list did not load — allow microphone access in System Settings"
                .to_string(),
        ),
    }
}

#[tauri::command]
pub fn list_mics() -> Result<MicInfo, String> {
    let devices = guarded_input_devices()?;
    Ok(MicInfo {
        devices,
        selected: crate::settings::load().input_device,
    })
}

#[tauri::command]
pub fn set_mic(name: Option<String>) -> Result<(), String> {
    let mut settings = crate::settings::load();
    match name.filter(|n| !n.is_empty()) {
        None => settings.input_device = None,
        Some(n) => {
            let devices = guarded_input_devices()?;
            if !devices.iter().any(|d| d == &n) {
                return Err("Unknown microphone".to_string());
            }
            settings.input_device = Some(n);
        }
    }
    crate::settings::save(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_mic() -> Result<MicTest, String> {
    let device = crate::settings::load().input_device;
    tauri::async_runtime::spawn_blocking(move || {
        let rec = ob_audio::Recorder::start(device.as_deref()).map_err(|e| e.to_string())?;
        let mut peak = 0.0f32;
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            peak = peak.max(rec.level());
            thread::sleep(Duration::from_millis(50));
        }
        let captured = rec.stop();
        Ok(MicTest {
            peak,
            duration_ms: captured.duration_ms,
            device: captured.device_name,
        })
    })
    .await
    .map_err(|_| "mic test task failed".to_string())?
}
