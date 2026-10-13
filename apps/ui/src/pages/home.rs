use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::{Page, use_nav, use_status_version};
use crate::i18n::pick;
use crate::ipc::{NoArgs, call};
use crate::keys::Keys;
use crate::stats::StatsCard;
use ob_ipc::{AppStatus, HistoryItem, HotkeySettings, PermissionStatus, ReadyState, cmd};

#[derive(serde::Serialize)]
struct HistoryArgs {
    limit: u32,
    query: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivacyArgs {
    kind: String,
}

fn open_privacy(kind: &str) {
    let kind = kind.to_string();
    spawn_local(async move {
        let _ = call::<_, ()>(cmd::OPEN_PRIVACY_SETTINGS, &PrivacyArgs { kind }).await;
    });
}

fn endpoint_label(state: ReadyState) -> &'static str {
    match state {
        ReadyState::Ready => "",
        ReadyState::NotConfigured => pick("not set up", "未设置"),
        ReadyState::Unreachable => pick("offline", "离线"),
    }
}

fn hotkey_from(status: Option<Result<AppStatus, String>>) -> String {
    status
        .and_then(|r| r.ok())
        .map(|s| s.hotkey)
        .unwrap_or_else(|| "Alt+Space".into())
}

#[component]
pub fn HomePage() -> impl IntoView {
    let nav = use_nav();
    let version = use_status_version();
    let status = AsyncDerived::new_unsync(move || {
        version.track();
        async move { call::<_, AppStatus>(cmd::APP_STATUS, &NoArgs {}).await }
    });
    let history = AsyncDerived::new_unsync(|| async {
        call::<_, Vec<HistoryItem>>(
            cmd::LIST_HISTORY,
            &HistoryArgs {
                limit: 3,
                query: None,
            },
        )
        .await
    });
    let hotkeys = AsyncDerived::new_unsync(|| async {
        call::<_, HotkeySettings>(cmd::GET_HOTKEYS, &NoArgs {}).await
    });
    let perms = AsyncDerived::new_unsync(|| async {
        call::<_, PermissionStatus>(cmd::PERMISSION_STATUS, &NoArgs {}).await
    });

    view! {
        {move || {
            let st = status.get();
            let first_run = matches!(
                &st,
                Some(Ok(s))
                    if s.asr.state == ReadyState::NotConfigured
                        && s.text.state == ReadyState::NotConfigured
            );
            if first_run {
                let nav_mac = nav;
                let nav_servers = nav;
                return view! {
                    <div class="card onboard">
                        <h2>{move || pick("Set up orangebot type", "设置 orangebot type")}</h2>
                        <ol>
                            <li>
                                <strong>{move || pick("Choose where models run", "选择模型在哪里运行")}</strong>
                                <div class="choice">
                                    <button
                                        class="btn primary"
                                        on:click=move |_| nav_mac.set(Page::Models)
                                    >
                                        {move || pick("Run on this Mac (private, offline)", "在本机运行（私密、离线）")}
                                    </button>
                                    <button class="btn" on:click=move |_| nav_servers.set(Page::Connections)>
                                        {move || pick("Use my own servers", "使用自己的服务器")}
                                    </button>
                                </div>
                            </li>
                            <li>
                                <strong>{move || pick("Allow access", "授予权限")}</strong>
                                <div class="choice">
                                    {move || match perms.get() {
                                        Some(Ok(p)) if p.accessibility => view! {
                                            <span class="muted">{move || pick("✓ Accessibility", "✓ 辅助功能")}</span>
                                        }
                                        .into_any(),
                                        _ => view! {
                                            <button
                                                class="btn"
                                                on:click=|_| open_privacy("accessibility")
                                            >
                                                {move || pick("Allow Accessibility", "授予辅助功能")}
                                            </button>
                                        }
                                        .into_any(),
                                    }}
                                    <span class="muted">
                                        {move || pick("macOS asks the first time you dictate.", "macOS 会在你首次听写时询问。")}
                                    </span>
                                    <button
                                        class="btn"
                                        on:click=|_| open_privacy("microphone")
                                    >
                                        {move || pick("Open Microphone settings", "打开麦克风设置")}
                                    </button>
                                    {move || match perms.get() {
                                        Some(Ok(p)) if p.input_monitoring => view! {
                                            <span class="muted">{move || pick("✓ Input Monitoring", "✓ 输入监控")}</span>
                                        }
                                        .into_any(),
                                        _ => view! {
                                            <button
                                                class="btn"
                                                on:click=|_| open_privacy("input_monitoring")
                                            >
                                                {move || pick("Allow Input Monitoring", "授予输入监控")}
                                            </button>
                                        }
                                        .into_any(),
                                    }}
                                    <span class="muted">
                                        {move || pick(
                                            "Needed for single-key hotkeys like Right ⇧",
                                            "单键快捷键（如 Right ⇧）需要此权限",
                                        )}
                                    </span>
                                </div>
                            </li>
                            <li>
                                <strong>{move || pick("Try it", "试一试")}</strong>
                                <p class="muted">
                                    {move || pick("Click into any text field, hold ", "点进任意文本框，按住 ")}
                                    {move || {
                                        let combo = hotkey_from(status.get());
                                        view! { <Keys combo/> }
                                    }}
                                    {move || pick(", speak, release.", "，说话，松开。")}
                                </p>
                            </li>
                        </ol>
                    </div>
                }
                .into_any();
            }
            let missing: Vec<String> = match &st {
                Some(Ok(s)) if s.asr.state != ReadyState::Ready || s.text.state != ReadyState::Ready => {
                    let mut missing = Vec::new();
                    if s.asr.state != ReadyState::Ready {
                        missing.push(
                            pick("Speech model: {x}", "语音模型：{x}")
                                .replace("{x}", endpoint_label(s.asr.state)),
                        );
                    }
                    if s.text.state != ReadyState::Ready {
                        missing.push(
                            pick("Text model: {x}", "文字模型：{x}")
                                .replace("{x}", endpoint_label(s.text.state)),
                        );
                    }
                    missing
                }
                _ => Vec::new(),
            };
            let nav_models = nav;
            let nav_connections = nav;
            let setup = if missing.is_empty() {
                ().into_any()
            } else {
                let missing = missing.join(" · ");
                view! {
                    <div class="card setup">
                        <strong>{move || pick("Finish setup", "完成设置")}</strong>
                        <p class="muted">{missing}</p>
                        <div>
                            <button
                                class="btn primary"
                                on:click=move |_| nav_models.set(Page::Models)
                            >
                                {move || pick("Get local models", "获取本机模型")}
                            </button>
                            <button
                                class="btn"
                                on:click=move |_| nav_connections.set(Page::Connections)
                            >
                                {move || pick("Connect a server", "连接服务器")}
                            </button>
                        </div>
                    </div>
                }
                .into_any()
            };
            view! {
                <>
                    <h1 class="hero">
                        {move || pick("Hold ", "按住 ")}
                        {move || {
                            let combo = hotkey_from(status.get());
                            view! { <Keys combo/> }
                        }}
                        {move || pick(" and speak.", " 说话。")}
                    </h1>
                    <p class="lede">
                        {move || pick(
                            "orangebot type turns speech into clean text in whatever app you're using. Speech and text models run on your Mac or on servers you choose.",
                            "orangebot type 把语音变成干净的文本，在你正用的任意应用里都能生效。语音模型和文字模型可以跑在你的 Mac 上，也可以跑在你选择的服务器上。",
                        )}
                    </p>
                    {setup}
                </>
            }
            .into_any()
        }}
        <div class="modes" style:grid-template-columns="repeat(4,1fr)">
            <div class="card mode">
                <div class="mode-name">{move || pick("Dictate", "听写")}</div>
                <p class="muted">
                    {move || pick("Speak and get clean, punctuated text.", "说出内容，得到带标点的整洁文本。")}
                </p>
                <div>
                    {move || {
                        let combo = hotkey_from(status.get());
                        view! { <Keys combo/> }
                    }}
                </div>
            </div>
            <div class="card mode">
                <div class="mode-name">{move || pick("Translate", "翻译")}</div>
                <p class="muted">
                    {move || pick("Speak in one language, type in another.", "说一种语言，输入另一种语言。")}
                </p>
                <div>
                    {move || match hotkeys.get() {
                        Some(Ok(h)) => {
                            view! { <Keys combo=h.translate/> }.into_any()
                        }
                        _ => ().into_any(),
                    }}
                </div>
            </div>
            <div class="card mode">
                <div class="mode-name">{move || pick("Ask", "提问")}</div>
                <p class="muted">
                    {move || pick("Ask about selected text, or anything.", "就选中的文本提问，或者随便问。")}
                </p>
                <div>
                    {move || match hotkeys.get() {
                        Some(Ok(h)) => {
                            view! { <Keys combo=h.ask/> }.into_any()
                        }
                        _ => ().into_any(),
                    }}
                </div>
            </div>
            <div class="card mode">
                <div class="mode-name">{move || pick("Rewrite", "改写")}</div>
                <p class="muted">
                    {move || pick(
                        "Select text, hold the key and say how to change it.",
                        "选中文字，按住按键说出要怎么改。",
                    )}
                </p>
                <div>
                    {move || match hotkeys.get() {
                        Some(Ok(h)) => {
                            view! { <Keys combo=h.rewrite/> }.into_any()
                        }
                        _ => ().into_any(),
                    }}
                </div>
            </div>
        </div>
        {move || {
            matches!(status.get(), Some(Ok(s))
                if !(s.asr.state == ReadyState::NotConfigured
                    && s.text.state == ReadyState::NotConfigured))
        }
            .then(|| view! { <StatsCard/> })}
        <div class="section-head">
            <h2>{move || pick("Recent", "最近")}</h2>
            <button class="link" on:click=move |_| nav.set(Page::History)>
                {move || pick("All history", "全部历史")}
            </button>
        </div>
        {move || match history.get() {
            None => ().into_any(),
            Some(Err(e)) => view! { <p class="muted">{e}</p> }.into_any(),
            Some(Ok(items)) if items.is_empty() => view! {
                <p class="muted">
                    {move || pick(
                        "Your dictations will appear here after the first one.",
                        "完成第一次听写后，记录会出现在这里。",
                    )}
                </p>
            }
            .into_any(),
            Some(Ok(items)) => view! {
                <ul class="recent">
                    {items
                        .into_iter()
                        .map(|item| {
                            view! {
                                <li>
                                    <span class="recent-text">{item.final_text}</span>
                                    <span class="muted mono">{item.mode}</span>
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
