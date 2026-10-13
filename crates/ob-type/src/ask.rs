use ob_providers::{ChatMessage, SearchHit};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskAction {
    Rewrite,
    Answer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResult {
    pub action: AskAction,
    pub text: String,
    pub search: Option<String>,
}

pub const ASK_ROUTER_SYSTEM: &str = "You help with text on the user's screen. You receive a spoken instruction and, optionally, the text the user selected. Reply with a single JSON object and nothing else: {\"action\": \"rewrite\" | \"answer\", \"text\": \"...\"}. Use \"rewrite\" only when a selection is given, it is editable, and the instruction asks to change it (rewrite, shorten, translate, fix, reformat); then \"text\" is the full replacement for the selection. Otherwise use \"answer\" and put a concise Markdown answer in \"text\". Never invent facts about the selection. If answering needs current or online information (news, prices, versions, schedules, \"latest\", \"search\"), and no selection is being rewritten, add \"search\": \"<short web query>\" to the JSON.";

const SELECTION_MAX_CHARS: usize = 8000;

#[derive(Deserialize)]
struct AskPayload {
    action: String,
    text: String,
    #[serde(default)]
    search: Option<String>,
}

pub fn build_ask_messages(
    instruction: &str,
    selection: Option<&str>,
    editable: bool,
) -> Vec<ChatMessage> {
    let mut user = format!("<instruction>\n{}\n</instruction>", instruction.trim());
    match selection {
        Some(sel) if !sel.is_empty() => {
            let truncated: String = sel.chars().take(SELECTION_MAX_CHARS).collect();
            user.push_str(&format!(
                "\n<selection editable=\"{editable}\">\n{truncated}\n</selection>"
            ));
        }
        _ => user.push_str("\n<selection>none</selection>"),
    }
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: ASK_ROUTER_SYSTEM.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user,
        },
    ]
}

fn strip_fence(s: &str) -> String {
    let t = s.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return s.to_string();
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    let rest = rest.trim_start_matches(['\r', '\n']);
    match rest.rfind("```") {
        Some(end) => rest[..end].trim().to_string(),
        None => rest.trim().to_string(),
    }
}

pub fn parse_ask_output(raw: &str, has_editable_selection: bool) -> AskResult {
    let s = strip_fence(&crate::clean_model_output(raw));
    let payload = s
        .find('{')
        .and_then(|start| s.rfind('}').map(|end| &s[start..=end]))
        .and_then(|slice| serde_json::from_str::<AskPayload>(slice).ok());
    match payload {
        Some(p) => {
            let action = if p.action == "rewrite" && has_editable_selection {
                AskAction::Rewrite
            } else {
                AskAction::Answer
            };
            AskResult {
                action,
                text: p.text.trim().to_string(),
                search: p
                    .search
                    .map(|q| q.trim().to_string())
                    .filter(|q| !q.is_empty()),
            }
        }
        None => AskResult {
            action: AskAction::Answer,
            text: s.trim().to_string(),
            search: None,
        },
    }
}

