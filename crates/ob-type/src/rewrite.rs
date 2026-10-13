use ob_providers::ChatMessage;

pub const REWRITE_SYSTEM: &str = "You rewrite the user's selected text according to their spoken instruction. Keep the meaning, facts, names, numbers and the original language unless the instruction says otherwise. Output only the rewritten text — no preamble, no quotes, no explanation.";

pub const DEFAULT_REWRITE_INSTRUCTION: &str = "Improve clarity and fix grammar and punctuation.";

const SELECTION_MAX_CHARS: usize = 12_000;

pub fn build_rewrite_messages(
    instruction: &str,
    selection: &str,
    system_override: Option<&str>,
    style: Option<&str>,
) -> Vec<ChatMessage> {
    let mut system = system_override
        .filter(|text| !text.is_empty())
        .unwrap_or(REWRITE_SYSTEM)
        .to_string();
    if let Some(notes) = style.filter(|text| !text.is_empty()) {
        system = crate::style::append_style(&system, notes);
    }
    let instruction = instruction.trim();
    let instruction = if instruction.is_empty() {
        DEFAULT_REWRITE_INSTRUCTION
    } else {
        instruction
    };
    let truncated: String = selection.chars().take(SELECTION_MAX_CHARS).collect();
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: system,
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "<instruction>\n{instruction}\n</instruction>\n<selection>\n{truncated}\n</selection>"
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_instruction_uses_default() {
        let msgs = build_rewrite_messages("   ", "hello world", None, None);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, REWRITE_SYSTEM);
        assert_eq!(
            msgs[1].content,
            format!(
                "<instruction>\n{DEFAULT_REWRITE_INSTRUCTION}\n</instruction>\n<selection>\nhello world\n</selection>"
            )
        );
        let blank = build_rewrite_messages("", "x", None, None);
        assert!(blank[1].content.contains(DEFAULT_REWRITE_INSTRUCTION));
    }

    #[test]
    fn override_replaces_system() {
        let msgs = build_rewrite_messages("shorter", "sel", Some("Custom."), None);
        assert_eq!(msgs[0].content, "Custom.");
        assert!(!msgs[0].content.contains("You rewrite"));
        assert!(
            msgs[1]
                .content
                .contains("<instruction>\nshorter\n</instruction>")
        );
    }

    #[test]
    fn empty_override_falls_back_and_style_appends() {
        let msgs = build_rewrite_messages("x", "sel", Some(""), None);
        assert_eq!(msgs[0].content, REWRITE_SYSTEM);
        let styled = build_rewrite_messages("x", "sel", None, Some("- Terse."));
        assert!(styled[0].content.starts_with(REWRITE_SYSTEM));
        assert!(styled[0].content.contains("- Terse."));
    }

    #[test]
    fn selection_truncated_char_safe() {
        let sel = "字".repeat(SELECTION_MAX_CHARS + 50);
        let msgs = build_rewrite_messages("x", &sel, None, None);
        let body = msgs[1]
            .content
            .split("<selection>\n")
            .nth(1)
            .unwrap()
            .trim_end_matches("\n</selection>")
            .to_string();
        assert_eq!(body.chars().count(), SELECTION_MAX_CHARS);
    }
}
