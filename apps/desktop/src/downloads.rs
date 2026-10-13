use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Instant;

use ob_ipc::{DownloadEvent, DownloadState};
use tauri::Emitter;

struct Job {
    model_id: String,
    state: String,
    downloaded: u64,
    total: u64,
    speed_bps: u64,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
    delete_on_stop: bool,
}

static JOBS: LazyLock<Mutex<Vec<Job>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static WORKER_RUNNING: AtomicBool = AtomicBool::new(false);

fn jobs() -> MutexGuard<'static, Vec<Job>> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner())
}

fn snapshot() -> Vec<DownloadState> {
    jobs()
        .iter()
        .map(|job| DownloadState {
            model_id: job.model_id.clone(),
            state: job.state.clone(),
            downloaded: job.downloaded,
            total: job.total,
            speed_bps: job.speed_bps,
            error: job.error.clone(),
        })
        .collect()
}

fn emit_changed(app: &tauri::AppHandle) {
    let _ = app.emit(ob_ipc::event::DOWNLOADS, snapshot());
}

fn emit_final(
    app: &tauri::AppHandle,
    model_id: &str,
    downloaded: u64,
    total: u64,
    error: Option<String>,
) {
    let _ = app.emit(
        ob_ipc::event::DOWNLOAD,
        DownloadEvent {
            model_id: model_id.to_string(),
            downloaded,
            total,
            done: true,
            error,
        },
    );
}

pub fn enqueue(app: &tauri::AppHandle, model_id: &str) -> Result<(), String> {
    let catalog = ob_models::builtin_catalog();
    let model = catalog
        .model(model_id)
        .ok_or_else(|| format!("unknown model {model_id}"))?;
    let total = ob_models::model_total_bytes(model);
    {
        let mut guard = jobs();
        if guard.iter().any(|job| job.model_id == model_id) {
            return Ok(());
        }
        guard.push(Job {
            model_id: model_id.to_string(),
            state: "queued".to_string(),
            downloaded: 0,
            total,
            speed_bps: 0,
            error: None,
            cancel: Arc::new(AtomicBool::new(false)),
            delete_on_stop: false,
        });
    }
    emit_changed(app);
    ensure_worker(app);
    Ok(())
}

fn ensure_worker(app: &tauri::AppHandle) {
    if WORKER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        worker(app).await;
    });
}

async fn worker(app: tauri::AppHandle) {
    loop {
        let picked = {
            let mut guard = jobs();
            guard
                .iter_mut()
                .find(|job| job.state == "queued")
                .map(|job| {
                    job.state = "downloading".to_string();
                    job.error = None;
                    (job.model_id.clone(), job.cancel.clone(), job.total)
                })
        };
        let Some((model_id, cancel, total)) = picked else {
            WORKER_RUNNING.store(false, Ordering::SeqCst);
            let pending = jobs().iter().any(|job| job.state == "queued");
            if pending && !WORKER_RUNNING.swap(true, Ordering::SeqCst) {
                continue;
            }
            break;
        };
        emit_changed(&app);
        run_job(&app, &model_id, &cancel, total).await;
    }
}

