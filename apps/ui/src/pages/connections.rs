use leptos::prelude::*;
use leptos::task::spawn_local;
use ob_ipc::{
    AppStatus, ConnectionInput, SearchInput, SearchSettingsInfo, Source, StepState, TestReport, cmd,
};

use crate::i18n::pick;
use crate::ipc::{NoArgs, call};

#[derive(Clone)]
enum Msg {
    Saved,
    Cleared,
    Error(String),
}

fn msg_text(msg: &Msg) -> String {
    match msg {
        Msg::Saved => pick("Saved", "已保存").to_string(),
        Msg::Cleared => pick("Cleared", "已清除").to_string(),
        Msg::Error(e) => e.clone(),
    }
}

#[derive(serde::Serialize, Clone)]
struct InputArgs {
    input: ConnectionInput,
}

#[derive(serde::Serialize, Clone)]
struct SearchSaveArgs {
    input: SearchInput,
}

#[derive(serde::Serialize, Clone)]
struct SearchTestArgs {
    query: String,
}

#[component]
pub fn ConnectionsPage() -> impl IntoView {
    view! {
        <h1>{move || pick("Connections", "连接")}</h1>
        <p class="lede">
            {move || pick(
                "Point each job at any OpenAI-compatible server — this Mac, a box on your network, or the cloud. Speech and text can use different servers.",
                "每项任务都可以指向任意 OpenAI 兼容服务器——本机、局域网里的机器或云端。语音和文字可以用不同的服务器。",
            )}
        </p>
        <ConnectionCard
            role="asr"
            title_en="Speech recognition (ASR)"
            title_zh="语音识别（ASR）"
            default_url="http://127.0.0.1:17860/v1"
            default_model="whisper"
        />
        <ConnectionCard
            role="text"
            title_en="Text model (LLM)"
            title_zh="文字模型（LLM）"
            default_url="http://127.0.0.1:11434/v1"
            default_model=""
        />
        <SearchCard />
    }
}

