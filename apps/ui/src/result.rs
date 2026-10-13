use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};

use crate::i18n::pick;
use crate::ipc::{call, listen};
use ob_ipc::{ResultEvent, cmd, event};

#[derive(serde::Serialize)]
struct TextArgs {
    text: String,
}

#[derive(serde::Serialize)]
struct RawArgs {
    raw: String,
}

fn reflect_prop(target: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(target, &JsValue::from_str(key)).ok()
}

fn call_method(obj: &JsValue, method: &str, args: Option<&JsValue>) -> Option<JsValue> {
    let f = reflect_prop(obj, method)?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    match args {
        Some(a) => f.call1(obj, a).ok(),
        None => f.call0(obj).ok(),
    }
}

fn copy_to_clipboard(text: &str) -> bool {
    let Some(navigator) = reflect_prop(&js_sys::global(), "navigator") else {
        return false;
    };
    let Some(clipboard) = reflect_prop(&navigator, "clipboard") else {
        return false;
    };
    call_method(&clipboard, "writeText", Some(&JsValue::from_str(text))).is_some()
}

fn hide_window() {
    let Some(tauri) = reflect_prop(&js_sys::global(), "__TAURI__") else {
        return;
    };
    let Some(window_ns) = reflect_prop(&tauri, "window") else {
        return;
    };
    let Some(current) = call_method(&window_ns, "getCurrentWindow", None) else {
        return;
    };
    let _ = call_method(&current, "hide", None);
}

#[component]
pub fn ResultCard() -> impl IntoView {
    let card = RwSignal::new(None::<ResultEvent>);
    let text = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());

    Effect::new(move |_| {
        spawn_local(listen::<ResultEvent>(event::RESULT, move |ev| {
            status.set(String::new());
            text.set(ev.text.clone());
            card.set(Some(ev));
        }));
    });

    let insert = move |_| {
        let value = text.get();
        spawn_local(async move {
            match call::<_, String>(cmd::RESULT_INSERT, &TextArgs { text: value }).await {
                Ok(outcome) => status.set(outcome),
                Err(e) => status.set(e),
            }
        });
    };

    let retry = move |_| {
        let Some(raw) = card.get().map(|c| c.raw) else {
            return;
        };
        spawn_local(async move {
            match call::<_, String>(cmd::RESULT_RETRY, &RawArgs { raw }).await {
                Ok(out) => {
                    text.set(out);
                    status.set(String::new());
                }
                Err(e) => status.set(e),
            }
        });
    };

    let copy = move |_| {
        if copy_to_clipboard(&text.get()) {
            status.set(pick("Copied", "已复制").to_string());
        } else {
            status.set(pick("Clipboard unavailable.", "无法访问剪贴板。").to_string());
        }
    };

    let dismiss = move |_| hide_window();

    view! {
        <div class="result">
            {move || match card.get() {
                None => view! {
                    <p class="muted">{move || pick("Nothing to recover.", "没有需要恢复的内容。")}</p>
                }
                    .into_any(),
                Some(c) => view! {
                    <p class="muted">{c.reason}</p>
                    <textarea
                        prop:value=move || text.get()
                        on:input=move |ev| text.set(event_target_value(&ev))
                    ></textarea>
                    <details>
                        <summary>{move || pick("What was heard", "听到了什么")}</summary>
                        <p>{c.raw}</p>
                    </details>
                    <div class="row">
                        <button class="btn primary" on:click=insert>
                            {move || pick("Insert into current app", "插入到当前应用")}
                        </button>
                        <button class="btn" on:click=retry>
                            {move || pick("Retry cleanup", "重试润色")}
                        </button>
                        <button class="btn" on:click=copy>{move || pick("Copy", "复制")}</button>
                        <button class="btn" on:click=dismiss>
                            {move || pick("Dismiss", "关闭")}
                        </button>
                    </div>
                    <p class="muted">{move || status.get()}</p>
                }                .into_any(),
            }}
        </div>
    }
}
