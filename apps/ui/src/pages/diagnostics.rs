use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};

use crate::i18n::pick;
use crate::ipc::{NoArgs, call};
use ob_ipc::{DiagReport, cmd};

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

fn report_text(r: &DiagReport) -> String {
    let mut out = format!("orangebot type {}\n", r.version);
    for c in &r.checks {
        let mark = if c.ok { "OK" } else { "FAIL" };
        out.push_str(&format!("[{mark}] {} — {}\n", c.name, c.detail));
    }
    out.push_str("\nASR log:\n");
    out.push_str(&r.asr_log_tail);
    out.push_str("\n\nText model log:\n");
    out.push_str(&r.llm_log_tail);
    out
}

#[component]
pub fn DiagnosticsPage() -> impl IntoView {
    let report = RwSignal::new(None::<DiagReport>);
    let running = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let copied = RwSignal::new(false);

    let run = move |_| {
        running.set(true);
        error.set(None);
        copied.set(false);
        spawn_local(async move {
            match call::<_, DiagReport>(cmd::RUN_DIAGNOSTICS, &NoArgs {}).await {
                Ok(r) => report.set(Some(r)),
                Err(e) => error.set(Some(e)),
            }
            running.set(false);
        });
    };

    let copy = move |_| {
        let Some(r) = report.get() else {
            return;
        };
        if copy_to_clipboard(&report_text(&r)) {
            copied.set(true);
        } else {
            error.set(Some(
                pick("Clipboard unavailable.", "无法访问剪贴板。").to_string(),
            ));
        }
    };

    view! {
        <h1>{move || pick("Help & diagnostics", "帮助与诊断")}</h1>
        <ul class="help-list">
            <li>
                {move || pick(
                    "Dictate: hold your hotkey, speak, release.",
                    "听写：按住快捷键，说话，松开。",
                )}
            </li>
            <li>
                {move || pick(
                    "Translate and Ask use their own hotkeys (Settings).",
                    "翻译和提问使用各自的快捷键（在设置中配置）。",
                )}
            </li>
            <li>
                {move || pick(
                    "Meetings: start in the Meetings page; only text is kept.",
                    "会议：在会议页开始，只保留文字。",
                )}
            </li>
        </ul>
        <div class="card set">
            <button class="btn primary" on:click=run disabled=move || running.get()>
                {move || pick("Run checks", "运行检查")}
            </button>
            {move || {
                if running.get() {
                    view! {
                        <span class="muted">{move || pick("Running…", "正在检查……")}</span>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
        {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        {move || match report.get() {
            None => ().into_any(),
            Some(r) => view! {
                <ul>
                    {r.checks
                        .iter()
                        .map(|c| {
                            view! {
                                <li class=if c.ok { "check ok" } else { "check bad" }>
                                    {format!(
                                        "{} {} — {}",
                                        if c.ok { "✓" } else { "✕" },
                                        c.name,
                                        c.detail,
                                    )}
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
                <details>
                    <summary>{move || pick("Speech model log", "语音模型日志")}</summary>
                    <pre class="log">{r.asr_log_tail.clone()}</pre>
                </details>
                <details>
                    <summary>{move || pick("Text model log", "文字模型日志")}</summary>
                    <pre class="log">{r.llm_log_tail.clone()}</pre>
                </details>
                <button class="btn" on:click=copy>
                    {move || {
                        if copied.get() {
                            pick("Copied", "已复制")
                        } else {
                            pick("Copy report", "复制报告")
                        }
                    }}
                </button>
            }
            .into_any(),
        }}
    }
}