async fn run_job(app: &tauri::AppHandle, model_id: &str, cancel: &AtomicBool, total: u64) {
    let catalog = ob_models::builtin_catalog();
    let Some(model) = catalog.model(model_id) else {
        set_job(model_id, |job| {
            job.state = "failed".to_string();
            job.error = Some(format!("unknown model {model_id}"));
        });
        emit_final(app, model_id, 0, total, Some("unknown model".to_string()));
        emit_changed(app);
        return;
    };
    let dest = ob_models::model_dir(&ob_domain::paths::models_dir(), model_id);
    let client = reqwest::Client::new();
    let assets: Vec<_> = model
        .assets
        .iter()
        .chain(model.runtime_assets.iter())
        .collect();

    let mut done_before: u64 = 0;
    let mut last_percent: i64 = -1;
    let mut window_start = Instant::now();
    let mut window_bytes: u64 = 0;
    let mut speed_bps: u64 = 0;
    let mut cancelled = false;
    let mut error: Option<String> = None;

    for asset in &assets {
        let base = done_before;
        let on_progress = |p: ob_models::DownloadProgress| {
            let done = base + p.downloaded;
            let elapsed = window_start.elapsed();
            if elapsed.as_millis() >= 1_000 {
                let delta = done.saturating_sub(window_bytes);
                speed_bps = (delta as f64 / elapsed.as_secs_f64().max(0.001)).round() as u64;
                window_start = Instant::now();
                window_bytes = done;
            }
            let percent = (done * 100 / total.max(1)) as i64;
            if percent != last_percent {
                last_percent = percent;
                set_job(model_id, |job| {
                    job.downloaded = done;
                    job.speed_bps = speed_bps;
                });
                let _ = app.emit(
                    ob_ipc::event::DOWNLOAD,
                    DownloadEvent {
                        model_id: model_id.to_string(),
                        downloaded: done,
                        total,
                        done: false,
                        error: None,
                    },
                );
                emit_changed(app);
            }
        };
        match ob_models::download_asset_cancellable(&client, asset, &dest, cancel, on_progress)
            .await
        {
            Ok(_) => done_before += asset.bytes,
            Err(e) => {
                cancelled = matches!(e, ob_models::DownloadError::Cancelled);
                if !cancelled {
                    error = Some(e.to_string());
                }
                break;
            }
        }
    }

    if cancelled {
        let delete = jobs()
            .iter()
            .find(|job| job.model_id == model_id)
            .is_some_and(|job| job.delete_on_stop);
        if delete {
            remove_part_files(&dest);
            jobs().retain(|job| job.model_id != model_id);
        } else {
            set_job(model_id, |job| {
                job.state = "paused".to_string();
                job.speed_bps = 0;
            });
        }
    } else if let Some(msg) = error {
        set_job(model_id, |job| {
            job.state = "failed".to_string();
            job.error = Some(msg.clone());
            job.speed_bps = 0;
        });
        emit_final(app, model_id, done_before, total, Some(msg));
    } else {
        set_job(model_id, |job| {
            job.state = "done".to_string();
            job.downloaded = job.total;
            job.speed_bps = 0;
        });
        emit_final(app, model_id, total, total, None);
    }
    emit_changed(app);
}

fn set_job(model_id: &str, update: impl FnOnce(&mut Job)) {
    if let Some(job) = jobs().iter_mut().find(|job| job.model_id == model_id) {
        update(job);
    }
}

fn remove_part_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_part_files(&path);
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".part"))
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[tauri::command]
pub fn list_downloads() -> Result<Vec<DownloadState>, String> {
    Ok(snapshot())
}

#[tauri::command]
pub fn pause_download(model_id: String) -> Result<(), String> {
    let mut guard = jobs();
    if let Some(job) = guard.iter_mut().find(|job| job.model_id == model_id) {
        job.delete_on_stop = false;
        job.cancel.store(true, Ordering::SeqCst);
        if job.state == "queued" {
            job.state = "paused".to_string();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn resume_download(app: tauri::AppHandle, model_id: String) -> Result<(), String> {
    {
        let mut guard = jobs();
        let Some(job) = guard.iter_mut().find(|job| job.model_id == model_id) else {
            return Err(format!("no download for {model_id}"));
        };
        if job.state != "paused" && job.state != "failed" {
            return Ok(());
        }
        job.cancel.store(false, Ordering::SeqCst);
        job.error = None;
        job.state = "queued".to_string();
    }
    emit_changed(&app);
    ensure_worker(&app);
    Ok(())
}

#[tauri::command]
pub fn cancel_download(model_id: String) -> Result<(), String> {
    let dest = ob_models::model_dir(&ob_domain::paths::models_dir(), &model_id);
    let mut guard = jobs();
    let Some(index) = guard.iter().position(|job| job.model_id == model_id) else {
        return Ok(());
    };
    if guard[index].state == "downloading" {
        guard[index].delete_on_stop = true;
        guard[index].cancel.store(true, Ordering::SeqCst);
        return Ok(());
    }
    guard.remove(index);
    drop(guard);
    remove_part_files(&dest);
    Ok(())
}
