use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use ob_ipc::{AppStatus, GeneralSettings, HotkeySettings, MicInfo, MicTest, UpdateInfo, cmd};

use crate::i18n::{Lang, LangCtx, browser_lang, pick};
use crate::ipc::{NoArgs, call};
use crate::keys::Keys;

fn key_from_code(code: &str) -> String {
    if let Some(letter) = code.strip_prefix("Key") {
        letter.to_string()
    } else if let Some(digit) = code.strip_prefix("Digit") {
        digit.to_string()
    } else {
        code.to_string()
    }
}

const MODIFIER_ONLY: [&str; 5] = ["Alt", "AltGraph", "Control", "Meta", "Shift"];

const PRESETS: [(&str, &str); 5] = [
    ("Right ⇧", "RightShift"),
    ("Right ⌥", "RightOption"),
    ("Right ⌘", "RightCommand"),
    ("Right ⌃", "RightControl"),
    ("fn", "Fn"),
];

fn fn_hint() -> &'static str {
    pick(
        "fn needs System Settings → Keyboard → \u{201c}Press 🌐 key to\u{201d} set to \u{201c}Do Nothing\u{201d}.",
        "fn 键需在 系统设置 → 键盘 →「按下 🌐 键时」中选择「不执行任何操作」。",
    )
}

#[derive(serde::Serialize)]
struct SetArgs {
    hotkeys: HotkeySettings,
}

#[derive(serde::Serialize)]
struct SetMicArgs {
    name: Option<String>,
}

#[derive(serde::Serialize)]
struct SetGeneralArgs {
    general: GeneralSettings,
}

#[derive(serde::Serialize)]
struct TargetsArgs {
    targets: Vec<String>,
}

#[derive(serde::Serialize)]
struct SetLangArgs {
    lang: String,
}

const ALL_LANGUAGES: [&str; 14] = [
    "English",
    "Simplified Chinese",
    "Traditional Chinese",
    "Japanese",
    "Korean",
    "Spanish",
    "French",
    "German",
    "Portuguese",
    "Italian",
    "Russian",
    "Vietnamese",
    "Thai",
    "Indonesian",
];

