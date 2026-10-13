use crate::state::Mode;
use ob_providers::ChatMessage;

pub const DICTATE_SYSTEM: &str = "You clean up dictated text. Remove filler words (um, uh, like, you know, 嗯, 那个, 就是) and false starts. When the speaker corrects themselves (\"no wait\", \"sorry\", \"I mean\", \"不对\", \"是…\"), keep ONLY the corrected version and drop the words it replaces. Fix punctuation and casing. Keep the speaker's language and wording otherwise. Never add facts, never answer questions in the text, never translate. Output only the cleaned text.";

pub const DICTATE_EXAMPLES: &[(&str, &str)] = &[
    (
        "um we meet at, uh, 10am, no wait, 11am in room B",
        "We meet at 11 am in room B.",
    ),
    (
        "嗯，那个，把报告发给小王，不对，是发给小李",
        "把报告发给小李。",
    ),
    (
        "send it to Anna I mean Maria by tonight",
        "Send it to Maria by tonight.",
    ),
    (
        "what time is the review tomorrow",
        "What time is the review tomorrow?",
    ),
];

pub const TRANSLATE_SYSTEM: &str = "Translate the dictated text into {target}. Keep names, numbers, dates and amounts exact. Remove filler words. Output only the translation.";

pub const ASK_SYSTEM: &str = "You are a concise assistant. Answer the spoken request. If it asks you to write something, output only that text, with no preamble.";

const THINK_OPEN: &str = concat!("<", "think", ">");
const THINK_CLOSE: &str = concat!("</", "think", ">");

fn transcript_block(text: &str) -> String {
    format!("<transcript>\n{}\n</transcript>", text.trim())
}

pub fn default_system(mode: &Mode) -> String {
    match mode {
        Mode::Dictate => DICTATE_SYSTEM.to_string(),
        Mode::Translate { target } => TRANSLATE_SYSTEM.replace("{target}", target),
        Mode::Ask => ASK_SYSTEM.to_string(),
        Mode::Rewrite => crate::rewrite::REWRITE_SYSTEM.to_string(),
    }
}

pub fn build_messages(mode: &Mode, transcript: &str, dictionary: &[String]) -> Vec<ChatMessage> {
    build_messages_with(mode, transcript, dictionary, None)
}

pub fn build_messages_with(
    mode: &Mode,
    transcript: &str,
    dictionary: &[String],
    system_override: Option<&str>,
) -> Vec<ChatMessage> {
    build_messages_full(mode, transcript, dictionary, system_override, None)
}

pub fn build_messages_full(
    mode: &Mode,
    transcript: &str,
    dictionary: &[String],
    system_override: Option<&str>,
    style: Option<&str>,
) -> Vec<ChatMessage> {
    if matches!(mode, Mode::Rewrite) {
        return crate::rewrite::build_rewrite_messages(
            crate::rewrite::DEFAULT_REWRITE_INSTRUCTION,
            transcript,
            system_override,
            style,
        );
    }
    if let Some(override_text) = system_override.filter(|text| !text.is_empty()) {
        let mut system = match mode {
            Mode::Translate { target } => override_text.replace("{target}", target),
            _ => override_text.to_string(),
        };
        append_dictionary(&mut system, dictionary);
        if let Some(notes) = style.filter(|text| !text.is_empty()) {
            system = crate::style::append_style(&system, notes);
        }
        return vec![
            ChatMessage {
                role: "system".to_string(),
                content: system,
            },
            ChatMessage {
                role: "user".to_string(),
                content: transcript_block(transcript),
            },
        ];
    }

    let mut system = default_system(mode);
    append_dictionary(&mut system, dictionary);
    if let Some(notes) = style.filter(|text| !text.is_empty()) {
        system = crate::style::append_style(&system, notes);
    }
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system,
    }];
    if matches!(mode, Mode::Dictate) {
        for (input, output) in DICTATE_EXAMPLES {
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: transcript_block(input),
            });
            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: (*output).to_string(),
            });
        }
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: transcript_block(transcript),
    });
    messages
}

fn append_dictionary(system: &mut String, dictionary: &[String]) {
    if !dictionary.is_empty() {
        system.push_str("\n\nPreferred spellings: ");
        system.push_str(&dictionary.join(", "));
        system.push('.');
    }
}

