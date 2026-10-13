use leptos::prelude::*;
use leptos::task::spawn_local;
use ob_ipc::{ModeInfo, cmd};

use crate::i18n::pick;
use crate::ipc::{NoArgs, call};
use crate::keys::Keys;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    id: String,
    prompt: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TestArgs {
    id: String,
    prompt: String,
    sample: String,
}

#[component]
pub fn ModesPage() -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let modes = AsyncDerived::new_unsync(move || {
        version.track();
        async move { call::<_, Vec<ModeInfo>>(cmd::GET_MODES, &NoArgs {}).await }
    });

    let selected = RwSignal::new(0_usize);
    let draft = RwSignal::new(String::new());
    let notice = RwSignal::<Option<String>>::new(None);
    let sample = RwSignal::new(String::new());
    let testing = RwSignal::new(false);
    let result = RwSignal::<Option<Result<String, String>>>::new(None);

    Effect::new(move |_| {
        let _ = version.get();
        if let Some(Ok(list)) = modes.get()
            && let Some(mode) = list.get(selected.get())
        {
            draft.set(mode.prompt.clone());
        }
    });

    let current = move || {
        modes
            .get()
            .and_then(|r| r.ok())
            .and_then(|list| list.get(selected.get()).cloned())
    };

    let save = move |restore: bool| {
        let Some(mode) = current() else {
            return;
        };
        let prompt = if restore { None } else { Some(draft.get()) };
        let label = if restore {
            pick("Restored", "已恢复")
        } else {
            pick("Saved", "已保存")
        };
        spawn_local(async move {
            let args = SaveArgs {
                id: mode.id.clone(),
                prompt,
            };
            match call::<_, ()>(cmd::SAVE_MODE_PROMPT, &args).await {
                Ok(()) => {
                    notice.set(Some(label.to_string()));
                    version.update(|v| *v += 1);
                }
                Err(e) => notice.set(Some(e)),
            }
        });
    };
    let save_button = save;
    let restore_button = save;

    let try_it = move |_| {
        let Some(mode) = current() else {
            return;
        };
        let args = TestArgs {
            id: mode.id,
            prompt: draft.get(),
            sample: sample.get(),
        };
        testing.set(true);
        result.set(None);
        spawn_local(async move {
            let out = call::<_, String>(cmd::TEST_MODE, &args).await;
            testing.set(false);
            result.set(Some(out));
        });
    };

    view! {
        <h1>{move || pick("Modes", "模式")}</h1>
        <p class="lede">
            {move || {
                pick(
                    "Each mode sends what you said to the text model with a set of instructions.
                     Edit them to match how you write. Translate can use {target} for the language
                     you picked in Settings.",
                    "每个模式都会把你的话连同一段指令发给文字模型。按你的写作习惯改一改它们。\
                     翻译模式可以用 {target} 指代你在设置里选的语言。",
                )
            }}
        </p>

        {move || match modes.get() {
            None => Some(
                view! { <p class="muted">{move || pick("Loading…", "加载中…")}</p> }.into_any(),
            ),
            Some(Err(e)) => Some(view! { <p class="error">{e}</p> }.into_any()),
            Some(Ok(_)) => None,
        }}

        <div class="seg">
            {move || {
                let names = [
                    pick("Dictate", "听写"),
                    pick("Translate", "翻译"),
                    pick("Ask", "提问"),
                    pick("Rewrite", "改写"),
                ];
                names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let name = name.to_string();
                        view! {
                            <button
                                class:on=move || selected.get() == i
                                on:click=move |_| {
                                    selected.set(i);
                                    notice.set(None);
                                    result.set(None);
                                }
                            >
                                {name.clone()}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>

        {move || {
            current().map(|mode| {
                let hotkey = mode.hotkey.clone();
                let customized = mode.customized;
                let is_rewrite = mode.id == "rewrite";
                let save = save_button;
                let restore = restore_button;
                view! {
                    <div class="mode-bar">
                        <Keys combo=hotkey/>
                        {move || {
                            customized.then(|| {
                                view! { <span class="badge">{pick("Customized", "已自定义")}</span> }
                            })
                        }}
                    </div>
                    <textarea class="prompt mono" rows=10
                        prop:value=move || draft.get()
                        on:input=move |ev| draft.set(event_target_value(&ev))
                    />
                    <div class="mode-bar">
                        <button class="btn"
                            on:click=move |_| save(false)
                        >
                            {move || pick("Save", "保存")}
                        </button>
                        <button class="btn ghost"
                            on:click=move |_| restore(true)
                        >
                            {move || pick("Restore default", "恢复默认")}
                        </button>
                        {move || notice.get().map(|n| view! { <span class="notice">{n}</span> })}
                    </div>
                    <div class="card test">
                        <textarea rows=3
                            placeholder=move || {
                                if is_rewrite {
                                    pick(
                                        "Paste some text to rewrite with the default instruction",
                                        "粘贴一段文字，将按默认指令改写",
                                    )
                                } else {
                                    pick(
                                        "Paste or type something you might say, e.g. um so the meeting is at 3, no wait, 4",
                                        "粘贴或输入一段你可能会说的话，例如：嗯，那个，会议在 3 点，不对，是 4 点",
                                    )
                                }
                            }
                            prop:value=move || sample.get()
                            on:input=move |ev| sample.set(event_target_value(&ev))
                        />
                        <div>
                            <button class="btn" on:click=try_it>
                                {move || {
                                    if testing.get() {
                                        pick("Trying…", "测试中…")
                                    } else {
                                        pick("Try it", "测试一下")
                                    }
                                }}
                            </button>
                        </div>
                        {move || result.get().map(|r| match r {
                            Ok(text) => view! { <div class="result">{text}</div> }.into_any(),
                            Err(e) => view! { <div class="result">{e}</div> }.into_any(),
                        })}
                    </div>
                }
            })
        }}
    }
}
