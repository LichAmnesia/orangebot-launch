use serde_json::Value;

fn directive<'a>(csp: &'a str, prefix: &str) -> Option<&'a str> {
    csp.split(';')
        .map(str::trim)
        .find(|part| part.starts_with(prefix))
}

#[test]
fn csp_allows_wasm_ui_and_ipc() {
    let raw = include_str!("../tauri.conf.json");
    let conf: Value = serde_json::from_str(raw).expect("tauri.conf.json parses");
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("app.security.csp is a string");
    assert!(
        csp.contains("'wasm-unsafe-eval'"),
        "script-src must allow 'wasm-unsafe-eval' or the WebAssembly UI never instantiates: {csp}"
    );
    let connect = directive(csp, "connect-src").expect("connect-src directive exists");
    assert!(
        connect.contains("'self'"),
        "connect-src must include 'self' or the .wasm fetch is blocked: {connect}"
    );
    assert!(
        directive(csp, "script-src").is_some(),
        "script-src directive exists: {csp}"
    );
}