#[component]
fn SearchCard() -> impl IntoView {
    let (kind, set_kind) = signal(String::new());
    let (base_url, set_base_url) = signal(String::new());
    let (api_key, set_api_key) = signal(String::new());
    let (titles, set_titles) = signal(Vec::<String>::new());
    let (busy, set_busy) = signal(false);
    let (message, set_message) = signal(None::<Msg>);

    let info = AsyncDerived::new_unsync(move || async move {
        call::<_, Option<SearchSettingsInfo>>(cmd::GET_SEARCH, &NoArgs {}).await
    });
    let (prefilled, set_prefilled) = signal(false);
    Effect::new_sync(move |_| {
        if prefilled.get() {
            return;
        }
        let Some(Ok(info)) = info.get() else {
            return;
        };
        if let Some(info) = info {
            if kind.get().is_empty() && !info.kind.is_empty() {
                set_kind.set(info.kind);
            }
            if base_url.get().is_empty() && !info.base_url.is_empty() {
                set_base_url.set(info.base_url);
            }
        }
        set_prefilled.set(true);
    });

    let on_test = move |_| {
        set_busy.set(true);
        set_message.set(None);
        let args = SearchTestArgs {
            query: "orangebot".to_string(),
        };
        spawn_local(async move {
            let result = call::<_, Vec<Source>>(cmd::TEST_SEARCH, &args).await;
            set_busy.set(false);
            match result {
                Ok(sources) => set_titles.set(sources.into_iter().map(|s| s.title).collect()),
                Err(e) => set_message.set(Some(Msg::Error(e))),
            }
        });
    };
    let on_save = move |_| {
        set_busy.set(true);
        set_message.set(None);
        let key = api_key.get();
        let args = SearchSaveArgs {
            input: SearchInput {
                kind: kind.get(),
                base_url: base_url.get(),
                api_key: (!key.is_empty()).then_some(key),
            },
        };
        spawn_local(async move {
            let result = call::<_, ()>(cmd::SAVE_SEARCH, &args).await;
            set_busy.set(false);
            set_message.set(Some(match result {
                Ok(()) => Msg::Saved,
                Err(e) => Msg::Error(e),
            }));
        });
    };

    view! {
        <div class="card conn">
            <h2>{move || pick("Web search (optional)", "网页搜索（可选）")}</h2>
            <p class="muted">
                {move || pick(
                    "Lets Ask answer questions about recent events, with sources. Queries go only to the service you choose.",
                    "让「提问」能回答近期事件并附上来源。查询只会发往你选择的服务。",
                )}
            </p>
            <label class="field">
                <span>{move || pick("Provider", "提供方")}</span>
                <select
                    class="input"
                    prop:value=kind
                    on:change=move |ev| set_kind.set(event_target_value(&ev))
                >
                    <option value="">{move || pick("Off", "关闭")}</option>
                    <option value="searxng">{move || pick("SearXNG (self-hosted)", "SearXNG（自建）")}</option>
                    <option value="brave">"Brave Search API"</option>
                </select>
            </label>
            <label class="field" hidden=move || kind.get() == "brave">
                <span>{move || pick("Base URL", "服务地址")}</span>
                <input
                    class="input mono"
                    placeholder="http://127.0.0.1:8888"
                    prop:value=base_url
                    on:input=move |ev| set_base_url.set(event_target_value(&ev))
                />
            </label>
            <label class="field" hidden=move || kind.get() != "brave">
                <span>{move || pick("API key", "API 密钥")}</span>
                <input
                    class="input mono"
                    type="password"
                    placeholder=move || pick("Stored in Keychain", "存于钥匙串")
                    prop:value=api_key
                    on:input=move |ev| set_api_key.set(event_target_value(&ev))
                />
            </label>
            <div class="row">
                <button class="btn" disabled=busy on:click=on_test>
                    {move || pick("Test", "测试")}
                </button>
                <button class="btn primary" disabled=busy on:click=on_save>
                    {move || pick("Save", "保存")}
                </button>
                {move || message.get().map(|m| view! { <span class="muted">{msg_text(&m)}</span> })}
            </div>
            {move || {
                let list = titles.get();
                (!list.is_empty()).then(|| {
                    view! {
                        <ul class="steps">
                            {list
                                .into_iter()
                                .map(|t| {
                                    view! {
                                        <li class="step ok">
                                            <span class="step-icon">"✓"</span>
                                            <span>{t}</span>
                                        </li>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </ul>
                    }
                })
            }}
        </div>
    }
}

#[component]
fn ConnectionCard(
    role: &'static str,
    title_en: &'static str,
    title_zh: &'static str,
    default_url: &'static str,
    default_model: &'static str,
) -> impl IntoView {
    let (base_url, set_base_url) = signal(default_url.to_string());
    let (model, set_model) = signal(default_model.to_string());
    let (api_key, set_api_key) = signal(String::new());
    let (report, set_report) = signal(None::<TestReport>);
    let (busy, set_busy) = signal(false);
    let (message, set_message) = signal(None::<Msg>);

    // leptos 0.8 has no `LocalResource`; AsyncDerived::new_unsync is the
    // thread-local one-shot resource. `.get()` is None while loading.
    let status = AsyncDerived::new_unsync(move || async move {
        call::<_, AppStatus>(cmd::APP_STATUS, &NoArgs {}).await
    });

    // Prefill once when the resource first yields Ok, only if the user has
    // not typed (signals still hold the defaults).
    let (prefilled, set_prefilled) = signal(false);
    Effect::new_sync(move |_| {
        if prefilled.get() {
            return;
        }
        let Some(Ok(status)) = status.get() else {
            return;
        };
        let endpoint = if role == "asr" {
            &status.asr
        } else {
            &status.text
        };
        if let Some(url) = endpoint.base_url.clone().filter(|u| !u.is_empty())
            && base_url.get() == default_url
        {
            set_base_url.set(url);
        }
        if let Some(m) = endpoint.model.clone().filter(|m| !m.is_empty())
            && model.get() == default_model
        {
            set_model.set(m);
        }
        set_prefilled.set(true);
    });

    let presets: &[(&str, &str, &str, &str)] = if role == "asr" {
        &[(
            "This Mac · whisper",
            "本机 · whisper",
            "http://127.0.0.1:17860/v1",
            "whisper",
        )]
    } else {
        &[
            ("Ollama", "Ollama", "http://127.0.0.1:11434/v1", ""),
            ("LM Studio", "LM Studio", "http://127.0.0.1:1234/v1", ""),
            ("llama.cpp", "llama.cpp", "http://127.0.0.1:17861/v1", ""),
        ]
    };
    let chips = move || {
        presets
            .iter()
            .map(|(label_en, label_zh, url, preset_model)| {
                let url = (*url).to_string();
                let preset_model = (*preset_model).to_string();
                view! {
                    <button
                        class="chip"
                        on:click=move |_| {
                            set_base_url.set(url.clone());
                            set_model.set(preset_model.clone());
                        }
                    >
                        {move || pick(label_en, label_zh)}
                    </button>
                }
            })
            .collect::<Vec<_>>()
    };

    let make_input = move || {
        let key = api_key.get();
        ConnectionInput {
            role: role.to_string(),
            base_url: base_url.get(),
            model: model.get(),
            api_key: (!key.is_empty()).then_some(key),
        }
    };

    let on_test = move |_| {
        set_busy.set(true);
        set_message.set(None);
        let args = InputArgs {
            input: make_input(),
        };
        spawn_local(async move {
            let result = call::<_, TestReport>(cmd::TEST_CONNECTION, &args).await;
            set_busy.set(false);
            match result {
                Ok(r) => set_report.set(Some(r)),
                Err(e) => set_message.set(Some(Msg::Error(e))),
            }
        });
    };
    let on_save = move |_| {
        set_busy.set(true);
        set_message.set(None);
        let args = InputArgs {
            input: make_input(),
        };
        spawn_local(async move {
            let result = call::<_, ()>(cmd::SAVE_CONNECTION, &args).await;
            set_busy.set(false);
            set_message.set(Some(match result {
                Ok(()) => Msg::Saved,
                Err(e) => Msg::Error(e),
            }));
        });
    };

    let route = if role == "asr" {
        "/audio/transcriptions"
    } else {
        "/chat/completions"
    };

    view! {
        <div class="card conn">
            <h2>{move || pick(title_en, title_zh)}</h2>
            <div class="chips">{chips}</div>
            <label class="field">
                <span>{move || pick("Base URL", "服务地址")}</span>
                <input
                    class="input mono"
                    prop:value=base_url
                    on:input=move |ev| set_base_url.set(event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span>{move || pick("Model ID", "模型 ID")}</span>
                <input
                    class="input mono"
                    prop:value=model
                    on:input=move |ev| set_model.set(event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span>{move || pick("API key", "API 密钥")}</span>
                <input
                    class="input mono"
                    type="password"
                    placeholder=move || pick("Optional · stored in Keychain", "选填 · 存于钥匙串")
                    prop:value=api_key
                    on:input=move |ev| set_api_key.set(event_target_value(&ev))
                />
            </label>
            <p class="muted mono">
                {move || format!(
                    "{}{}{}",
                    pick("Requests go to ", "请求将发往 "),
                    base_url.get().trim_end_matches('/'),
                    route,
                )}
            </p>
            <div class="row">
                <button class="btn" disabled=busy on:click=on_test>
                    {move || pick("Test", "测试")}
                </button>
                <button class="btn primary" disabled=busy on:click=on_save>
                    {move || pick("Save", "保存")}
                </button>
                {move || message.get().map(|m| view! { <span class="muted">{msg_text(&m)}</span> })}
            </div>
            {move || report.get().map(|r| view! { <Steps report=r /> })}
            <FallbackSection
                role=if role == "asr" {
                    "asr_fallback"
                } else {
                    "text_fallback"
                }
            />
        </div>
    }
}

#[component]
fn Steps(report: TestReport) -> impl IntoView {
    view! {
        <ul class="steps">
            {report
                .steps
                .iter()
                .map(|s| {
                    let (cls, icon) = match s.state {
                        StepState::Ok => ("step ok", "✓"),
                        StepState::Failed => ("step failed", "✕"),
                        StepState::Skipped => ("step skipped", "–"),
                    };
                    view! {
                        <li class=cls>
                            <span class="step-icon">{icon}</span>
                            <span>{s.name.clone()}</span>
                            <span class="muted">{s.detail.clone()}</span>
                        </li>
                    }
                })
                .collect::<Vec<_>>()}
        </ul>
    }
}

#[component]
fn FallbackSection(role: &'static str) -> impl IntoView {
    let (base_url, set_base_url) = signal(String::new());
    let (model, set_model) = signal(String::new());
    let (api_key, set_api_key) = signal(String::new());
    let (report, set_report) = signal(None::<TestReport>);
    let (busy, set_busy) = signal(false);
    let (message, set_message) = signal(None::<Msg>);

    let make_input = move || {
        let key = api_key.get();
        ConnectionInput {
            role: role.to_string(),
            base_url: base_url.get(),
            model: model.get(),
            api_key: (!key.is_empty()).then_some(key),
        }
    };

    let on_test = move |_| {
        set_busy.set(true);
        set_message.set(None);
        let args = InputArgs {
            input: make_input(),
        };
        spawn_local(async move {
            let result = call::<_, TestReport>(cmd::TEST_CONNECTION, &args).await;
            set_busy.set(false);
            match result {
                Ok(r) => set_report.set(Some(r)),
                Err(e) => set_message.set(Some(Msg::Error(e))),
            }
        });
    };
    let on_save = move |_| {
        set_busy.set(true);
        set_message.set(None);
        let cleared = base_url.get().trim().is_empty();
        let args = InputArgs {
            input: make_input(),
        };
        spawn_local(async move {
            let result = call::<_, ()>(cmd::SAVE_CONNECTION, &args).await;
            set_busy.set(false);
            set_message.set(Some(match result {
                Ok(()) if cleared => Msg::Cleared,
                Ok(()) => Msg::Saved,
                Err(e) => Msg::Error(e),
            }));
        });
    };

    view! {
        <details class="fallback">
            <summary>{move || pick("Fallback server (optional)", "备用服务器（可选）")}</summary>
            <p class="muted">
                {move || pick(
                    "Used automatically when the main server cannot be reached.",
                    "主服务器无法连接时自动启用。",
                )}
            </p>
            <label class="field">
                <span>{move || pick("Base URL", "服务地址")}</span>
                <input
                    class="input mono"
                    prop:value=base_url
                    on:input=move |ev| set_base_url.set(event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span>{move || pick("Model ID", "模型 ID")}</span>
                <input
                    class="input mono"
                    prop:value=model
                    on:input=move |ev| set_model.set(event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span>{move || pick("API key", "API 密钥")}</span>
                <input
                    class="input mono"
                    type="password"
                    placeholder=move || pick("Optional · stored in Keychain", "选填 · 存于钥匙串")
                    prop:value=api_key
                    on:input=move |ev| set_api_key.set(event_target_value(&ev))
                />
            </label>
            <div class="row">
                <button class="btn" disabled=busy on:click=on_test>
                    {move || pick("Test", "测试")}
                </button>
                <button class="btn primary" disabled=busy on:click=on_save>
                    {move || pick("Save", "保存")}
                </button>
                {move || message.get().map(|m| view! { <span class="muted">{msg_text(&m)}</span> })}
            </div>
            {move || report.get().map(|r| view! { <Steps report=r /> })}
        </details>
    }
}
