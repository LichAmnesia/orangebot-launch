use crate::i18n::pick;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Bumped whenever the backend status may have changed (push event or poll).
#[derive(Clone, Copy)]
pub struct StatusVersion(pub RwSignal<u32>);

pub fn use_status_version() -> RwSignal<u32> {
    expect_context::<StatusVersion>().0
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Meetings,
    History,
    Dictionary,
    Modes,
    Personalization,
    Models,
    Connections,
    Settings,
    Diagnostics,
}

impl Page {
    pub const ALL_MAIN: [Page; 8] = [
        Page::Home,
        Page::Meetings,
        Page::History,
        Page::Dictionary,
        Page::Modes,
        Page::Personalization,
        Page::Models,
        Page::Connections,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Home => pick("Home", "首页"),
            Page::Meetings => pick("Meetings", "会议"),
            Page::History => pick("History", "历史"),
            Page::Dictionary => pick("Dictionary", "词典"),
            Page::Modes => pick("Modes", "模式"),
            Page::Personalization => pick("Personalization", "个性化"),
            Page::Models => pick("Models", "模型"),
            Page::Connections => pick("Connections", "连接"),
            Page::Settings => pick("Settings", "设置"),
            Page::Diagnostics => pick("Help", "帮助"),
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    crate::i18n::provide_lang();
    let (page, set_page) = signal(Page::Home);
    provide_context(set_page);

    let status_version = RwSignal::new(0_u32);
    provide_context(StatusVersion(status_version));
    spawn_local(crate::ipc::listen::<()>(ob_ipc::event::STATUS, move |_| {
        status_version.update(|v| *v += 1)
    }));
    set_interval(
        move || status_version.update(|v| *v += 1),
        std::time::Duration::from_secs(5),
    );

    view! {
        <div class="shell">
            <nav class="sidebar">
                <div class="brand" data-tauri-drag-region="">
                    <strong>"Launch"</strong>
                    <small class="muted">"by orangebot"</small>
                </div>
                {Page::ALL_MAIN
                    .iter()
                    .map(|&p| {
                        view! {
                            <button
                                class=move || {
                                    format!("nav-item{}", if page.get() == p { " active" } else { "" })
                                }
                                on:click=move |_| set_page.set(p)
                            >
                                {move || p.label()}
                            </button>
                        }
                    })
                    .collect_view()}
                <div class="nav-sep"></div>
                <div class="nav-spacer"></div>
                <button
                    class=move || {
                        format!(
                            "nav-item{}",
                            if page.get() == Page::Settings { " active" } else { "" },
                        )
                    }
                    on:click=move |_| set_page.set(Page::Settings)
                >
                    {move || Page::Settings.label()}
                </button>
                <button
                    class=move || {
                        format!(
                            "nav-item{}",
                            if page.get() == Page::Diagnostics { " active" } else { "" },
                        )
                    }
                    on:click=move |_| set_page.set(Page::Diagnostics)
                >
                    {move || Page::Diagnostics.label()}
                </button>
            </nav>
            <main class="main">
                <header class="topbar" data-tauri-drag-region="">
                    <StatusPill/>
                </header>
                <section class="content">
                    <div class="page">{move || page_view(page.get())}</div>
                </section>
            </main>
        </div>
    }
}

#[component]
fn StatusPill() -> impl IntoView {
    // leptos 0.8 (reactive_graph 0.2) has no `LocalResource`:
    // `AsyncDerived::new_unsync` is the thread-local one-shot resource.
    let version = use_status_version();
    let status = AsyncDerived::new_unsync(move || {
        version.track();
        async move {
            crate::ipc::call::<_, ob_ipc::AppStatus>(
                ob_ipc::cmd::APP_STATUS,
                &crate::ipc::NoArgs {},
            )
            .await
        }
    });

    view! {
        <span class="status-pill">
            {move || match status.get() {
                None => view! {
                    <span class="dot"></span>{pick("Checking…", "检查中…")}
                }
                .into_any(),
                Some(Err(_)) => view! {
                    <span class="dot warn"></span>{pick("Preview", "预览")}
                }
                .into_any(),
                Some(Ok(s)) => {
                    let ready = s.asr.state == ob_ipc::ReadyState::Ready
                        && s.text.state == ob_ipc::ReadyState::Ready;
                    let not_configured = s.asr.state == ob_ipc::ReadyState::NotConfigured
                        || s.text.state == ob_ipc::ReadyState::NotConfigured;
                    if ready {
                        view! { <span class="dot ok"></span>{pick("Ready", "已就绪")} }.into_any()
                    } else if not_configured {
                        view! { <span class="dot warn"></span>{pick("Not set up", "未设置")} }
                            .into_any()
                    } else {
                        view! { <span class="dot warn"></span>{pick("Model offline", "模型离线")} }
                            .into_any()
                    }
                }
            }}
        </span>
    }
}

/// Navigation handle for child components (e.g. "Get set up" on Home).
pub fn use_nav() -> WriteSignal<Page> {
    expect_context::<WriteSignal<Page>>()
}

fn page_view(page: Page) -> AnyView {
    match page {
        Page::Home => view! { <crate::pages::home::HomePage/> }.into_any(),
        Page::Meetings => view! { <crate::pages::meetings::MeetingsPage/> }.into_any(),
        Page::History => view! { <crate::pages::history::HistoryPage/> }.into_any(),
        Page::Models => view! { <crate::pages::models::ModelsPage/> }.into_any(),
        Page::Connections => view! { <crate::pages::connections::ConnectionsPage/> }.into_any(),
        Page::Settings => view! { <crate::pages::settings::SettingsPage/> }.into_any(),
        Page::Dictionary => view! { <crate::pages::dictionary::DictionaryPage/> }.into_any(),
        Page::Modes => view! { <crate::pages::modes::ModesPage/> }.into_any(),
        Page::Personalization => {
            view! { <crate::pages::personalization::PersonalizationPage/> }.into_any()
        }
        Page::Diagnostics => view! { <crate::pages::diagnostics::DiagnosticsPage/> }.into_any(),
    }
}
