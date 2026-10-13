use leptos::prelude::*;
use leptos::task::spawn_local;
use pulldown_cmark::{Event, Options, Parser};
use wasm_bindgen::{JsCast, JsValue};

use crate::i18n::pick;
use crate::ipc::{call, listen};

#[derive(serde::Serialize)]
struct ReplaceArgs {
    text: String,
}

/// Renders markdown to HTML. Raw HTML in the source is downgraded to text
/// so a model answer can never inject markup into this window.
fn render_markdown(md: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(md, options).map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        other => other,
    });
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, parser);
    out
}

fn copy_to_clipboard(text: &str) -> bool {
    let global = js_sys::global();
    let Ok(navigator) = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    let Ok(clipboard) = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")) else {
        return false;
    };
    let Ok(write_text) = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText")) else {
        return false;
    };
    let Some(f) = write_text.dyn_ref::<js_sys::Function>() else {
        return false;
    };
    f.call1(&clipboard, &JsValue::from_str(text)).is_ok()
}

#[component]
pub fn AnswerCard() -> impl IntoView {
    let (question, set_question) = signal(String::new());
    let (answer, set_answer) = signal(None::<String>);
    let (error, set_error) = signal(None::<String>);
    let (done, set_done) = signal(false);
    let (copied, set_copied) = signal(false);
    let (scope, set_scope) = signal(String::new());
    let (can_replace, set_can_replace) = signal(false);
    let (sources, set_sources) = signal(Vec::<ob_ipc::Source>::new());
    let (replaced, set_replaced) = signal(None::<String>);
    let (replace_err, set_replace_err) = signal(None::<String>);

    Effect::new(move |_| {
        spawn_local(listen::<ob_ipc::AnswerEvent>(
            ob_ipc::event::ANSWER,
            move |ev| {
                if !ev.done {
                    set_answer.set(None);
                    set_error.set(None);
                    set_done.set(false);
                }
                set_copied.set(false);
                set_replaced.set(None);
                set_replace_err.set(None);
                set_question.set(ev.question);
                set_scope.set(ev.scope);
                set_can_replace.set(ev.can_replace);
                set_sources.set(ev.sources);
                if let Some(a) = ev.answer {
                    set_answer.set(Some(a));
                }
                if let Some(e) = ev.error {
                    set_error.set(Some(e));
                }
                if ev.done {
                    set_done.set(true);
                }
            },
        ));
    });

    let replace = move |_| {
        let Some(text) = answer.get() else {
            return;
        };
        spawn_local(async move {
            match call::<_, String>(ob_ipc::cmd::REPLACE_SELECTION, &ReplaceArgs { text }).await {
                Ok(outcome) => set_replaced.set(Some(outcome)),
                Err(e) => set_replace_err.set(Some(e)),
            }
        });
    };

    view! {
        <div class="answer">
            <span class="chip">
                {move || {
                    if scope.get() == "selection" {
                        pick("About your selection", "关于你选中的内容")
                    } else {
                        pick("General answer", "通用回答")
                    }
                }}
            </span>
            <p class="q muted">{move || question.get()}</p>
            {move || {
                if !done.get() {
                    return view! {
                        <p class="muted">{move || pick("Thinking…", "思考中……")}</p>
                    }
                        .into_any();
                }
                if let Some(err) = error.get() {
                    return view! { <p class="error">{err}</p> }.into_any();
                }
                match answer.get() {
                    Some(a) => view! {
                        <div class="md" inner_html=render_markdown(&a)></div>
                    }
                    .into_any(),
                    None => ().into_any(),
                }
            }}
            {move || {
                let list = sources.get();
                if list.is_empty() {
                    return ().into_any();
                }
                view! {
                    <ol class="sources">
                        {list
                            .into_iter()
                            .map(|s| {
                                view! {
                                    <li>
                                        <a href=s.url.clone() target="_blank">{s.title}</a>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ol>
                }
                .into_any()
            }}
            <div>
                <button
                    class="btn"
                    on:click=move |_| {
                        let text = answer.get();
                        if text.as_deref().is_some_and(copy_to_clipboard) {
                            set_copied.set(true);
                        }
                    }
                >
                    {move || {
                        if copied.get() {
                            pick("Copied", "已复制")
                        } else {
                            pick("Copy", "复制")
                        }
                    }}
                </button>
                {move || {
                    if can_replace.get() && answer.get().is_some() {
                        view! {
                            <div>
                                <button class="btn primary" on:click=replace>
                                    {move || pick("Replace selected text", "替换选中文本")}
                                </button>
                                {move || {
                                    replaced
                                        .get()
                                        .map(|outcome| view! { <span class="muted">{outcome}</span> })
                                }}
                                {move || {
                                    replace_err
                                        .get()
                                        .map(|e| view! { <span class="error">{e}</span> })
                                }}
                            </div>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_html_is_not_rendered() {
        let out = render_markdown("<script>x</script>");
        assert!(!out.contains("<script>"));
    }

    #[test]
    fn bold_renders() {
        let out = render_markdown("**b**");
        assert!(out.contains("<strong>b</strong>"));
    }
}
