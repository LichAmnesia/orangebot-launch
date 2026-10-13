use leptos::prelude::*;

pub fn key_label(part: &str) -> String {
    match part {
        "Alt" => "⌥".into(),
        "Control" | "Ctrl" => "⌃".into(),
        "Shift" => "⇧".into(),
        "Super" | "Cmd" | "Meta" => "⌘".into(),
        "Space" => "Space".into(),
        "RightShift" => "Right ⇧".into(),
        "RightOption" => "Right ⌥".into(),
        "RightCommand" => "Right ⌘".into(),
        "RightControl" => "Right ⌃".into(),
        "Fn" => "fn".into(),
        _ => part.to_string(),
    }
}

#[component]
pub fn Keys(combo: String) -> impl IntoView {
    combo
        .split('+')
        .map(|p| {
            let part = p.trim();
            view! { <span class="keycap">{key_label(part)}</span> }
        })
        .collect_view()
}
