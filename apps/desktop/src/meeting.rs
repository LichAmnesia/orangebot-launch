//! Live meeting transcription. Audio chunks live in memory only: they are
//! encoded, transcribed and dropped. Only text is ever written to disk.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ob_ipc::{MeetingDetail, MeetingEvent, MeetingInfo, MeetingSegment, event};
use ob_meeting::{Chunk, Chunker, ChunkerConfig, Transcript};
use ob_providers::{ClientConfig, OpenAiClient};
use ob_storage::{MeetingRow, Store};
use tauri::{AppHandle, Emitter, State};

use crate::secrets;

/// Shared handle to the one meeting that may be running.
#[derive(Default)]
pub struct MeetingState {
    active: std::sync::Mutex<Option<ActiveMeeting>>,
}

struct ActiveMeeting {
    #[expect(
        dead_code,
        reason = "kept to identify the running meeting in diagnostics"
    )]
    id: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const SAMPLE_RATE: u32 = 16_000;
const TICK: Duration = Duration::from_millis(250);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn store() -> Result<Store, String> {
    let dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Store::open(&ob_domain::paths::history_db()).map_err(|err| err.to_string())
}

#[allow(clippy::too_many_arguments)]
fn emit(
    app: &AppHandle,
    meeting_id: &str,
    state: &str,
    level: f32,
    elapsed_ms: u64,
    segment: Option<MeetingSegment>,
    error: Option<String>,
) {
    let _ = app.emit(
        event::MEETING,
        MeetingEvent {
            meeting_id: meeting_id.to_string(),
            state: state.to_string(),
            level,
            elapsed_ms,
            segment,
            error,
        },
    );
}

fn info_of(row: MeetingRow) -> MeetingInfo {
    MeetingInfo {
        id: row.id,
        title: row.title,
        started_at_ms: row.started_at_ms,
        ended_at_ms: row.ended_at_ms,
        segment_count: row.segment_count as u32,
        preview: row.preview,
    }
}

fn find_info(id: &str) -> Result<MeetingInfo, String> {
    store()?
        .list_meetings(1_000)
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(info_of)
        .find(|info| info.id == id)
        .ok_or_else(|| "not found".to_string())
}

