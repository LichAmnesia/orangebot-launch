//! Typed IPC contract shared by the desktop shell and the Rust UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeInfo {
    pub id: String,
    pub asr_model: String,
    pub text_model: String,
    pub total_bytes: u64,
    pub target_ram_gib: u32,
    pub qualification: String,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub role: String,
    pub license: String,
    pub total_bytes: u64,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadyState {
    Ready,
    NotConfigured,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointStatus {
    pub role: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub state: ReadyState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppStatus {
    pub version: String,
    pub tools: Vec<ToolInfo>,
    pub asr: EndpointStatus,
    pub text: EndpointStatus,
    pub hotkey: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub created_at_ms: i64,
    pub mode: String,
    pub app: Option<String>,
    pub raw_text: String,
    pub final_text: String,
    pub delivery: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionInput {
    pub role: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Ok,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestStep {
    pub name: String,
    pub state: StepState,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestReport {
    pub endpoint: String,
    pub steps: Vec<TestStep>,
}

impl TestReport {
    pub fn passed(&self) -> bool {
        !self.steps.iter().any(|s| s.state == StepState::Failed)
            && self.steps.iter().any(|s| s.state == StepState::Ok)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadEvent {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceEvent {
    pub phase: String,
    pub level: f32,
    pub text: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingSegment {
    pub seq: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingInfo {
    pub id: String,
    pub title: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub segment_count: u32,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingDetail {
    pub info: MeetingInfo,
    pub segments: Vec<MeetingSegment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingEvent {
    pub meeting_id: String,
    pub state: String,
    pub level: f32,
    pub elapsed_ms: u64,
    pub segment: Option<MeetingSegment>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub total_ram_gib: u32,
    pub free_disk_gib: u32,
    pub chip: String,
    pub recommended_recipe: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub microphone: String,
    pub input_monitoring: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictEntryInfo {
    pub id: i64,
    pub phrase: String,
    pub aliases: Vec<String>,
    pub source: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotkeyStatus {
    pub mode: String,
    pub hotkey: String,
    pub state: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotkeySettings {
    pub dictate: String,
    pub translate: String,
    pub ask: String,
    pub rewrite: String,
    pub activation: String,
    pub translate_target: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeInfo {
    pub id: String,
    pub name: String,
    pub hotkey: String,
    pub default_prompt: String,
    pub prompt: String,
    pub customized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerEvent {
    pub question: String,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub done: bool,
    pub scope: String,
    pub can_replace: bool,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultEvent {
    pub raw: String,
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadState {
    pub model_id: String,
    pub state: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayCount {
    pub date: String,
    pub sessions: u32,
    pub words: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub words_week: u64,
    pub minutes_week: f64,
    pub sessions_week: u32,
    pub words_total: u64,
    pub streak_days: u32,
    pub days: Vec<DayCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicInfo {
    pub devices: Vec<String>,
    pub selected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicTest {
    pub peak: f32,
    pub duration_ms: u64,
    pub device: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagReport {
    pub version: String,
    pub checks: Vec<DiagCheck>,
    pub asr_log_tail: String,
    pub llm_log_tail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub polish: bool,
    pub launch_at_login: bool,
    pub retention_days: u32,
    pub meetings_follow_retention: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchSettingsInfo {
    pub kind: String,
    pub base_url: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchInput {
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleInfo {
    pub enabled: bool,
    pub notes: String,
    pub learned_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current: String,
    pub available: Option<String>,
    pub notes: Option<String>,
}

pub mod cmd {
    pub const APP_STATUS: &str = "app_status";
    pub const LIST_RECIPES: &str = "list_recipes";
    pub const LIST_MODELS: &str = "list_models";
    pub const LIST_HISTORY: &str = "list_history";
    pub const TEST_CONNECTION: &str = "test_connection";
    pub const SAVE_CONNECTION: &str = "save_connection";
    pub const DOWNLOAD_MODEL: &str = "download_model";
    pub const USE_RECIPE: &str = "use_recipe";
    pub const START_MEETING: &str = "start_meeting";
    pub const STOP_MEETING: &str = "stop_meeting";
    pub const LIST_MEETINGS: &str = "list_meetings";
    pub const GET_MEETING: &str = "get_meeting";
    pub const DELETE_MEETING: &str = "delete_meeting";
    pub const MEETING_MARKDOWN: &str = "meeting_markdown";
    pub const SYSTEM_INFO: &str = "system_info";
    pub const PERMISSION_STATUS: &str = "permission_status";
    pub const OPEN_PRIVACY_SETTINGS: &str = "open_privacy_settings";
    pub const LIST_DICTIONARY: &str = "list_dictionary";
    pub const ADD_DICTIONARY_ENTRY: &str = "add_dictionary_entry";
    pub const SET_DICTIONARY_ENABLED: &str = "set_dictionary_enabled";
    pub const DELETE_DICTIONARY_ENTRY: &str = "delete_dictionary_entry";
    pub const GET_HOTKEYS: &str = "get_hotkeys";
    pub const HOTKEY_STATUS: &str = "hotkey_status";
    pub const REQUEST_ACCESSIBILITY: &str = "request_accessibility";
    pub const SET_HOTKEYS: &str = "set_hotkeys";
    pub const DELETE_ALL_HISTORY: &str = "delete_all_history";
    pub const GET_MODES: &str = "get_modes";
    pub const SAVE_MODE_PROMPT: &str = "save_mode_prompt";
    pub const TEST_MODE: &str = "test_mode";
    pub const LIST_MICS: &str = "list_mics";
    pub const SET_MIC: &str = "set_mic";
    pub const TEST_MIC: &str = "test_mic";
    pub const RUN_DIAGNOSTICS: &str = "run_diagnostics";
    pub const GET_GENERAL: &str = "get_general";
    pub const SET_GENERAL: &str = "set_general";
    pub const SUSPEND_HOTKEYS: &str = "suspend_hotkeys";
    pub const RESUME_HOTKEYS: &str = "resume_hotkeys";
    pub const REPLACE_SELECTION: &str = "replace_selection";
    pub const RESULT_INSERT: &str = "result_insert";
    pub const RESULT_RETRY: &str = "result_retry";
    pub const LIST_DOWNLOADS: &str = "list_downloads";
    pub const PAUSE_DOWNLOAD: &str = "pause_download";
    pub const RESUME_DOWNLOAD: &str = "resume_download";
    pub const CANCEL_DOWNLOAD: &str = "cancel_download";
    pub const GET_STATS: &str = "get_stats";
    pub const GET_TRANSLATE_TARGETS: &str = "get_translate_targets";
    pub const SET_TRANSLATE_TARGETS: &str = "set_translate_targets";
    pub const GET_SEARCH: &str = "get_search";
    pub const SAVE_SEARCH: &str = "save_search";
    pub const TEST_SEARCH: &str = "test_search";
    pub const GET_STYLE: &str = "get_style";
    pub const SAVE_STYLE: &str = "save_style";
    pub const LEARN_STYLE: &str = "learn_style";
    pub const CHECK_UPDATE: &str = "check_update";
    pub const INSTALL_UPDATE: &str = "install_update";
    pub const GET_UI_LANGUAGE: &str = "get_ui_language";
    pub const SET_UI_LANGUAGE: &str = "set_ui_language";
}

pub mod event {
    pub const DOWNLOAD: &str = "download-progress";
    pub const VOICE: &str = "voice-event";
    pub const MEETING: &str = "meeting-event";
    pub const STATUS: &str = "status-changed";
    pub const ANSWER: &str = "answer-event";
    pub const RESULT: &str = "result-event";
    pub const DOWNLOADS: &str = "downloads-changed";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(role: &str) -> EndpointStatus {
        EndpointStatus {
            role: role.to_string(),
            base_url: Some("http://localhost:8004/v1".to_string()),
            model: Some("qwen".to_string()),
            state: ReadyState::Ready,
        }
    }

    fn sample_status() -> AppStatus {
        AppStatus {
            version: "0.1.0".to_string(),
            tools: vec![ToolInfo {
                id: "whisper_cpp".to_string(),
                name: "whisper.cpp".to_string(),
                version: "1.2.3".to_string(),
                requires: vec!["whisper-turbo-q5".to_string()],
            }],
            asr: endpoint("asr"),
            text: endpoint("text"),
            hotkey: "Cmd+Shift+Space".to_string(),
        }
    }

    fn steps(states: &[StepState]) -> TestReport {
        TestReport {
            endpoint: "http://localhost:8004/v1".to_string(),
            steps: states
                .iter()
                .enumerate()
                .map(|(i, state)| TestStep {
                    name: format!("step{i}"),
                    state: *state,
                    detail: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn app_status_roundtrip() {
        let status = sample_status();
        let json = serde_json::to_string(&status).unwrap();
        let back: AppStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, back);
    }

    #[test]
    fn ready_state_is_snake_case() {
        let json = serde_json::to_string(&ReadyState::NotConfigured).unwrap();
        assert_eq!(json, "\"not_configured\"");
    }

    #[test]
    fn test_report_passed_rules() {
        assert!(steps(&[StepState::Ok, StepState::Skipped]).passed());
        assert!(!steps(&[StepState::Ok, StepState::Failed]).passed());
        assert!(!steps(&[StepState::Skipped]).passed());
    }

    #[test]
    fn meeting_event_roundtrip_with_segment() {
        let event = MeetingEvent {
            meeting_id: "m-1".to_string(),
            state: "transcribing".to_string(),
            level: 0.25,
            elapsed_ms: 12_000,
            segment: Some(MeetingSegment {
                seq: 2,
                start_ms: 10_000,
                end_ms: 12_000,
                text: "hello".to_string(),
            }),
            error: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: MeetingEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }
}
