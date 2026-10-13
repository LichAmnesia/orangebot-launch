use leptos::prelude::*;
use leptos::task::spawn_local;
use ob_ipc::{DownloadEvent, DownloadState, ModelInfo, RecipeInfo, SystemInfo, cmd, event};

use crate::i18n::pick;
use crate::ipc::{NoArgs, call, listen};

/// Argument bundle for `download_model` / pause / resume / cancel
/// (Tauri expects camelCase keys).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DlArgs {
    model_id: String,
}

/// Argument bundle for `use_recipe` (Tauri expects camelCase keys).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UseArgs {
    recipe_id: String,
}

fn send_dl(cmd_name: &'static str, model_id: String) {
    spawn_local(async move {
        let _ = call::<_, ()>(cmd_name, &DlArgs { model_id }).await;
    });
}

fn recipe_label(id: &str) -> &str {
    match id {
        "local-light" => pick("Light", "轻量"),
        "local-balanced" => pick("Balanced", "均衡"),
        "local-chinese" => pick("Chinese", "中文"),
        "local-chinese-quality" => pick("Chinese · quality", "中文 · 高质量"),
        other => other,
    }
}

fn recipe_note(id: &str) -> Option<&'static str> {
    match id {
        "local-balanced" => Some(pick(
            "Recommended · passes all 20 cleanup checks",
            "推荐 · 通过全部 20 项文本清洗检查",
        )),
        "local-light" => Some(pick(
            "Basic cleanup · may miss self-corrections",
            "基础清洗 · 可能漏掉自我纠正",
        )),
        _ => None,
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn mb(bytes: u64) -> u64 {
    bytes / 1_000_000
}

fn dl_row(state: DownloadState) -> AnyView {
    let pct = if state.total == 0 {
        0
    } else {
        ((state.downloaded * 100) / state.total).min(100)
    };
    let label = format!(
        "{} · {}% · {}/{} MB · {:.1} MB/s",
        state.model_id,
        pct,
        mb(state.downloaded),
        mb(state.total),
        state.speed_bps as f64 / 1e6,
    );
    let state_name = state.state.clone();
    let model_id = state.model_id.clone();
    let error = state.error.clone();
    let pause_id = model_id.clone();
    let resume_id = model_id.clone();
    let cancel_id = model_id.clone();
    let control: AnyView = match state_name.as_str() {
        "downloading" => view! {
            <button
                class="btn"
                on:click=move |_| send_dl(cmd::PAUSE_DOWNLOAD, pause_id.clone())
            >
                {pick("Pause", "暂停")}
            </button>
        }
        .into_any(),
        "paused" | "failed" => view! {
            <button
                class="btn"
                on:click=move |_| send_dl(cmd::RESUME_DOWNLOAD, resume_id.clone())
            >
                {pick("Resume", "继续")}
            </button>
        }
        .into_any(),
        "queued" => view! { <span class="muted">{pick("Waiting…", "排队中…")}</span> }.into_any(),
        _ => ().into_any(),
    };
    view! {
        <div class="dl-row">
            <div class="bar">
                <div class="bar-fill" style:width=format!("{pct}%")></div>
            </div>
            <span class="muted mono">{label}</span>
            {error.map(|msg| view! { <p class="error">{msg}</p> })}
            <div class="dl-actions">
                {control}
                <button
                    class="btn"
                    on:click=move |_| send_dl(cmd::CANCEL_DOWNLOAD, cancel_id.clone())
                >
                    {pick("Cancel", "取消")}
                </button>
            </div>
        </div>
    }
    .into_any()
}

#[component]
pub fn ModelsPage() -> impl IntoView {
    let version = RwSignal::new(0_u32);
    let downloads = RwSignal::new(Vec::<DownloadState>::new());
    let pending_use = RwSignal::new(None::<String>);
    let starting = RwSignal::new(None::<String>);
    let running = RwSignal::new(None::<String>);
    let (error, set_error) = signal(None::<String>);

    let recipes = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, Vec<RecipeInfo>>(cmd::LIST_RECIPES, &NoArgs {}).await }
    });
    let models = AsyncDerived::new_unsync(move || {
        let _ = version.get();
        async move { call::<_, Vec<ModelInfo>>(cmd::LIST_MODELS, &NoArgs {}).await }
    });
    let sysinfo = AsyncDerived::new_unsync(|| async move {
        call::<_, SystemInfo>(cmd::SYSTEM_INFO, &NoArgs {}).await
    });

    Effect::new(move |_| {
        spawn_local(listen::<Vec<DownloadState>>(
            event::DOWNLOADS,
            move |list| {
                downloads.set(list);
            },
        ));
        spawn_local(listen::<DownloadEvent>(event::DOWNLOAD, move |ev| {
            if ev.done {
                version.update(|v| *v += 1);
            }
        }));
        spawn_local(async move {
            let list = call::<_, Vec<DownloadState>>(cmd::LIST_DOWNLOADS, &NoArgs {}).await;
            if let Ok(list) = list {
                downloads.set(list);
            }
        });
    });

    let launch = move |recipe_id: String| {
        starting.set(Some(recipe_id.clone()));
        spawn_local(async move {
            let args = UseArgs {
                recipe_id: recipe_id.clone(),
            };
            match call::<_, ()>(cmd::USE_RECIPE, &args).await {
                Ok(()) => running.set(Some(recipe_id)),
                Err(e) => set_error.set(Some(e)),
            }
            starting.set(None);
        });
    };

    let pending_launch = {
        move || {
            let Some(recipe_id) = pending_use.get() else {
                return;
            };
            let states = downloads.get();
            let Some(Ok(list)) = recipes.get() else {
                return;
            };
            let Some(recipe) = list.into_iter().find(|r| r.id == recipe_id) else {
                return;
            };
            let recipe_models = [recipe.asr_model.clone(), recipe.text_model.clone()];
            let state_of = |id: &str| {
                states
                    .iter()
                    .find(|s| s.model_id == id && s.state != "done")
            };
            if recipe_models.iter().all(|id| {
                states
                    .iter()
                    .any(|s| s.model_id == *id && s.state == "done")
            }) {
                pending_use.set(None);
                launch(recipe_id);
                return;
            }
            if let Some(msg) = recipe_models.iter().find_map(|id| {
                state_of(id).filter(|s| s.state == "failed").map(|s| {
                    s.error.clone().unwrap_or_else(|| {
                        format!("{} {}", id, pick("download failed", "下载失败"))
                    })
                })
            }) {
                pending_use.set(None);
                set_error.set(Some(msg));
            }
        }
    };
    Effect::new(move |_| pending_launch());

    view! {
        <h1>{move || pick("Models", "模型")}</h1>
        <p class="lede">
            {move || pick(
                "Pick a recipe — one speech model and one text model that fit your Mac. Files are downloaded once, checked against their published SHA-256, and stay on this machine.",
                "选一个方案 —— 一个语音模型加一个文字模型，恰好适配你的 Mac。文件只需下载一次，会按官方 SHA-256 校验，且始终留在本机。",
            )}
        </p>
        {move || error.get().map(|msg| view! { <p class="error">{msg}</p> })}
        {move || match sysinfo.get() {
            Some(Ok(si)) => view! {
                <p class="muted">{move || format!(
                    "{} {} · {} GB {} · {} GB {}",
                    pick("This Mac:", "本机："),
                    si.chip,
                    si.total_ram_gib,
                    pick("memory", "内存"),
                    si.free_disk_gib,
                    pick("free", "可用"),
                )}</p>
            }
            .into_any(),
            _ => ().into_any(),
        }}
        <div class="recipes">
            {move || match recipes.get() {
                None => {
                    view! { <p class="muted">{pick("Loading recipes…", "正在加载方案…")}</p> }
                        .into_any()
                }
                Some(Err(err)) => view! { <p class="error">{err}</p> }.into_any(),
                Some(Ok(list)) => list
                    .into_iter()
                    .map(|recipe| {
                        let label = recipe_label(&recipe.id).to_string();
                        let installed = recipe.installed;
                        let note = recipe_note(&recipe.id);
                        let recommended = sysinfo
                            .get()
                            .and_then(|r| r.ok())
                            .map(|s| s.recommended_recipe);
                        let badge = if Some(recipe.id.as_str()) == recommended.as_deref() {
                            pick("Recommended for this Mac", "本机推荐").to_string()
                        } else {
                            recipe.qualification.clone()
                        };
                        let recipe_id = recipe.id.clone();
                        let asr_id = recipe.asr_model.clone();
                        let text_id = recipe.text_model.clone();
                        let download_pair = (!installed)
                            .then(|| (asr_id.clone(), text_id.clone()));
                        let btn_label: &'static str = if installed {
                            pick("Use on this Mac", "在本机运行")
                        } else {
                            pick("Download & use", "下载并使用")
                        };
                        let controls = {
                            move || -> AnyView {
                                let states = downloads.get();
                                let active: Vec<DownloadState> =
                                    [asr_id.as_str(), text_id.as_str()]
                                        .into_iter()
                                        .filter_map(|id| {
                                            states
                                                .iter()
                                                .find(|s| s.model_id == id && s.state != "done")
                                                .cloned()
                                        })
                                        .collect();
                                if !active.is_empty() {
                                    return active.into_iter().map(dl_row).collect_view().into_any();
                                }
                                if running.get().as_deref() == Some(recipe_id.as_str()) {
                                    return view! {
                                        <span class="badge">{pick("Running on this Mac", "已在本机运行")}</span>
                                    }
                                    .into_any();
                                }
                                if starting.get().as_deref() == Some(recipe_id.as_str()) {
                                    return view! {
                                        <span class="muted">
                                            {pick("Starting local models…", "正在启动本地模型…")}
                                        </span>
                                    }
                                    .into_any();
                                }
                                let rid = recipe_id.clone();
                                let pair = download_pair.clone();
                                view! {
                                    <button
                                        class="btn primary"
                                        on:click=move |_| {
                                            set_error.set(None);
                                            let id = rid.clone();
                                            let pair = pair.clone();
                                            spawn_local(async move {
                                                if let Some((asr, text)) = pair {
                                                    for model_id in [asr, text] {
                                                        let args = DlArgs { model_id };
                                                        if let Err(e) =
                                                            call::<_, ()>(cmd::DOWNLOAD_MODEL, &args)
                                                                .await
                                                        {
                                                            set_error.set(Some(e));
                                                            return;
                                                        }
                                                    }
                                                    pending_use.set(Some(id));
                                                } else {
                                                    launch(id);
                                                }
                                            });
                                        }
                                    >
                                        {btn_label}
                                    </button>
                                }
                                .into_any()
                            }
                        };
                        view! {
                            <div class="card recipe">
                                <div class="recipe-head">
                                    <strong>{label}</strong>
                                    <span class="badge">{badge}</span>
                                </div>
                                <p class="mono muted">
                                    {format!("{} + {}", recipe.asr_model, recipe.text_model)}
                                </p>
                                {note.map(|note| view! { <p class="muted">{note}</p> })}
                                <p class="muted">
                                    {format!(
                                        "{} · {} {} GB {}",
                                        gb(recipe.total_bytes),
                                        pick("needs", "需要"),
                                        recipe.target_ram_gib,
                                        pick("RAM", "内存"),
                                    )}
                                </p>
                                {controls}
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any(),
            }}
        </div>
        <h2>{move || pick("All models", "全部模型")}</h2>
        {move || match models.get() {
            None => {
                view! { <p class="muted">{pick("Loading models…", "正在加载模型…")}</p> }
                    .into_any()
            }
            Some(Err(err)) => view! { <p class="error">{err}</p> }.into_any(),
            Some(Ok(list)) => view! {
                <table class="models">
                    <thead>
                        <tr>
                            <th>{pick("Model", "模型")}</th>
                            <th>{pick("Role", "类型")}</th>
                            <th>{pick("License", "许可证")}</th>
                            <th>{pick("Size", "大小")}</th>
                            <th>{pick("Status", "状态")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {list
                            .into_iter()
                            .map(|model| {
                                let status = if model.installed {
                                    pick("Installed", "已安装")
                                } else {
                                    "—"
                                };
                                view! {
                                    <tr>
                                        <td class="mono">{model.id}</td>
                                        <td>{model.role}</td>
                                        <td>{model.license}</td>
                                        <td>{gb(model.total_bytes)}</td>
                                        <td>{status}</td>
                                    </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            }
            .into_any(),
        }}
    }
}