/// Hint for the transcriber: preferred spellings plus recent context.
fn prompt_for(terms: &[String], transcript: &Transcript) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if !terms.is_empty() {
        parts.push(terms.join(", "));
    }
    let tail = transcript.tail(160);
    if !tail.is_empty() {
        parts.push(tail);
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

#[tauri::command]
pub fn start_meeting(
    app: AppHandle,
    state: State<'_, MeetingState>,
    title: Option<String>,
) -> Result<String, String> {
    if state
        .active
        .lock()
        .map_err(|_| "meeting state poisoned")?
        .is_some()
    {
        return Err("A meeting is already being transcribed".to_string());
    }

    let settings = crate::settings::load();
    let asr = settings
        .asr
        .ok_or_else(|| "Set up a speech model first".to_string())?;

    let started_at = now_ms();
    let id = format!("m{started_at}");
    let title = title.unwrap_or_else(|| format!("Meeting {started_at}"));
    store()?
        .create_meeting(&id, &title, started_at as i64)
        .map_err(|err| err.to_string())?;

    let (tx, rx) = std::sync::mpsc::channel::<Chunk>();
    let rx = Arc::new(std::sync::Mutex::new(rx));
    let stop = Arc::new(AtomicBool::new(false));

    // Transcriber: drains chunks, keeps only text.
    {
        let app = app.clone();
        let id = id.clone();
        let rx = Arc::clone(&rx);
        let stop = Arc::clone(&stop);
        tauri::async_runtime::spawn(async move {
            let client = match OpenAiClient::new(ClientConfig {
                base_url: asr.base_url,
                model: asr.model,
                api_key: secrets::get_key("asr"),
                connect_timeout_ms: 4_000,
                total_timeout_ms: 120_000,
            }) {
                Ok(client) => client,
                Err(err) => {
                    emit(&app, &id, "failed", 0.0, 0, None, Some(err.to_string()));
                    stop.store(true, Ordering::SeqCst);
                    return;
                }
            };
            let store = match store() {
                Ok(store) => store,
                Err(err) => {
                    emit(&app, &id, "failed", 0.0, 0, None, Some(err));
                    stop.store(true, Ordering::SeqCst);
                    return;
                }
            };
            let terms: Vec<String> = store
                .list_dict(true)
                .map(|entries| entries.into_iter().map(|entry| entry.phrase).collect())
                .unwrap_or_default();

            let started = Instant::now();
            let mut transcript = Transcript::default();
            loop {
                let pull = {
                    let rx = Arc::clone(&rx);
                    tauri::async_runtime::spawn_blocking(move || {
                        rx.lock().ok().map(|guard| guard.recv())
                    })
                    .await
                };
                let Some(chunk) = pull.ok().flatten().and_then(|received| received.ok()) else {
                    break;
                };
                if chunk.is_silent {
                    continue;
                }
                let Chunk {
                    samples,
                    start_ms,
                    end_ms,
                    is_silent: _,
                } = chunk;
                emit(
                    &app,
                    &id,
                    "transcribing",
                    0.0,
                    started.elapsed().as_millis() as u64,
                    None,
                    None,
                );
                let wav = ob_audio::encode_wav_mono_i16(&samples, SAMPLE_RATE);
                drop(samples);
                let prompt = prompt_for(&terms, &transcript);
                match client
                    .transcribe_with_prompt(wav, None, prompt.as_deref())
                    .await
                {
                    Ok(text) => {
                        if let Some(segment) = transcript.push(start_ms, end_ms, &text) {
                            let _ = store.append_segment(
                                &id,
                                segment.seq as i64,
                                segment.start_ms as i64,
                                segment.end_ms as i64,
                                &segment.text,
                            );
                            emit(
                                &app,
                                &id,
                                "recording",
                                0.0,
                                started.elapsed().as_millis() as u64,
                                Some(MeetingSegment {
                                    seq: segment.seq,
                                    start_ms: segment.start_ms,
                                    end_ms: segment.end_ms,
                                    text: segment.text.clone(),
                                }),
                                None,
                            );
                        }
                    }
                    Err(err) => emit(
                        &app,
                        &id,
                        "failed",
                        0.0,
                        started.elapsed().as_millis() as u64,
                        None,
                        Some(err.to_string()),
                    ),
                }
            }

            let _ = store.finish_meeting(&id, now_ms() as i64);
            emit(&app, &id, "stopped", 0.0, 0, None, None);
        });
    }

    // Recorder: owns the !Send cpal stream, feeds the chunker.
    let recorder = {
        let app = app.clone();
        let id = id.clone();
        let stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("ob-meeting".to_string())
            .spawn(move || {
                let rec = match ob_audio::Recorder::start(
                    crate::settings::load().input_device.as_deref(),
                ) {
                    Ok(rec) => rec,
                    Err(err) => {
                        emit(&app, &id, "failed", 0.0, 0, None, Some(err.to_string()));
                        return;
                    }
                };
                let mut chunker = Chunker::new(ChunkerConfig::default());
                let started = Instant::now();
                loop {
                    std::thread::sleep(TICK);
                    chunker.push(&rec.take_chunk());
                    emit(
                        &app,
                        &id,
                        "recording",
                        rec.level(),
                        started.elapsed().as_millis() as u64,
                        None,
                        None,
                    );
                    while let Some(chunk) = chunker.next_chunk() {
                        if tx.send(chunk).is_err() {
                            break;
                        }
                    }
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                }
                chunker.push(&rec.take_chunk());
                if let Some(chunk) = chunker.flush() {
                    let _ = tx.send(chunk);
                }
                drop(rec);
                drop(tx);
            })
            .map_err(|err| err.to_string())?
    };

    let mut guard = state.active.lock().map_err(|_| "meeting state poisoned")?;
    if guard.is_some() {
        stop.store(true, Ordering::SeqCst);
        return Err("A meeting is already being transcribed".to_string());
    }
    *guard = Some(ActiveMeeting {
        id: id.clone(),
        stop,
        thread: Some(recorder),
    });
    Ok(id)
}

#[tauri::command]
pub fn stop_meeting(state: State<'_, MeetingState>) -> Result<(), String> {
    let taken = state
        .active
        .lock()
        .map_err(|_| "meeting state poisoned")?
        .take();
    let Some(mut active) = taken else {
        return Err("No meeting is being transcribed".to_string());
    };
    active.stop.store(true, Ordering::SeqCst);
    if let Some(thread) = active.thread.take() {
        std::thread::spawn(move || {
            let _ = thread.join();
        });
    }
    Ok(())
}

#[tauri::command]
pub fn list_meetings(limit: u32) -> Result<Vec<MeetingInfo>, String> {
    let rows = store()?
        .list_meetings(limit)
        .map_err(|err| err.to_string())?;
    Ok(rows.into_iter().map(info_of).collect())
}

#[tauri::command]
pub fn get_meeting(id: String) -> Result<MeetingDetail, String> {
    let info = find_info(&id)?;
    let rows = store()?
        .meeting_segments(&id)
        .map_err(|err| err.to_string())?;
    let segments = rows
        .into_iter()
        .map(|row| MeetingSegment {
            seq: row.seq as u32,
            start_ms: row.start_ms as u64,
            end_ms: row.end_ms as u64,
            text: row.text,
        })
        .collect();
    Ok(MeetingDetail { info, segments })
}

#[tauri::command]
pub fn delete_meeting(id: String) -> Result<bool, String> {
    store()?.delete_meeting(&id).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn meeting_markdown(id: String) -> Result<String, String> {
    let info = find_info(&id)?;
    let rows = store()?
        .meeting_segments(&id)
        .map_err(|err| err.to_string())?;
    let mut transcript = Transcript::default();
    for row in rows {
        transcript.push(row.start_ms as u64, row.end_ms as u64, &row.text);
    }
    Ok(transcript.to_markdown(&info.title))
}
