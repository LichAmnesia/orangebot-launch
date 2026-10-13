use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub has_key: bool,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchSettings {
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub has_key: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub asr: Option<Connection>,
    #[serde(default)]
    pub text: Option<Connection>,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub managed_recipe: Option<String>,
    #[serde(default = "default_translate_hotkey")]
    pub translate_hotkey: String,
    #[serde(default = "default_ask_hotkey")]
    pub ask_hotkey: String,
    #[serde(default = "default_rewrite_hotkey")]
    pub rewrite_hotkey: String,
    #[serde(default = "default_activation")]
    pub activation: String,
    #[serde(default = "default_translate_target")]
    pub translate_target: String,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default)]
    pub prompt_dictate: Option<String>,
    #[serde(default)]
    pub prompt_translate: Option<String>,
    #[serde(default)]
    pub prompt_ask: Option<String>,
    #[serde(default)]
    pub prompt_rewrite: Option<String>,
    #[serde(default)]
    pub input_device: Option<String>,
    #[serde(default = "default_true")]
    pub polish: bool,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub asr_fallback: Option<Connection>,
    #[serde(default)]
    pub text_fallback: Option<Connection>,
    #[serde(default = "default_targets")]
    pub translate_targets: Vec<String>,
    #[serde(default)]
    pub meetings_follow_retention: bool,
    #[serde(default)]
    pub search: Option<SearchSettings>,
    #[serde(default)]
    pub style_enabled: bool,
    #[serde(default)]
    pub style_notes: String,
    #[serde(default)]
    pub style_learned_at_ms: Option<i64>,
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            asr: None,
            text: None,
            hotkey: default_hotkey(),
            managed_recipe: None,
            translate_hotkey: default_translate_hotkey(),
            ask_hotkey: default_ask_hotkey(),
            rewrite_hotkey: default_rewrite_hotkey(),
            activation: default_activation(),
            translate_target: default_translate_target(),
            retention_days: default_retention_days(),
            prompt_dictate: None,
            prompt_translate: None,
            prompt_ask: None,
            prompt_rewrite: None,
            input_device: None,
            polish: default_true(),
            launch_at_login: false,
            asr_fallback: None,
            text_fallback: None,
            translate_targets: default_targets(),
            meetings_follow_retention: false,
            search: None,
            style_enabled: false,
            style_notes: String::new(),
            style_learned_at_ms: None,
            ui_language: default_ui_language(),
        }
    }
}

fn default_hotkey() -> String {
    "RightShift".into()
}

fn default_translate_hotkey() -> String {
    "RightOption".into()
}

fn default_ask_hotkey() -> String {
    "RightCommand".into()
}

fn default_rewrite_hotkey() -> String {
    "RightControl".into()
}

fn default_activation() -> String {
    "hold".into()
}

fn default_translate_target() -> String {
    "English".into()
}

fn default_retention_days() -> u32 {
    30
}

fn default_true() -> bool {
    true
}

fn default_targets() -> Vec<String> {
    vec!["English".into()]
}

fn default_ui_language() -> String {
    "system".into()
}

pub fn settings_path() -> PathBuf {
    ob_domain::paths::data_dir().join("settings.toml")
}

pub fn load() -> Settings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|raw| toml::from_str(&raw).ok())
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn save(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(settings).map_err(std::io::Error::other)?;
    std::fs::write(path, raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip_with_asr() {
        let settings = Settings {
            asr: Some(Connection {
                base_url: "http://127.0.0.1:8321/v1".to_string(),
                model: "sensevoice".to_string(),
                has_key: false,
            }),
            text: None,
            hotkey: "Cmd+Shift+Space".to_string(),
            managed_recipe: None,
            ..Settings::default()
        };
        let raw = toml::to_string_pretty(&settings).unwrap();
        let back: Settings = toml::from_str(&raw).unwrap();
        assert_eq!(back, settings);
    }

    #[test]
    fn empty_toml_gives_new_defaults() {
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(settings.translate_hotkey, "RightOption");
        assert_eq!(settings.ask_hotkey, "RightCommand");
        assert_eq!(settings.rewrite_hotkey, "RightControl");
        assert_eq!(settings.prompt_rewrite, None);
        assert_eq!(settings.activation, "hold");
        assert_eq!(settings.translate_target, "English");
        assert_eq!(settings.retention_days, 30);
    }

    #[test]
    fn empty_toml_gives_mic_general_defaults() {
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(settings.input_device, None);
        assert!(settings.polish);
        assert!(!settings.launch_at_login);
    }

    #[test]
    fn empty_toml_gives_default_hotkey() {
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(settings.hotkey, "RightShift");
        assert_eq!(settings, Settings::default());
    }
}
