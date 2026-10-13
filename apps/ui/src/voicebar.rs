use std::collections::VecDeque;
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use ob_ipc::{VoiceEvent, event};

use crate::i18n::pick;
use crate::ipc::listen;

#[component]
pub fn VoiceBar() -> impl IntoView {
    let phase = RwSignal::new("idle".to_string());
    let text = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    let levels = RwSignal::new(VecDeque::from(vec![0.0_f32; 12]));
    let started_ms = RwSignal::new(0.0_f64);
    let now_ms = RwSignal::new(0.0_f64);

    spawn_local(listen::<VoiceEvent>(event::VOICE, move |e| {
        if e.phase == "recording" && phase.get_untracked() != "recording" {
            started_ms.set(js_sys::Date::now());
            text.set(None);
            error.set(None);
        }
        if e.phase == "recording" {
            levels.update(|q| {
                q.pop_front();
                q.push_back(e.level.min(1.0));
            });
        }
        phase.set(e.phase);
        if e.text.is_some() {
            text.set(e.text);
        }
        if e.error.is_some() {
            error.set(e.error);
        }
    }));
    set_interval(
        move || now_ms.set(js_sys::Date::now()),
        Duration::from_millis(200),
    );

    let label = move || match phase.get().as_str() {
        "recording" => pick("Listening", "正在聆听").to_string(),
        "transcribing" => pick("Transcribing…", "转写中……").to_string(),
        "polishing" => pick("Polishing…", "润色中……").to_string(),
        "inserting" => pick("Inserting…", "插入中……").to_string(),
        "done" => pick("Inserted", "已插入").to_string(),
        "copied" => pick("Copied — press ⌘V to paste", "已复制，按 ⌘V 粘贴").to_string(),
        "" => pick("No speech detected", "没有听到声音").to_string(),
        "cancelled" => pick("Cancelled", "已取消").to_string(),
        "failed" => error
            .get()
            .unwrap_or_else(|| pick("Something went wrong", "出错了").to_string()),
        _ => String::new(),
    };

    let sub = move || match phase.get().as_str() {
        "recording" => {
            let secs = ((now_ms.get() - started_ms.get()) / 1000.0).max(0.0) as u64;
            format!("{:02}:{:02}", secs / 60, secs % 60)
        }
        "polishing" | "inserting" | "done" | "copied" => {
            let t = text.get().unwrap_or_default();
            let chars: Vec<char> = t.chars().collect();
            if chars.len() > 48 {
                let head: String = chars[..48].iter().collect();
                format!("{head}…")
            } else {
                t
            }
        }
        _ => String::new(),
    };

    view! {
        <style>"html,body{background:transparent!important;overflow:hidden}"</style>
        <div class=move || format!("vbar {}", phase.get())>
            <span class="vdot"></span>
            <div class="wave">
                {move || {
                    levels
                        .get()
                        .iter()
                        .map(|l| {
                            // sqrt makes quiet speech visible.
                            let h = 4.0 + l.sqrt() * 22.0;
                            view! { <i style:height=format!("{h}px")></i> }
                        })
                        .collect_view()
                }}
            </div>
            <div class="vtext">
                <span class="vlabel">{label}</span>
                <span class="vsub">{sub}</span>
            </div>
            <span class="vkey">"Esc"</span>
        </div>
    }
}
