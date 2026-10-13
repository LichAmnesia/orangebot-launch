//! Configuration schema: the app config file and its validation rules.

use serde::{Deserialize, Serialize};

/// Current config schema version accepted by this crate.
pub const SCHEMA_VERSION: u32 = 1;

/// Root of the application configuration file.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub privacy: Privacy,
    pub retention: Retention,
    pub managed_runtime: ManagedRuntime,
    #[serde(default)]
    pub profiles: Vec<ProviderProfile>,
    pub routing: Routing,
}

/// Privacy switches.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Privacy {
    pub strict_local: bool,
    pub allow_cloud_fallback: bool,
    pub cursor_context: bool,
    pub screenshot_context: bool,
}

/// How long captured media is kept.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Retention {
    pub text_days: u32,
    pub audio_days: u32,
}

/// Settings for the runtime that hosts managed models.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ManagedRuntime {
    pub llm_bind_host: String,
    pub llm_port: u16,
    pub asr_api_enabled: bool,
    pub asr_api_bind_host: String,
    pub asr_api_port: u16,
    pub idle_unload_minutes: u32,
}

/// What a provider profile is able to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Asr,
    Text,
}

/// Where the provider lives relative to this app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    Managed,
    ExternalLocal,
    Lan,
    Cloud,
}

/// Wire protocol used to talk to the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    WorkerIpc,
    OpenaiChat,
    OpenaiTranscription,
}

/// A named provider the router can select.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ProviderProfile {
    pub id: String,
    pub role: Role,
    pub location: Location,
    pub protocol: Protocol,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_timeout_ms: Option<u64>,
}

/// Per-capability route table.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Routing {
    pub dictate: Route,
    pub translate: Route,
    pub ask: Route,
    pub learning: Route,
}

/// One routing entry: which profiles a capability may use.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Route {
    #[serde(default)]
    pub asr: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub target_languages: Vec<String>,
    #[serde(default)]
    pub search_enabled: bool,
}

/// Errors produced while loading or validating configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unsupported schema_version {0}")]
    UnsupportedSchema(u32),
    #[error("route {route} references unknown profile {id}")]
    UnknownProfile { route: String, id: String },
    #[error("profile {id} must have role {expected:?}")]
    RoleMismatch { id: String, expected: Role },
}

impl AppConfig {
    /// Parse, version-gate and validate a config document.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        let cfg: Self = toml::from_str(s)?;
        if cfg.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema(cfg.schema_version));
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// Check that every routed profile exists and has the expected role.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let routes = [
            ("dictate", &self.routing.dictate),
            ("translate", &self.routing.translate),
            ("ask", &self.routing.ask),
            ("learning", &self.routing.learning),
        ];
        for (name, route) in routes {
            if let Some(id) = &route.asr {
                self.check_role(name, id, Role::Asr)?;
            }
            if let Some(id) = &route.text {
                self.check_role(name, id, Role::Text)?;
            }
        }
        Ok(())
    }

    fn check_role(&self, route: &str, id: &str, expected: Role) -> Result<(), ConfigError> {
        let profile = self
            .profile(id)
            .ok_or_else(|| ConfigError::UnknownProfile {
                route: route.to_string(),
                id: id.to_string(),
            })?;
        if profile.role != expected {
            return Err(ConfigError::RoleMismatch {
                id: id.to_string(),
                expected,
            });
        }
        Ok(())
    }

    /// Look up a profile by id.
    pub fn profile(&self, id: &str) -> Option<&ProviderProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }
}

impl ProviderProfile {
    /// Join `base_url` and `route` with exactly one separator.
    pub fn endpoint_url(&self, route: &str) -> Option<String> {
        let base = self.base_url.as_ref()?;
        Some(format!(
            "{}/{}",
            base.trim_end_matches('/'),
            route.trim_start_matches('/')
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../docs/spec/config.example.toml");

    #[test]
    fn parses_example_config() {
        let cfg = AppConfig::from_toml_str(EXAMPLE).unwrap();
        assert_eq!(cfg.profiles.len(), 4);
        assert_eq!(
            cfg.routing.translate.target_languages,
            vec!["en-US".to_string(), "zh-CN".to_string()]
        );
    }

    #[test]
    fn unknown_profile_in_route() {
        let toml = EXAMPLE.replace(
            "text = \"managed-text\"\nsearch_enabled = false",
            "text = \"ghost\"\nsearch_enabled = false",
        );
        let err = AppConfig::from_toml_str(&toml).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownProfile { .. }));
    }

    #[test]
    fn role_mismatch_in_route() {
        let toml = EXAMPLE.replace(
            "text = \"managed-text\"\nfallback = \"none\"",
            "text = \"managed-asr\"\nfallback = \"none\"",
        );
        let err = AppConfig::from_toml_str(&toml).unwrap_err();
        assert!(matches!(err, ConfigError::RoleMismatch { .. }));
    }

    #[test]
    fn unsupported_schema_version() {
        let toml = EXAMPLE.replace("schema_version = 1", "schema_version = 2");
        let err = AppConfig::from_toml_str(&toml).unwrap_err();
        assert!(matches!(err, ConfigError::UnsupportedSchema(2)));
    }

    #[test]
    fn endpoint_url_joins_with_single_slash() {
        let cfg = AppConfig::from_toml_str(EXAMPLE).unwrap();
        let mut profile = cfg.profile("managed-text").unwrap().clone();
        profile.base_url = Some("http://h:1/v1/".to_string());
        assert_eq!(
            profile.endpoint_url("/models"),
            Some("http://h:1/v1/models".to_string())
        );
        profile.base_url = None;
        assert_eq!(profile.endpoint_url("/models"), None);
    }
}
