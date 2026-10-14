//! Launch Meeting: live transcription of long recordings — text only, audio is never stored.

pub mod chunker;
pub mod transcript;

pub use chunker::*;
pub use transcript::*;

use ob_tools::{Capability, Tool, ToolManifest};

/// Manifest of the meeting tool.
pub static MEETING_MANIFEST: ToolManifest = ToolManifest {
    id: "meeting",
    name: "Launch Meeting",
    version: env!("CARGO_PKG_VERSION"),
    requires: &[Capability::Asr],
};

/// Tool entry point: chunked ASR over a long recording.
#[derive(Debug, Default)]
pub struct MeetingTool;

impl Tool for MeetingTool {
    fn manifest(&self) -> &ToolManifest {
        &MEETING_MANIFEST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_shape() {
        let tool = MeetingTool;
        let manifest = tool.manifest();
        assert_eq!(manifest.id, "meeting");
        assert_eq!(manifest.name, "Launch Meeting");
        assert_eq!(manifest.requires, &[Capability::Asr]);
    }
}
