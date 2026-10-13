use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

use crate::i18n::pick;
use crate::ipc::{NoArgs, call};
use ob_ipc::{HistoryItem, cmd};

#[derive(serde::Serialize)]
struct Args {
    limit: u32,
    query: Option<String>,
}

fn format_time(ms: i64) -> String {
    js_sys::Date::new(&JsValue::from_f64(ms as f64))
        .to_locale_string("default", &JsValue::UNDEFINED)
        .into()
}

fn delivery_label(delivery: &str) -> String {
    match delivery {
        "verified" | "attempted" => pick("Inserted", "已插入").into(),
        "copied_only" => pick("Copied", "已复制").into(),
        "printed" => pick("Printed", "已打印").into(),
        "failed" => pick("Failed", "失败").into(),
        other => other.to_string(),
    }
}

#[component]
pub fn HistoryPage() -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let version = RwSignal::new(0_u32);
    let (confirming, set_confirming) = signal(false);
    let (message, set_message) = signal(None::<String>);
    let (deleted_count, set_deleted_count) = signal(None::<u32>);

    let items = AsyncDerived::new_unsync(move || {
        let q = query.get();
        let _ = version.get();
        async move {
            call::<_, Vec<HistoryItem>>(
                cmd::LIST_HISTORY,
                &Args {
                    limit: 200,
                    query: (!q.is_empty()).then_some(q.clone()),
                },
            )
            .await
        }
    });

    let delete_all = move |_| {
        set_confirming.set(false);
        spawn_local(async move {
            match call::<_, u32>(cmd::DELETE_ALL_HISTORY, &NoArgs {}).await {
                Ok(n) => {
                    set_deleted_count.set(Some(n));
                    set_message.set(None);
                }
                Err(e) => set_message.set(Some(e)),
            }
            version.update(|v| *v += 1);
        });
    };

    view! {
        <h1>{move || pick("History", "历史")}</h1>
        <div class="row">
            <input
                class="input search"
            placeholder=move || pick("Search your dictations", "搜索你的听写")
            on:input=move |ev| {
                let value = js_sys::Reflect::get(
                    &event_target(&ev),
                    &JsValue::from_str("value"),
                )
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
                    set_query.set(value);
                }
            />
            {move || {
                if confirming.get() {
                    view! {
                        <span class="muted">
                            {move || pick("Delete all history? This cannot be undone.", "删除全部历史？此操作无法撤销。")}
                        </span>
                        <button class="btn primary" on:click=delete_all>
                            {move || pick("Delete", "删除")}
                        </button>
                        <button
                            class="btn"
                            on:click=move |_| set_confirming.set(false)
                        >
                            {move || pick("Cancel", "取消")}
                        </button>
                    }
                    .into_any()
                } else {
                    view! {
                        <button
                            class="link"
                            on:click=move |_| {
                                set_message.set(None);
                                set_deleted_count.set(None);
                                set_confirming.set(true);
                            }
                        >
                            {move || pick("Delete all", "全部删除")}
                        </button>
                    }
                    .into_any()
                }
            }}
            {move || deleted_count.get().map(|n| {
                view! {
                    <span class="muted">{format!("{} {n}", pick("Deleted", "已删除"))}</span>
                }
            })}
            {move || message.get().map(|m| view! { <span class="muted">{m}</span> })}
        </div>
        {move || match items.get() {
            None => ().into_any(),
            Some(Err(e)) => view! { <p class="muted">{e}</p> }.into_any(),
            Some(Ok(list)) if list.is_empty() => view! {
                <p class="muted">
                    {move || pick(
                        "Nothing here yet. Hold the hotkey and speak — every dictation is saved on this Mac for 30 days.",
                        "还没有任何记录。按住快捷键说话即可——每条听写都会保存在这台 Mac 上，保留 30 天。",
                    )}
                </p>
            }
            .into_any(),
            Some(Ok(list)) => view! {
                <ul class="history">
                    {list
                        .into_iter()
                        .map(|item| {
                            let time = format_time(item.created_at_ms);
                            let delivery = item.delivery.clone();
                            let label_delivery = delivery.clone();
                            let app = item.app.clone().unwrap_or_default();
                            view! {
                                <li class="h-item">
                                    <div class="h-meta">
                                        <span class="mono muted">{time}</span>
                                        <span class="badge-mode">{item.mode}</span>
                                        <span class="muted">{app}</span>
                                        <span class=format!("delivery {delivery}")>
                                            {move || delivery_label(&label_delivery)}
                                        </span>
                                    </div>
                                    <p class="h-final">{item.final_text}</p>
                                    <details>
                                        <summary class="muted">
                                            {move || pick("Raw transcript", "原始转写稿")}
                                        </summary>
                                        <p class="mono h-raw">{item.raw_text}</p>
                                    </details>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            }
            .into_any(),
        }}
    }
}