fn row(en: &'static str, zh: &'static str, v: impl IntoView + 'static) -> AnyView {
    view! {
        <div class="set-row">
            <span class="set-k">{move || pick(en, zh)}</span>
            <span class="set-v">{v}</span>
        </div>
    }
    .into_any()
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let status = AsyncDerived::new_unsync(move || async move {
        call::<_, AppStatus>(cmd::APP_STATUS, &NoArgs {}).await
    });

    let version = RwSignal::new(0u32);
    let hotkeys = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, HotkeySettings>(cmd::GET_HOTKEYS, &NoArgs {}).await }
    });

    let dictate = RwSignal::new(String::new());
    let translate = RwSignal::new(String::new());
    let ask = RwSignal::new(String::new());
    let rewrite = RwSignal::new(String::new());
    let activation = RwSignal::new("hold".to_string());
    let translate_target = RwSignal::new("English".to_string());

    let recording = RwSignal::new(None::<&'static str>);
    let record_err = RwSignal::new(None::<String>);
    let mod_code = RwSignal::new(None::<String>);
    let saved = RwSignal::new(false);
    let save_err = RwSignal::new(None::<String>);

    let start_recording = move |label: &'static str| {
        saved.set(false);
        record_err.set(None);
        mod_code.set(None);
        recording.set(Some(label));
        spawn_local(async move {
            let _ = call::<_, ()>(cmd::SUSPEND_HOTKEYS, &NoArgs {}).await;
        });
    };
    let stop_recording = move |resume: bool| {
        recording.set(None);
        mod_code.set(None);
        if resume {
            spawn_local(async move {
                let _ = call::<_, ()>(cmd::RESUME_HOTKEYS, &NoArgs {}).await;
            });
        }
    };

    Effect::new(move |_| {
        if let Some(Ok(h)) = hotkeys.get() {
            dictate.set(h.dictate);
            translate.set(h.translate);
            ask.set(h.ask);
            rewrite.set(h.rewrite);
            activation.set(h.activation);
            translate_target.set(h.translate_target);
        }
    });

    let targets = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, Vec<String>>(cmd::GET_TRANSLATE_TARGETS, &NoArgs {}).await }
    });
    let override_targets = RwSignal::new(None::<Vec<String>>);
    let targets_err = RwSignal::new(None::<String>);
    let add_pick = RwSignal::new(String::new());

    let target_list = move || -> Vec<String> {
        if let Some(list) = override_targets.get() {
            return list;
        }
        match targets.get() {
            Some(Ok(t)) if !t.is_empty() => t,
            _ => {
                let single = translate_target.get();
                if single.is_empty() {
                    vec!["English".to_string()]
                } else {
                    vec![single]
                }
            }
        }
    };

    Effect::new(move |_| {
        if let Some(Ok(t)) = targets.get() {
            override_targets.set(None);
            if let Some(first) = t.first() {
                translate_target.set(first.clone());
            }
        }
    });

    let commit_targets = move |list: Vec<String>| {
        override_targets.set(Some(list.clone()));
        translate_target.set(list.first().cloned().unwrap_or_default());
        targets_err.set(None);
        spawn_local(async move {
            if let Err(e) =
                call::<_, ()>(cmd::SET_TRANSLATE_TARGETS, &TargetsArgs { targets: list }).await
            {
                targets_err.set(Some(e));
            }
        });
    };

    let _keydown = window_event_listener(ev::keydown, move |ev| {
        let Some(target) = recording.get_untracked() else {
            return;
        };
        ev.prevent_default();
        let key = ev.key();
        if key == "Escape" {
            stop_recording(true);
            record_err.set(None);
            return;
        }
        if MODIFIER_ONLY.contains(&key.as_str()) {
            mod_code.set(Some(ev.code()));
            return;
        }
        mod_code.set(None);
        let mut combo = String::new();
        if ev.ctrl_key() {
            combo.push_str("Control+");
        }
        if ev.alt_key() {
            combo.push_str("Alt+");
        }
        if ev.shift_key() {
            combo.push_str("Shift+");
        }
        if ev.meta_key() {
            combo.push_str("Super+");
        }
        if !combo.contains('+') {
            record_err.set(Some(
                pick("Use at least one modifier key", "至少需要一个修饰键").to_string(),
            ));
            return;
        }
        combo.push_str(&key_from_code(&ev.code()));
        match target {
            "Dictate" => dictate.set(combo),
            "Translate" => translate.set(combo),
            "Rewrite" => rewrite.set(combo),
            _ => ask.set(combo),
        }
        record_err.set(None);
        stop_recording(true);
    });

    let _keyup = window_event_listener(ev::keyup, move |ev| {
        let Some(target) = recording.get_untracked() else {
            return;
        };
        let code = ev.code();
        let Some(last) = mod_code.get_untracked() else {
            return;
        };
        if last != code {
            return;
        }
        match code.as_str() {
            "ShiftRight" | "AltRight" | "MetaRight" | "ControlRight" => {
                let value = match code.as_str() {
                    "ShiftRight" => "RightShift",
                    "AltRight" => "RightOption",
                    "MetaRight" => "RightCommand",
                    _ => "RightControl",
                };
                match target {
                    "Dictate" => dictate.set(value.to_string()),
                    "Translate" => translate.set(value.to_string()),
                    "Rewrite" => rewrite.set(value.to_string()),
                    _ => ask.set(value.to_string()),
                }
                record_err.set(None);
                stop_recording(true);
            }
            "ShiftLeft" | "AltLeft" | "MetaLeft" | "ControlLeft" => {
                record_err.set(Some(
                    pick(
                        "Use the right-hand key, or add another key",
                        "请使用右侧按键，或再搭配一个其他键",
                    )
                    .to_string(),
                ));
            }
            _ => {}
        }
    });

    let hotkey_row =
        move |label: &'static str, en: &'static str, zh: &'static str, combo: RwSignal<String>| {
            view! {
                <div class="set-row">
                    <span class="set-k">{move || pick(en, zh)}</span>
                    <span class="set-v">
                        {move || {
                            if recording.get() == Some(label) {
                                view! {
                                    <span class="rec">
                                        {move || pick("Press the new keys…", "按下新的快捷键…")}
                                    </span>
                                    <button
                                        class="link"
                                        on:click=move |_| {
                                            record_err.set(None);
                                            stop_recording(true);
                                        }
                                    >
                                        {move || pick("Cancel", "取消")}
                                    </button>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <Keys combo=combo.get()/>
                                    <button class="btn" on:click=move |_| start_recording(label)>
                                        {move || pick("Change", "更改")}
                                    </button>
                                }
                                    .into_any()
                            }
                        }}
                    </span>
                </div>
                <div class="set-row">
                    <span class="set-k"></span>
                    <span class="set-v">
                        {PRESETS
                            .iter()
                            .map(|(name, value)| {
                                let name = name.to_string();
                                let value = value.to_string();
                                view! {
                                    <button
                                        class="link"
                                        on:click=move |_| {
                                            combo.set(value.clone());
                                            record_err.set(None);
                                            saved.set(false);
                                            if recording.get_untracked() == Some(label) {
                                                stop_recording(true);
                                            }
                                        }
                                    >
                                        {name}
                                    </button>
                                }
                            })
                            .collect_view()}
                        {move || {
                            (combo.get() == "Fn")
                                .then(|| view! { <p class="muted">{fn_hint()}</p> })
                        }}
                    </span>
                </div>
            }
        };

    let save = move |_| {
        recording.set(None);
        mod_code.set(None);
        let settings = HotkeySettings {
            dictate: dictate.get(),
            translate: translate.get(),
            ask: ask.get(),
            rewrite: rewrite.get(),
            activation: activation.get(),
            translate_target: translate_target.get(),
        };
        spawn_local(async move {
            match call::<_, ()>(cmd::SET_HOTKEYS, &SetArgs { hotkeys: settings }).await {
                Ok(()) => {
                    saved.set(true);
                    save_err.set(None);
                }
                Err(e) => {
                    saved.set(false);
                    save_err.set(Some(e));
                    // Setting failed, so nothing re-registered the hotkeys we suspended.
                    let _ = call::<_, ()>(cmd::RESUME_HOTKEYS, &NoArgs {}).await;
                }
            }
        });
    };

    let mics = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, MicInfo>(cmd::LIST_MICS, &NoArgs {}).await }
    });
    let mic = RwSignal::new(String::new());
    let mic_err = RwSignal::new(None::<String>);
    let mic_testing = RwSignal::new(false);
    let mic_peak = RwSignal::new(None::<f32>);

    Effect::new(move |_| {
        if let Some(Ok(m)) = mics.get() {
            mic.set(m.selected.clone().unwrap_or_default());
        }
    });

    let mic_changed = move |ev| {
        let value = js_sys::Reflect::get(
            &event_target(&ev),
            &wasm_bindgen::JsValue::from_str("value"),
        )
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
        mic.set(value.clone());
        mic_err.set(None);
        let name = if value.is_empty() { None } else { Some(value) };
        spawn_local(async move {
            if let Err(e) = call::<_, ()>(cmd::SET_MIC, &SetMicArgs { name }).await {
                mic_err.set(Some(e));
            }
        });
    };

    let test_mic = move |_| {
        mic_testing.set(true);
        mic_peak.set(None);
        mic_err.set(None);
        spawn_local(async move {
            match call::<_, MicTest>(cmd::TEST_MIC, &NoArgs {}).await {
                Ok(t) => {
                    mic_peak.set(Some(t.peak));
                    mic_testing.set(false);
                }
                Err(e) => {
                    mic_err.set(Some(e));
                    mic_testing.set(false);
                }
            }
        });
    };

    let general = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, GeneralSettings>(cmd::GET_GENERAL, &NoArgs {}).await }
    });
    let polish = RwSignal::new(true);
    let launch_at_login = RwSignal::new(false);
    let retention_days = RwSignal::new(30u32);
    let meetings_follow = RwSignal::new(false);
    let general_err = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(g)) = general.get() {
            polish.set(g.polish);
            launch_at_login.set(g.launch_at_login);
            retention_days.set(g.retention_days);
            meetings_follow.set(g.meetings_follow_retention);
        }
    });

    let set_general = move |polish: bool, launch_at_login: bool| {
        let retention_days = retention_days.get_untracked();
        let meetings_follow_retention = meetings_follow.get_untracked();
        spawn_local(async move {
            let settings = GeneralSettings {
                polish,
                launch_at_login,
                retention_days,
                meetings_follow_retention,
            };
            if let Err(e) =
                call::<_, ()>(cmd::SET_GENERAL, &SetGeneralArgs { general: settings }).await
            {
                general_err.set(Some(e));
            }
        });
    };

    let ui_lang = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, String>(cmd::GET_UI_LANGUAGE, &NoArgs {}).await }
    });
    let lang = RwSignal::new("system".to_string());
    let lang_err = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(l)) = ui_lang.get() {
            lang.set(l);
        }
    });

    let lang_changed = move |ev| {
        let value = js_sys::Reflect::get(
            &event_target(&ev),
            &wasm_bindgen::JsValue::from_str("value"),
        )
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
        if value.is_empty() {
            return;
        }
        lang.set(value.clone());
        lang_err.set(None);
        spawn_local(async move {
            if let Err(e) = call::<_, ()>(cmd::SET_UI_LANGUAGE, &SetLangArgs { lang: value }).await
            {
                lang_err.set(Some(e));
            }
        });
        if let Some(ctx) = use_context::<LangCtx>() {
            ctx.0.set(match lang.get_untracked().as_str() {
                "en" => Lang::En,
                "zh-CN" => Lang::Zh,
                _ => browser_lang(),
            });
        }
    };

    let update = RwSignal::new(None::<UpdateInfo>);
    let update_err = RwSignal::new(None::<String>);
    let update_busy = RwSignal::new(false);

    let check_update = move |_| {
        update_busy.set(true);
        update_err.set(None);
        spawn_local(async move {
            match call::<_, UpdateInfo>(cmd::CHECK_UPDATE, &NoArgs {}).await {
                Ok(info) => update.set(Some(info)),
                Err(e) => update_err.set(Some(e)),
            }
            update_busy.set(false);
        });
    };

    let install_update = move |_| {
        update_err.set(None);
        spawn_local(async move {
            if let Err(e) = call::<_, ()>(cmd::INSTALL_UPDATE, &NoArgs {}).await {
                update_err.set(Some(e));
            }
        });
    };

    view! {
        <h1>{move || pick("Settings", "设置")}</h1>
        <div class="settings">
            <section class="card set">
                <h2>{move || pick("Hotkey", "快捷键")}</h2>
                {hotkey_row("Dictate", "Dictate", "听写", dictate)}
                {hotkey_row("Translate", "Translate", "翻译", translate)}
                {hotkey_row("Ask", "Ask", "提问", ask)}
                {hotkey_row("Rewrite", "Rewrite", "改写", rewrite)}
                {row(
                    "Cancel",
                    "取消",
                    view! { <span class="keycap">"Esc"</span> },
                )}
                <div class="set-row">
                    <span class="set-k">{move || pick("Activation", "触发方式")}</span>
                    <span class="set-v">
                        <div class="seg">
                            <button
                                class=move || if activation.get() == "hold" { "on" } else { "" }
                                on:click=move |_| activation.set("hold".to_string())
                            >
                                {move || pick("Hold to talk", "按住说话")}
                            </button>
                            <button
                                class=move || if activation.get() == "toggle" { "on" } else { "" }
                                on:click=move |_| activation.set("toggle".to_string())
                            >
                                {move || pick("Press to start, press to stop", "按一下开始，再按一下结束")}
                            </button>
                        </div>
                    </span>
                </div>
                {move || {
                    let list = target_list();
                    let count = list.len();
                    list.into_iter()
                        .enumerate()
                        .map(|(i, lang)| {
                            let up = i > 0;
                            view! {
                                <div class="set-row">
                                    <span class="set-k">{lang}</span>
                                    <span class="set-v">
                                        <button
                                            class="link"
                                            disabled=!up
                                            on:click=move |_| {
                                                let mut list = target_list();
                                                if i > 0 {
                                                    list.swap(i - 1, i);
                                                    commit_targets(list);
                                                }
                                            }
                                        >
                                            "↑"
                                        </button>
                                        <button
                                            class="link"
                                            disabled=count == 1
                                            on:click=move |_| {
                                                let mut list = target_list();
                                                if list.len() > 1 && i < list.len() {
                                                    list.remove(i);
                                                    commit_targets(list);
                                                }
                                            }
                                        >
                                            "×"
                                        </button>
                                    </span>
                                </div>
                            }
                        })
                        .collect_view()
                }}
                <div class="set-row">
                    <span class="set-k">{move || pick("Translate into", "翻译为")}</span>
                    <span class="set-v">
                        <select
                            class="input"
                            prop:value=move || add_pick.get()
                            on:change=move |ev| {
                                let value = js_sys::Reflect::get(
                                    &event_target(&ev),
                                    &wasm_bindgen::JsValue::from_str("value"),
                                )
                                .ok()
                                .and_then(|v| v.as_string())
                                .unwrap_or_default();
                                add_pick.set(value);
                            }
                        >
                            <option value="" selected=move || add_pick.get().is_empty()>
                                {move || pick("Choose…", "请选择…")}
                            </option>
                            {move || {
                                let list = target_list();
                                ALL_LANGUAGES
                                    .iter()
                                    .copied()
                                    .filter(|l| !list.iter().any(|t| t.as_str() == *l))
                                    .map(|lang| view! { <option value=lang>{lang}</option> })
                                    .collect_view()
                            }}
                        </select>
                        <button
                            class="btn"
                            on:click=move |_| {
                                let pick = add_pick.get_untracked();
                                add_pick.set(String::new());
                                if pick.is_empty() {
                                    return;
                                }
                                let mut list = target_list();
                                if !list.contains(&pick) {
                                    list.push(pick);
                                    commit_targets(list);
                                }
                            }
                        >
                            {move || pick("Add", "添加")}
                        </button>
                    </span>
                </div>
                <p class="muted">
                    {move || {
                        pick(
                            "The first language is used by the Translate hotkey.",
                            "第一个语言将用于翻译快捷键。",
                        )
                    }}
                </p>
                {move || targets_err.get().map(|e| view! { <p class="error">{e}</p> })}
                {move || {
                    record_err
                        .get()
                        .map(|e| view! { <p class="error">{e}</p> })
                }}
                {move || match hotkeys.get() {
                    Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                    _ => ().into_any(),
                }}
                <div class="set-row">
                    <button class="btn primary" on:click=save>
                        {move || pick("Save", "保存")}
                    </button>
                    <span class="set-v">
                        {move || {
                            if saved.get() {
                                view! {
                                    <span class="rec">{move || pick("Saved", "已保存")}</span>
                                }
                                    .into_any()
                            } else {
                                ().into_any()
                            }
                        }}
                        {move || save_err.get().map(|e| view! { <span class="error">{e}</span> })}
                    </span>
                </div>
            </section>
            <section class="card set">
                <h2>{move || pick("Microphone", "麦克风")}</h2>
                <div class="set-row">
                    <span class="set-k">{move || pick("Input device", "输入设备")}</span>
                    <span class="set-v">
                        <select class="input" prop:value=move || mic.get() on:change=mic_changed>
                            <option value="" selected=move || mic.get().is_empty()>
                                {move || pick("System default", "系统默认")}
                            </option>
                            {move || match mics.get() {
                                Some(Ok(m)) => m
                                    .devices
                                    .into_iter()
                                    .map(|d| {
                                        let label = d.clone();
                                        view! {
                                            <option value=d.clone() selected=move || mic.get() == d>
                                                {label}
                                            </option>
                                        }
                                    })
                                    .collect_view()
                                    .into_any(),
                                _ => ().into_any(),
                            }}
                        </select>
                        <button class="btn" on:click=test_mic disabled=move || mic_testing.get()>
                            {move || pick("Test", "测试")}
                        </button>
                    </span>
                </div>
                {move || {
                    mic_peak
                        .get()
                        .map(|peak| {
                            view! {
                                <div class="set-row">
                                    <span class="set-k"></span>
                                    <span class="set-v">
                                        <div class="meter">
                                            <div style:width=format!("{}%", peak * 100.0)></div>
                                        </div>
                                        {if peak > 0.02 {
                                            view! {
                                                <span class="rec">
                                                    {move || pick("Heard you", "听到了")}
                                                </span>
                                            }
                                                .into_any()
                                        } else {
                                            view! {
                                                <span class="muted">
                                                    {move || {
                                                        pick(
                                                            "Too quiet — check the microphone",
                                                            "声音太小，请检查麦克风",
                                                        )
                                                    }}
                                                </span>
                                            }
                                                .into_any()
                                        }}
                                    </span>
                                </div>
                            }
                        })
                }}
                {move || mic_err.get().map(|e| view! { <p class="error">{e}</p> })}
                {move || match mics.get() {
                    Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                    _ => ().into_any(),
                }}
            </section>
            <section class="card set">
                <h2>{move || pick("General", "通用")}</h2>
                <div class="set-row">
                    <span class="set-k">
                        {move || {
                            pick(
                                "Clean up text with the text model (off = insert exactly what was heard)",
                                "用文字模型润色文本（关闭则原样插入所听内容）",
                            )
                        }}
                    </span>
                    <span class="set-v">
                        <input
                            type="checkbox"
                            prop:checked=move || polish.get()
                            on:change=move |ev| {
                                let value = event_target_checked(&ev);
                                polish.set(value);
                                set_general(value, launch_at_login.get_untracked());
                            }
                        />
                    </span>
                </div>
                <div class="set-row">
                    <span class="set-k">
                        {move || pick("Open orangebot at login", "登录时打开 orangebot")}
                    </span>
                    <span class="set-v">
                        <input
                            type="checkbox"
                            prop:checked=move || launch_at_login.get()
                            on:change=move |ev| {
                                let value = event_target_checked(&ev);
                                launch_at_login.set(value);
                                set_general(polish.get_untracked(), value);
                            }
                        />
                    </span>
                </div>
                {move || general_err.get().map(|e| view! { <p class="error">{e}</p> })}
                {move || match general.get() {
                    Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                    _ => ().into_any(),
                }}
            </section>
            <section class="card set">
                <h2>{move || pick("Privacy", "隐私")}</h2>
                {row(
                    "Audio",
                    "音频",
                    view! {
                        <span>
                            {move || {
                                pick(
                                    "Not saved — discarded right after transcription",
                                    "不保存 — 转写完即丢弃",
                                )
                            }}
                        </span>
                    },
                )}
                <div class="set-row">
                    <span class="set-k">{move || pick("Text history", "文本历史")}</span>
                    <span class="set-v">
                        <select
                            class="input"
                            prop:value=move || retention_days.get().to_string()
                            on:change=move |ev| {
                                let raw = js_sys::Reflect::get(
                                    &event_target(&ev),
                                    &wasm_bindgen::JsValue::from_str("value"),
                                )
                                .ok()
                                .and_then(|v| v.as_string())
                                .unwrap_or_default();
                                let Ok(days) = raw.parse::<u32>() else {
                                    return;
                                };
                                retention_days.set(days);
                                set_general(
                                    polish.get_untracked(),
                                    launch_at_login.get_untracked(),
                                );
                            }
                        >
                            <option value="7">
                                {move || pick("7 days", "7 天")}
                            </option>
                            <option value="30">
                                {move || pick("30 days", "30 天")}
                            </option>
                            <option value="90">
                                {move || pick("90 days", "90 天")}
                            </option>
                            <option value="365">
                                {move || pick("1 year", "1 年")}
                            </option>
                            <option value="0">
                                {move || pick("Forever", "永久保留")}
                            </option>
                        </select>
                    </span>
                </div>
                <div class="set-row">
                    <span class="set-k">{move || pick("Meeting transcripts", "会议转写稿")}</span>
                    <span class="set-v">
                        <select
                            class="input"
                            prop:value=move || {
                                if meetings_follow.get() { "follow" } else { "keep" }
                            }
                            on:change=move |ev| {
                                let value = js_sys::Reflect::get(
                                    &event_target(&ev),
                                    &wasm_bindgen::JsValue::from_str("value"),
                                )
                                .ok()
                                .and_then(|v| v.as_string())
                                .unwrap_or_default();
                                let follow = value == "follow";
                                meetings_follow.set(follow);
                                set_general(
                                    polish.get_untracked(),
                                    launch_at_login.get_untracked(),
                                );
                            }
                        >
                            <option value="keep">
                                {move || pick("Keep until I delete them", "一直保留，直到我手动删除")}
                            </option>
                            <option value="follow">
                                {move || pick("Same as text history", "与文本历史相同")}
                            </option>
                        </select>
                    </span>
                </div>
                {row(
                    "Network",
                    "网络",
                    view! {
                        <span>
                            {move || {
                                pick(
                                    "Only the servers you set in Connections",
                                    "只连接你在「连接」里设置的服务器",
                                )
                            }}
                        </span>
                    },
                )}
                <p class="muted">
                    {move || {
                        pick(
                            "Everything stays on this Mac. Delete single items in History and Meetings, or everything at once in History.",
                            "所有内容都留在本机。可在「历史」和「会议」里逐条删除，或在「历史」里一键清空。",
                        )
                    }}
                </p>
                <p class="muted">
                    {move || {
                        pick(
                            "Strict local: when both connections point to this Mac, nothing you say leaves it.",
                            "严格本地：当两个连接都指向本机时，你说的内容绝不会离开这台 Mac。",
                        )
                    }}
                </p>
            </section>
            <section class="card set">
                <h2>{move || pick("Language", "语言")}</h2>
                <div class="set-row">
                    <span class="set-k"></span>
                    <span class="set-v">
                        <select class="input" prop:value=move || lang.get() on:change=lang_changed>
                            <option value="system">
                                {move || pick("Same as system", "跟随系统")}
                            </option>
                            <option value="en">"English"</option>
                            <option value="zh-CN">"简体中文"</option>
                        </select>
                    </span>
                </div>
                {move || lang_err.get().map(|e| view! { <p class="error">{e}</p> })}
                {move || match ui_lang.get() {
                    Some(Err(e)) => view! { <p class="error">{e}</p> }.into_any(),
                    _ => ().into_any(),
                }}
            </section>
            <section class="card set">
                <h2>{move || pick("About", "关于")}</h2>
                {row("Version", "版本", move || match status.get() {
                    Some(Ok(s)) => view! { {s.version} }.into_any(),
                    _ => "…".into_any(),
                })}
                {row("Tools", "工具", move || match status.get() {
                    Some(Ok(s)) => {
                        let names = s
                            .tools
                            .iter()
                            .map(|t| t.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ");
                        view! { {names} }.into_any()
                    }
                    _ => "…".into_any(),
                })}
                {row("License", "许可证", "Apache-2.0")}
                <div class="set-row">
                    <span class="set-k">{move || pick("Updates", "更新")}</span>
                    <span class="set-v">
                        <button
                            class="btn"
                            on:click=check_update
                            disabled=move || update_busy.get()
                        >
                            {move || pick("Check for updates", "检查更新")}
                        </button>
                        {move || match update.get() {
                            Some(info) => match info.available {
                                Some(v) => {
                                    let v = v.clone();
                                    view! {
                                        <span>
                                            {move || {
                                                let v = v.clone();
                                                pick(
                                                        "Version {v} is available",
                                                        "新版本 {v} 可用",
                                                    )
                                                    .replace("{v}", &v)
                                            }}
                                        </span>
                                        <button class="btn" on:click=install_update>
                                            {move || {
                                                pick("Install and restart", "安装并重启")
                                            }}
                                        </button>
                                    }
                                        .into_any()
                                }
                                None => {
                                    let cur = info.current.clone();
                                    view! {
                                        <span>
                                            {move || {
                                                let cur = cur.clone();
                                                pick(
                                                        "You're up to date ({v})",
                                                        "已是最新版本（{v}）",
                                                    )
                                                    .replace("{v}", &cur)
                                            }}
                                        </span>
                                    }
                                        .into_any()
                                }
                            },
                            None => ().into_any(),
                        }}
                        {move || update_err
                            .get()
                            .map(|e| view! { <span class="error">{e}</span> })}
                    </span>
                </div>
            </section>
            {move || match status.get() {
                Some(Err(e)) => view! { <p class="muted">{e}</p> }.into_any(),
                _ => ().into_any(),
            }}
        </div>
    }
}
