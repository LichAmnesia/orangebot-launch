//! Writing-style preference learning and injection.

use ob_providers::ChatMessage;

pub const STYLE_LEARN_SYSTEM: &str = "You read samples of text a person wrote by voice and accepted. Describe their writing style as at most 8 short bullet points: tone and formality, sentence length, punctuation habits, capitalisation, emoji use, how they mix languages, recurring sign-offs or phrases. Describe patterns only — never quote names, numbers, addresses or other personal details from the samples. Output only the bullet list.";

const SAMPLE_MAX_CHARS: usize = 600;
const SAMPLE_MAX_COUNT: usize = 60;
const STYLE_HEADER: &str =
    "\n\nThe user's style preferences (follow them unless the instruction says otherwise):\n";

pub fn build_style_learn_messages(samples: &[String]) -> Vec<ChatMessage> {
    let joined: Vec<String> = samples
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let truncated: String = s.chars().take(SAMPLE_MAX_CHARS).collect();
            format!("<sample>{truncated}</sample>")
        })
        .take(SAMPLE_MAX_COUNT)
        .collect();
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: STYLE_LEARN_SYSTEM.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: joined.join("\n"),
        },
    ]
}

pub fn append_style(system: &str, notes: &str) -> String {
    let notes = notes.trim();
    if notes.is_empty() {
        return system.to_string();
    }
    format!("{system}{STYLE_HEADER}{notes}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> String {
        text.to_string()
    }

    #[test]
    fn learn_messages_wrap_trim_and_drop() {
        let msgs = build_style_learn_messages(&[s("  hello there  "), s("   "), s("second note")]);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, STYLE_LEARN_SYSTEM);
        assert_eq!(
            msgs[1].content,
            "<sample>hello there</sample>\n<sample>second note</sample>"
        );
    }

    #[test]
    fn learn_messages_truncate_sample_char_safe() {
        let long = "字".repeat(SAMPLE_MAX_CHARS + 50);
        let msgs = build_style_learn_messages(&[long]);
        let body = msgs[1]
            .content
            .trim_start_matches("<sample>")
            .trim_end_matches("</sample>")
            .to_string();
        assert_eq!(body.chars().count(), SAMPLE_MAX_CHARS);
    }

    #[test]
    fn learn_messages_cap_at_sixty() {
        let samples: Vec<String> = (0..70).map(|i| format!("n{i}")).collect();
        let msgs = build_style_learn_messages(&samples);
        assert_eq!(msgs[1].content.lines().count(), SAMPLE_MAX_COUNT);
        assert!(msgs[1].content.contains("<sample>n59</sample>"));
        assert!(!msgs[1].content.contains("<sample>n60</sample>"));
    }

    #[test]
    fn append_style_empty_notes_unchanged() {
        assert_eq!(append_style("sys", ""), "sys");
        assert_eq!(append_style("sys", "  \n "), "sys");
    }

    #[test]
    fn append_style_joins_trimmed_notes() {
        let out = append_style("sys", "\n- short sentences\n ");
        assert_eq!(
            out,
            "sys\n\nThe user's style preferences (follow them unless the instruction says otherwise):\n- short sentences"
        );
    }
}