pub fn build_search_answer_messages(question: &str, hits: &[SearchHit]) -> Vec<ChatMessage> {
    let mut results = String::new();
    for (i, hit) in hits.iter().enumerate() {
        results.push_str(&format!(
            "[{}] {}\n{}\n{}\n",
            i + 1,
            hit.title,
            hit.url,
            hit.snippet
        ));
    }
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: "Answer the question using only the numbered search results. Cite sources inline as [1], [2]. If the results do not answer it, say so. Be concise; Markdown allowed.".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "<question>\n{}\n</question>\n<results>\n{}</results>",
                question.trim(),
                results
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn think_open() -> &'static str {
        concat!("<", "think", ">")
    }

    fn think_close() -> &'static str {
        concat!("</", "think", ">")
    }

    #[test]
    fn messages_with_selection() {
        let msgs = build_ask_messages("  shorten this  ", Some("hello world"), true);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, ASK_ROUTER_SYSTEM);
        assert_eq!(msgs[1].role, "user");
        assert_eq!(
            msgs[1].content,
            "<instruction>\nshorten this\n</instruction>\n<selection editable=\"true\">\nhello world\n</selection>"
        );
    }

    #[test]
    fn messages_without_selection() {
        let msgs = build_ask_messages("what is this", None, false);
        assert!(msgs[1].content.ends_with("\n<selection>none</selection>"));
        let empty = build_ask_messages("what is this", Some(""), false);
        assert!(empty[1].content.ends_with("\n<selection>none</selection>"));
        let ro = build_ask_messages("x", Some("sel"), false);
        assert!(ro[1].content.contains("<selection editable=\"false\">"));
    }

    #[test]
    fn selection_truncated_char_safe() {
        let sel = "字".repeat(SELECTION_MAX_CHARS + 10);
        let msgs = build_ask_messages("x", Some(&sel), true);
        let body = msgs[1]
            .content
            .split("<selection editable=\"true\">\n")
            .nth(1)
            .unwrap()
            .trim_end_matches("\n</selection>")
            .to_string();
        assert_eq!(body.chars().count(), SELECTION_MAX_CHARS);
    }

    #[test]
    fn parse_rewrite_with_editable_selection() {
        let r = parse_ask_output("{\"action\":\"rewrite\",\"text\":\" new text \"}", true);
        assert_eq!(r.action, AskAction::Rewrite);
        assert_eq!(r.text, "new text");
    }

    #[test]
    fn parse_rewrite_without_editable_selection_becomes_answer() {
        let r = parse_ask_output("{\"action\":\"rewrite\",\"text\":\"x\"}", false);
        assert_eq!(r.action, AskAction::Answer);
        assert_eq!(r.text, "x");
    }

    #[test]
    fn parse_fenced_json() {
        let raw = "```json\n{\"action\":\"answer\",\"text\":\"42\"}\n```";
        let r = parse_ask_output(raw, false);
        assert_eq!(r.action, AskAction::Answer);
        assert_eq!(r.text, "42");
        let plain_fence = "```\n{\"action\":\"answer\",\"text\":\"ok\"}\n```";
        assert_eq!(parse_ask_output(plain_fence, false).text, "ok");
    }

    #[test]
    fn parse_garbage_falls_back_to_answer() {
        let r = parse_ask_output("just some prose, no json at all", false);
        assert_eq!(r.action, AskAction::Answer);
        assert_eq!(r.text, "just some prose, no json at all");
    }

    #[test]
    fn parse_json_wrapped_in_reasoning_tags() {
        let raw = format!(
            "{}reasoning here{}{{\"action\":\"rewrite\",\"text\":\"fixed\"}}",
            think_open(),
            think_close()
        );
        let r = parse_ask_output(&raw, true);
        assert_eq!(r.action, AskAction::Rewrite);
        assert_eq!(r.text, "fixed");
    }

    #[test]
    fn parse_router_json_with_search() {
        let r = parse_ask_output(
            "{\"action\":\"answer\",\"text\":\"let me check\",\"search\":\" llm news \"}",
            false,
        );
        assert_eq!(r.action, AskAction::Answer);
        assert_eq!(r.search.as_deref(), Some("llm news"));
        let absent = parse_ask_output("{\"action\":\"answer\",\"text\":\"x\"}", false);
        assert_eq!(absent.search, None);
        let empty = parse_ask_output(
            "{\"action\":\"answer\",\"text\":\"x\",\"search\":\"\"}",
            false,
        );
        assert_eq!(empty.search, None);
    }

    #[test]
    fn parse_fallback_has_no_search() {
        let r = parse_ask_output("plain prose", false);
        assert_eq!(r.search, None);
    }

    #[test]
    fn search_answer_messages_number_two_hits() {
        let hits = vec![
            SearchHit {
                title: "T1".to_string(),
                url: "http://a".to_string(),
                snippet: "S1".to_string(),
            },
            SearchHit {
                title: "T2".to_string(),
                url: "http://b".to_string(),
                snippet: "S2".to_string(),
            },
        ];
        let msgs = build_search_answer_messages("  what is new?  ", &hits);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("Cite sources inline as [1], [2]"));
        assert!(
            msgs[1]
                .content
                .contains("<question>\nwhat is new?\n</question>")
        );
        assert!(msgs[1].content.contains("[1] T1\nhttp://a\nS1"));
        assert!(msgs[1].content.contains("[2] T2\nhttp://b\nS2"));
        assert!(msgs[1].content.ends_with("</results>"));
    }
}
