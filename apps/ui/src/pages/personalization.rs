use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

use crate::i18n::pick;
use crate::ipc::{NoArgs, call};
use ob_ipc::{StyleInfo, cmd};

#[derive(serde::Serialize)]
struct SaveStyleArgs {
    style: StyleInfo,
}

fn current_style(current: RwSignal<Option<StyleInfo>>, enabled: bool, notes: String) -> StyleInfo {
    StyleInfo {
        enabled,
        notes,
        learned_at_ms: current.get().and_then(|c| c.learned_at_ms),
    }
}

fn locale_date(ms: i64) -> String {
    js_sys::Date::new(&JsValue::from_f64(ms as f64))
        .to_locale_string("default", &JsValue::UNDEFINED)
        .into()
}

#[component]
pub fn PersonalizationPage() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let style = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, StyleInfo>(cmd::GET_STYLE, &NoArgs {}).await }
    });
    let current = RwSignal::new(None::<StyleInfo>);
    let enabled = RwSignal::new(false);
    let notes = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let learned = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(Ok(s)) = style.get() {
            current.set(Some(s.clone()));
            enabled.set(s.enabled);
            notes.set(s.notes.clone());
        }
    });

    let persist = move |next: StyleInfo| {
        spawn_local(async move {
            match call::<_, ()>(
                cmd::SAVE_STYLE,
                &SaveStyleArgs {
                    style: next.clone(),
                },
            )
            .await
            {
                Ok(()) => {
                    current.set(Some(next));
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let save = move |_| {
        learned.set(false);
        persist(current_style(current, enabled.get(), notes.get()));
    };

    let delete_notes = move |_| {
        learned.set(false);
        let cleared = StyleInfo {
            enabled: false,
            notes: String::new(),
            learned_at_ms: None,
        };
        enabled.set(false);
        notes.set(String::new());
        persist(cleared);
    };

    let learn = move |_| {
        error.set(None);
        learned.set(false);
        spawn_local(async move {
            match call::<_, StyleInfo>(cmd::LEARN_STYLE, &NoArgs {}).await {
                Ok(s) => {
                    enabled.set(s.enabled);
                    notes.set(s.notes.clone());
                    current.set(Some(s));
                    learned.set(true);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <h1>{move || pick("Personalization", "个性化")}</h1>
        <p class="lede">
            {move || pick(
                "Launch can follow how you write — tone, punctuation, capitalisation. Learned notes stay on this Mac and you can edit or delete them any time.",
                "Launch 会逐渐了解你的写作风格——语气、标点、大小写。学到的笔记只存在这台 Mac 上，随时可以编辑或删除。",
            )}
        </p>
        <div class="person">
            <label>
                <input
                    type="checkbox"
                    prop:checked=move || enabled.get()
                    on:change=move |ev| enabled.set(event_target_checked(&ev))
                />
                {move || pick("Use my style when cleaning up text", "整理文字时套用我的风格")}
            </label>
            <textarea
                class="prompt"
                rows=8
                prop:value=move || notes.get()
                on:input=move |ev| notes.set(event_target_value(&ev))
            ></textarea>
            <div>
                <button class="btn primary" on:click=learn>
                    {move || pick("Learn from my history", "从我的历史中学习")}
                </button>
                <button class="btn" on:click=save>{move || pick("Save", "保存")}</button>
                <button class="btn" on:click=delete_notes>
                    {move || pick("Delete notes", "删除笔记")}
                </button>
            </div>
            {move || {
                if learned.get() {
                    view! {
                        <p class="rec">
                            {move || pick("Learned from your recent dictations", "已根据你最近的听写完成学习")}
                        </p>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}
            {move || {
                current
                    .get()
                    .and_then(|c| c.learned_at_ms)
                    .map(|ms| {
                        view! {
                            <p class="muted">
                                {format!("{}{}", pick("Last learned: ", "最近学习："), locale_date(ms))}
                            </p>
                        }
                    })
            }}
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
    }
}