pub fn clean_model_output(output: &str) -> String {
    let mut s = output.to_string();
    while let Some(start) = s.find(THINK_OPEN) {
        let rest = &s[start + THINK_OPEN.len()..];
        match rest.find(THINK_CLOSE) {
            Some(end) => {
                let stop = start + THINK_OPEN.len() + end + THINK_CLOSE.len();
                s.replace_range(start..stop, "");
            }
            None => s.replace_range(start.., ""),
        }
    }
    let mut t = s.trim();
    t = t.strip_prefix("<transcript>").unwrap_or(t);
    t = t.strip_suffix("</transcript>").unwrap_or(t);
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn dictate_messages_shape() {
        let msgs = build_messages(&Mode::Dictate, "  hello world  ", &[]);
        assert_eq!(msgs.len(), 2 + 2 * DICTATE_EXAMPLES.len());
        assert_eq!(msgs.len(), 10);
        assert_eq!(msgs[0], msg("system", DICTATE_SYSTEM));
        assert!(msgs[0].content.contains("Never add facts"));
        for (i, m) in msgs[1..msgs.len() - 1].iter().enumerate() {
            let expected = if i % 2 == 0 { "user" } else { "assistant" };
            assert_eq!(m.role, expected);
        }
        assert_eq!(
            msgs[1],
            msg(
                "user",
                "<transcript>\num we meet at, uh, 10am, no wait, 11am in room B\n</transcript>"
            )
        );
        assert_eq!(msgs[2], msg("assistant", "We meet at 11 am in room B."));
        assert_eq!(
            msgs[msgs.len() - 1],
            msg("user", "<transcript>\nhello world\n</transcript>")
        );
    }

    #[test]
    fn translate_replaces_target() {
        let msgs = build_messages(
            &Mode::Translate {
                target: "English".to_string(),
            },
            "你好",
            &[],
        );
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].content.contains("into English"));
        assert!(!msgs[0].content.contains("{target}"));
    }

    #[test]
    fn dictionary_appended_to_system() {
        let dict = vec!["OpenSeek".to_string(), "DeepSeek".to_string()];
        let msgs = build_messages(&Mode::Dictate, "hi", &dict);
        assert!(
            msgs[0]
                .content
                .ends_with("\n\nPreferred spellings: OpenSeek, DeepSeek.")
        );
    }

    #[test]
    fn override_replaces_system_and_drops_examples() {
        let msgs = build_messages_with(
            &Mode::Dictate,
            "hello",
            &["Foo".to_string()],
            Some("Custom rules."),
        );
        assert_eq!(msgs.len(), 2);
        assert_eq!(
            msgs[0].content,
            "Custom rules.\n\nPreferred spellings: Foo."
        );
    }

    #[test]
    fn none_override_keeps_ten_messages() {
        let msgs = build_messages_with(&Mode::Dictate, "hello", &[], None);
        assert_eq!(msgs.len(), 10);
    }

    #[test]
    fn style_appended_keeps_examples() {
        let msgs =
            build_messages_full(&Mode::Dictate, "hello", &[], None, Some("- Keep it terse."));
        assert_eq!(msgs.len(), 10);
        assert!(msgs[0].content.starts_with(DICTATE_SYSTEM));
        assert!(msgs[0].content.contains("- Keep it terse."));
    }

    #[test]
    fn translate_override_replaces_target() {
        let msgs = build_messages_with(
            &Mode::Translate {
                target: "German".to_string(),
            },
            "hi",
            &[],
            Some("Say it in {target}."),
        );
        assert_eq!(msgs[0].content, "Say it in German.");
    }

    #[test]
    fn strips_single_think_block() {
        let input = format!("{THINK_OPEN}x\ny{THINK_CLOSE}\n Hello ");
        assert_eq!(clean_model_output(&input), "Hello");
    }

    #[test]
    fn strips_two_think_blocks() {
        let input = format!("a{THINK_OPEN}x{THINK_CLOSE} {THINK_OPEN}y{THINK_CLOSE}b");
        assert_eq!(clean_model_output(&input), "a b");
    }

    #[test]
    fn unterminated_think_removes_rest() {
        let input = format!("{THINK_OPEN}abc");
        assert_eq!(clean_model_output(&input), "");
    }

    #[test]
    fn strips_echoed_transcript_tags() {
        assert_eq!(
            clean_model_output("<transcript>\nhello there\n</transcript>"),
            "hello there"
        );
    }
}
