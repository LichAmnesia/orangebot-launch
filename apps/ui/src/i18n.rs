use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Zh,
}

#[derive(Clone, Copy)]
pub struct LangCtx(pub RwSignal<Lang>);

pub fn browser_lang() -> Lang {
    let language = js_sys::Reflect::get(
        &js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
            .unwrap_or(wasm_bindgen::JsValue::UNDEFINED),
        &"language".into(),
    )
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_default();
    if language.starts_with("zh") {
        Lang::Zh
    } else {
        Lang::En
    }
}

pub fn lang() -> Lang {
    use_context::<LangCtx>()
        .map(|ctx| ctx.0.get())
        .unwrap_or_else(browser_lang)
}

pub fn pick(en: &'static str, zh: &'static str) -> &'static str {
    if lang() == Lang::Zh { zh } else { en }
}

pub fn provide_lang() {
    let signal = RwSignal::new(browser_lang());
    provide_context(LangCtx(signal));
    spawn_local(async move {
        let Ok(raw) =
            crate::ipc::call::<_, String>(ob_ipc::cmd::GET_UI_LANGUAGE, &crate::ipc::NoArgs {})
                .await
        else {
            return;
        };
        let resolved = match raw.as_str() {
            "en" => Lang::En,
            "zh-CN" => Lang::Zh,
            "system" => browser_lang(),
            _ => return,
        };
        signal.set(resolved);
    });
}
