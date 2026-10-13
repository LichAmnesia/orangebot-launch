mod answer;
mod app;
mod i18n;
mod ipc;
mod keys;
mod pages;
mod result;
mod stats;
mod voicebar;

fn main() {
    let hash = js_sys::Reflect::get(
        &js_sys::Reflect::get(&js_sys::global(), &"location".into())
            .unwrap_or(wasm_bindgen::JsValue::UNDEFINED),
        &"hash".into(),
    )
    .ok()
    .and_then(|v| v.as_string())
    .unwrap_or_default();
    if hash == "#voicebar" {
        leptos::mount::mount_to_body(voicebar::VoiceBar);
    } else if hash == "#answer" {
        leptos::mount::mount_to_body(answer::AnswerCard);
    } else if hash == "#result" {
        leptos::mount::mount_to_body(result::ResultCard);
    } else {
        leptos::mount::mount_to_body(app::App);
    }
}
