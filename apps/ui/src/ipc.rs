use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        js_namespace = ["window", "__TAURI__", "core"],
        js_name = invoke,
        catch
    )]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(
        js_namespace = ["window", "__TAURI__", "event"],
        js_name = listen,
        catch
    )]
    async fn tauri_listen(
        event: &str,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<JsValue, JsValue>;
}

pub fn in_desktop() -> bool {
    js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("__TAURI__")).unwrap_or(false)
}

pub async fn call<A: serde::Serialize, R: serde::de::DeserializeOwned>(
    cmd: &str,
    args: &A,
) -> Result<R, String> {
    if !in_desktop() {
        return Err("Open this window from the Launch app.".into());
    }
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let v = tauri_invoke(cmd, args)
        .await
        .map_err(|js| js.as_string().unwrap_or_else(|| format!("{js:?}")))?;
    serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct NoArgs {}

pub async fn listen<T: serde::de::DeserializeOwned + 'static>(
    event: &str,
    mut f: impl FnMut(T) + 'static,
) {
    if !in_desktop() {
        return;
    }
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
        let payload =
            js_sys::Reflect::get(&ev, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
        if let Ok(v) = serde_wasm_bindgen::from_value::<T>(payload) {
            f(v)
        }
    });
    let _ = tauri_listen(event, &closure).await;
    closure.forget();
}
