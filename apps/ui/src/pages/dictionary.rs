use leptos::prelude::*;
use leptos::task::spawn_local;
use ob_ipc::{DictEntryInfo, cmd};

use crate::i18n::pick;
use crate::ipc::{NoArgs, call};

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AddArgs {
    phrase: String,
    aliases: Vec<String>,
}

#[derive(serde::Serialize, Clone, Copy)]
struct IdArgs {
    id: i64,
}

#[derive(serde::Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct EnabledArgs {
    id: i64,
    enabled: bool,
}

fn entries_view(entries: Result<Vec<DictEntryInfo>, String>, version: RwSignal<u32>) -> AnyView {
    match entries {
        Err(e) => view! { <p class="error">{e}</p> }.into_any(),
        Ok(list) if list.is_empty() => view! {
            <p class="muted">{move || pick("No words yet.", "还没有词条。")}</p>
        }
        .into_any(),
        Ok(list) => view! {
            <ul class="dict-list">
                {list
                    .into_iter()
                    .map(|entry| {
                        let entry_id = entry.id;
                        let aliases = entry.aliases.join(", ");
                        view! {
                            <li>
                                <span class="dict-phrase">{entry.phrase}</span>
                                <span class="muted">{aliases}</span>
                                <span class="badge-src">{entry.source}</span>
                                <input
                                    type="checkbox"
                                    prop:checked=entry.enabled
                                    on:change=move |ev| {
                                        let args = EnabledArgs {
                                            id: entry_id,
                                            enabled: event_target_checked(&ev),
                                        };
                                        spawn_local(async move {
                                            let _ = call::<_, ()>(
                                                cmd::SET_DICTIONARY_ENABLED,
                                                &args,
                                            )
                                            .await;
                                            version.update(|v| *v += 1);
                                        });
                                    }
                                />
                                <button
                                    class="link"
                                    on:click=move |_| {
                                        let args = IdArgs { id: entry_id };
                                        spawn_local(async move {
                                            let _ = call::<_, ()>(
                                                cmd::DELETE_DICTIONARY_ENTRY,
                                                &args,
                                            )
                                            .await;
                                            version.update(|v| *v += 1);
                                        });
                                    }
                                >
                                    {move || pick("Remove", "删除")}
                                </button>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        }
        .into_any(),
    }
}

#[component]
pub fn DictionaryPage() -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let list = AsyncDerived::new_unsync(move || async move {
        let _ = version.get();
        call::<_, Vec<DictEntryInfo>>(cmd::LIST_DICTIONARY, &NoArgs {}).await
    });

    let (phrase, set_phrase) = signal(String::new());
    let (aliases, set_aliases) = signal(String::new());
    let (error, set_error) = signal(None::<String>);

    let add = move || {
        let phrase = phrase.get().trim().to_string();
        if phrase.is_empty() {
            return;
        }
        let alias_list: Vec<String> = aliases
            .get()
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(str::to_string)
            .collect();
        let args = AddArgs {
            phrase,
            aliases: alias_list,
        };
        spawn_local(async move {
            match call::<_, ()>(cmd::ADD_DICTIONARY_ENTRY, &args).await {
                Ok(()) => {
                    set_phrase.set(String::new());
                    set_aliases.set(String::new());
                    set_error.set(None);
                    version.update(|v| *v += 1);
                }
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    view! {
        <h1>{move || pick("Dictionary", "词典")}</h1>
        <p class="lede">
            {move || {
                pick(
                    "Names, product words and spellings you use often. They are passed to speech recognition as hints and to the text model as preferred spellings.",
                    "你常用的名称、产品词和拼写。语音识别会把它们作为提示，文字模型会把它们作为首选拼写。",
                )
            }}
        </p>
        <div class="dict-add card">
            <input
                class="input"
                placeholder=move || pick("e.g. Launch", "例如 Launch")
                prop:value=phrase
                on:input=move |ev| set_phrase.set(event_target_value(&ev))
                on:keydown=move |ev| {
                    if ev.key() == "Enter" {
                        add();
                    }
                }
            />
            <input
                class="input"
                placeholder=move || {
                    pick(
                        "Also heard as (comma-separated, optional)",
                        "也常被听成（逗号分隔，选填）",
                    )
                }
                prop:value=aliases
                on:input=move |ev| set_aliases.set(event_target_value(&ev))
            />
            <button class="btn primary" on:click=move |_| add()>
                {move || pick("Add", "添加")}
            </button>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
        {move || list.get().map(|entries| entries_view(entries, version))}
    }
}
