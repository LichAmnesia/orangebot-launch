//! Text-only transcript of a meeting. Raw audio is never stored.

/// One recognised stretch of audio on the master timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub seq: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Ordered text of the whole meeting.
#[derive(Debug, Default, Clone)]
pub struct Transcript {
    pub segments: Vec<Segment>,
}

impl Transcript {
    /// Append text; empty or whitespace-only input is dropped.
    pub fn push(&mut self, start_ms: u64, end_ms: u64, text: &str) -> Option<&Segment> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let seq = self.segments.len() as u32;
        self.segments.push(Segment {
            seq,
            start_ms,
            end_ms,
            text: text.to_string(),
        });
        self.segments.last()
    }

    /// Last `max_chars` characters of the transcript, for prompting.
    pub fn tail(&self, max_chars: usize) -> String {
        let joined = self
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let chars: Vec<char> = joined.chars().collect();
        chars[chars.len().saturating_sub(max_chars)..]
            .iter()
            .collect()
    }

    /// Markdown rendering with one timestamped line per segment.
    pub fn to_markdown(&self, title: &str) -> String {
        let mut out = format!("# {title}\n\n");
        for segment in &self.segments {
            out.push_str(&format!(
                "**[{}]** {}\n\n",
                clock(segment.start_ms),
                segment.text
            ));
        }
        out
    }
}

fn clock(ms: u64) -> String {
    format!("{:02}:{:02}", ms / 60_000, (ms / 1_000) % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_numbers_and_skips_blanks() {
        let mut transcript = Transcript::default();
        assert!(transcript.push(0, 1_000, "   ").is_none());
        let first = transcript.push(0, 1_000, "  hello  ").expect("text kept");
        assert_eq!(first.seq, 0);
        assert_eq!(first.text, "hello");
        let second = transcript.push(1_000, 2_000, "world").expect("text kept");
        assert_eq!(second.seq, 1);
        assert_eq!(transcript.segments.len(), 2);
    }

    #[test]
    fn tail_is_char_safe() {
        let mut transcript = Transcript::default();
        transcript.push(0, 1_000, "会议纪要");
        transcript.push(1_000, 2_000, "已生成");
        assert_eq!(transcript.tail(100), "会议纪要 已生成");
        assert_eq!(transcript.tail(3), "已生成");
        assert_eq!(transcript.tail(0), "");
    }

    #[test]
    fn markdown_has_timestamps() {
        let mut transcript = Transcript::default();
        transcript.push(65_000, 68_000, "决定周五发布");
        transcript.push(0, 1_000, "开头");
        let md = transcript.to_markdown("Standup");
        assert!(md.starts_with("# Standup\n\n"));
        assert!(md.contains("**[01:05]** 决定周五发布\n\n"));
        assert!(md.ends_with("**[00:00]** 开头\n\n"));
    }
}
