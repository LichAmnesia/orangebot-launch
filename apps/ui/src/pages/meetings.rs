use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};

use crate::i18n::pick;
use crate::ipc::{NoArgs, call, listen};
use ob_ipc::{MeetingDetail, MeetingEvent, MeetingInfo, cmd, event};

#[derive(serde::Serialize)]
struct StartArgs {
    title: Option<String>,
}

#[derive(serde::Serialize)]
struct ListArgs {
    limit: u32,
}

#[derive(serde::Serialize)]
struct IdArgs {
    id: String,
}

fn fmt_ms(ms: u64) -> String {
    let total = ms / 1000;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn fmt_date(ms: i64) -> String {
    js_sys::Date::new(&JsValue::from_f64(ms as f64))
        .to_locale_string("default", &JsValue::UNDEFINED)
        .into()
}

fn copy_to_clipboard(text: &str) -> bool {
    let global = js_sys::global();
    let Ok(navigator) = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    let Ok(clipboard) = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")) else {
        return false;
    };
    let Ok(write_text) = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText")) else {
        return false;
    };
    let Some(f) = write_text.dyn_ref::<js_sys::Function>() else {
        return false;
    };
    f.call1(&clipboard, &JsValue::from_str(text)).is_ok()
}

#[component]
pub fn MeetingsPage() -> impl IntoView {
    let active_id = RwSignal::new(None::<String>);
    let live = RwSignal::new(Vec::<ob_ipc::MeetingSegment>::new());
    let level = RwSignal::new(0f32);
    let elapsed = RwSignal::new(0u64);
    let status = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let version = RwSignal::new(0u32);
    let selected = RwSignal::new(None::<String>);
    let copied = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(listen::<MeetingEvent>(event::MEETING, move |ev| {
            status.set(ev.state.clone());
            level.set(ev.level);
            elapsed.set(ev.elapsed_ms);
            if let Some(seg) = ev.segment {
                live.update(|segs| {
                    if !segs.iter().any(|s| s.seq == seg.seq) {
                        segs.push(seg);
                    }
                });
            }
            if ev.state == "failed" {
                error.set(ev.error.clone());
            }
            if ev.state == "stopped" {
                active_id.set(None);
                version.update(|v| *v += 1);
            }
        }));
    });

    let meetings = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, Vec<MeetingInfo>>(cmd::LIST_MEETINGS, &ListArgs { limit: 50 }).await }
    });

    let detail = AsyncDerived::new_unsync(move || {
        let id = selected.get();
        async move {
            match id {
                Some(id) => Some(call::<_, MeetingDetail>(cmd::GET_MEETING, &IdArgs { id }).await),
                None => None,
            }
        }
    });

    let start = move |_| {
        spawn_local(async move {
            match call::<_, String>(cmd::START_MEETING, &StartArgs { title: None }).await {
                Ok(id) => {
                    error.set(None);
                    live.set(Vec::new());
                    elapsed.set(0);
                    level.set(0.0);
                    status.set("listening".to_string());
                    active_id.set(Some(id));
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let stop = move |_| {
        spawn_local(async move {
            if let Err(e) = call::<_, ()>(cmd::STOP_MEETING, &NoArgs {}).await {
                error.set(Some(e));
            }
        });
    };

    let copy_markdown = move |_| {
        let Some(id) = selected.get() else {
            return;
        };
        spawn_local(async move {
            match call::<_, String>(cmd::MEETING_MARKDOWN, &IdArgs { id }).await {
                Ok(md) => {
                    if copy_to_clipboard(&md) {
                        copied.set(true);
                    } else {
                        error.set(Some(
                            pick("Clipboard unavailable.", "无法访问剪贴板。").to_string(),
                        ));
                    }
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let delete = move |_| {
        let Some(id) = selected.get() else {
            return;
        };
        spawn_local(async move {
            match call::<_, bool>(cmd::DELETE_MEETING, &IdArgs { id }).await {
                Ok(_) => {
                    selected.set(None);
                    version.update(|v| *v += 1);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <h1>{move || pick("Meetings", "会议")}</h1>
        <p class="lede">
            {move || pick(
                "Transcribe a whole meeting as it happens. Only the text is kept — audio stays in memory for a few seconds and is discarded after each piece is transcribed.",
                "会议全程实时转写，只保留文字 —— 音频仅在内存中停留几秒，每段转写完成后立即丢弃。",
            )}
        </p>
        {move || error.get().map(|msg| view! { <p class="error">{msg}</p> })}
        <div class="card meet-ctl">
            {move || match active_id.get() {
                None => view! {
                    <button class="btn primary" on:click=start>
                        {move || pick("Start transcribing", "开始转写")}
                    </button>
                    <span class="muted">
                        {move || pick(
                            "Uses your speech model from Connections.",
                            "使用「连接」页配置的语音模型。",
                        )}
                    </span>
                }
                .into_any(),
                Some(_) => view! {
                    <span class="rec-dot"></span>
                    <span class="mono">{move || fmt_ms(elapsed.get())}</span>
                    <div class="meter">
                        <div style:width=move || format!("{}%", level.get() * 100.0)></div>
                    </div>
                    <button class="btn" on:click=stop>{move || pick("Stop", "停止")}</button>
                }
                .into_any(),
            }}
        </div>
        {move || {
            let active = active_id.get().is_some();
            let segs = live.get();
            if !active && segs.is_empty() {
                return ().into_any();
            }
            let transcribing = status.get() == "transcribing";
            view! {
                <div class="card live">
                    {segs
                        .into_iter()
                        .map(|seg| {
                            view! {
                                <p>
                                    <span class="ts mono">{fmt_ms(seg.start_ms)}</span>
                                    {seg.text}
                                </p>
                            }
                        })
                        .collect_view()}
                    {move || {
                        if transcribing {
                            view! { <p class="muted">"…"</p> }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </div>
            }
            .into_any()
        }}
        <h2>{move || pick("Past meetings", "过往会议")}</h2>
        {move || match meetings.get() {
            None => ().into_any(),
            Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
            Some(Ok(list)) if list.is_empty() => view! {
                <p class="muted">{move || pick("No saved meetings yet.", "还没有已保存的会议。")}</p>
            }
            .into_any(),
            Some(Ok(list)) => view! {
                <ul class="meet-list">
                    {list
                        .into_iter()
                        .map(|info| {
                            let id = info.id.clone();
                            let date = fmt_date(info.started_at_ms);
                            let title = info.title.clone();
                            let preview = info.preview.clone();
                            view! {
                                <li
                                    class="meet-row"
                                    on:click=move |_| {
                                        copied.set(false);
                                        selected.set(Some(id.clone()));
                                    }
                                >
                                    <div class="h-meta">
                                        <span>{title}</span>
                                        <span class="mono muted">{date}</span>
                                        <span class="muted">
                                            {move || {
                                                format!(
                                                    "{} {}",
                                                    info.segment_count,
                                                    pick("segments", "段"),
                                                )
                                            }}
                                        </span>
                                    </div>
                                    <p class="muted">{preview}</p>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            }
            .into_any(),
        }}
        {move || match detail.get() {
            None => ().into_any(),
            Some(None) => ().into_any(),
            Some(Some(Err(e))) => view! { <p class="error">{e}</p> }.into_any(),
            Some(Some(Ok(d))) => {
                let copy_label = move || {
                    if copied.get() {
                        pick("Copied", "已复制")
                    } else {
                        pick("Copy as Markdown", "复制为 Markdown")
                    }
                };
                view! {
                    <div class="card">
                        <div class="h-meta">
                            <span>{d.info.title.clone()}</span>
                            <span class="mono muted">{fmt_date(d.info.started_at_ms)}</span>
                        </div>
                        <div class="live">
                            {d.segments
                                .into_iter()
                                .map(|seg| {
                                    view! {
                                        <p>
                                            <span class="ts mono">{fmt_ms(seg.start_ms)}</span>
                                            {seg.text}
                                        </p>
                                    }
                                })
                                .collect_view()}
                        </div>
                        <div class="meet-ctl">
                            <button class="btn" on:click=copy_markdown>{copy_label}</button>
                            <button class="btn" on:click=delete>
                                {move || pick("Delete", "删除")}
                            </button>
                        </div>
                    </div>
                }
                .into_any()
            }
        }}
    }
}
