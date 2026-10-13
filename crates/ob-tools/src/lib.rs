//! Tool trait, manifests, capabilities and registry (extension point).

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    Asr,
    Text,
    Tts,
    Ocr,
    Embedding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolManifest {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub requires: &'static [Capability],
}

pub trait Tool: Send + Sync {
    fn manifest(&self) -> &ToolManifest;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("tool id {0} already registered")]
    Duplicate(String),
    #[error("invalid tool id {0}")]
    InvalidId(String),
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<&'static str, Box<dyn Tool>>,
}

fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> Result<(), RegistryError> {
        let id = tool.manifest().id;
        if !is_valid_id(id) {
            return Err(RegistryError::InvalidId(id.to_string()));
        }
        if self.tools.contains_key(id) {
            return Err(RegistryError::Duplicate(id.to_string()));
        }
        self.tools.insert(id, tool);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&dyn Tool> {
        self.tools.get(id).map(Box::as_ref)
    }

    pub fn list(&self) -> Vec<&ToolManifest> {
        self.tools.values().map(|t| t.manifest()).collect()
    }

    pub fn missing_capabilities(
        &self,
        id: &str,
        available: &[Capability],
    ) -> Option<Vec<Capability>> {
        let tool = self.get(id)?;
        let missing: BTreeSet<Capability> = tool
            .manifest()
            .requires
            .iter()
            .copied()
            .filter(|c| !available.contains(c))
            .collect();
        Some(missing.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy {
        manifest: &'static ToolManifest,
    }

    impl Tool for Dummy {
        fn manifest(&self) -> &ToolManifest {
            self.manifest
        }
    }

    const TYPE_MANIFEST: ToolManifest = ToolManifest {
        id: "type",
        name: "Type",
        version: "0.1.0",
        requires: &[Capability::Asr, Capability::Text],
    };

    const READ_MANIFEST: ToolManifest = ToolManifest {
        id: "read",
        name: "Read",
        version: "0.1.0",
        requires: &[Capability::Tts],
    };

    fn dummy(manifest: &'static ToolManifest) -> Box<dyn Tool> {
        Box::new(Dummy { manifest })
    }

    #[test]
    fn list_is_sorted_by_id() {
        let mut reg = ToolRegistry::new();
        reg.register(dummy(&TYPE_MANIFEST)).unwrap();
        reg.register(dummy(&READ_MANIFEST)).unwrap();
        let ids: Vec<&str> = reg.list().iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["read", "type"]);
    }

    #[test]
    fn duplicate_id_rejected() {
        let mut reg = ToolRegistry::new();
        reg.register(dummy(&TYPE_MANIFEST)).unwrap();
        let err = reg.register(dummy(&TYPE_MANIFEST)).unwrap_err();
        assert_eq!(err, RegistryError::Duplicate("type".to_string()));
    }

    #[test]
    fn invalid_ids_rejected() {
        const BAD_UPPER: ToolManifest = ToolManifest {
            id: "Type",
            name: "",
            version: "",
            requires: &[],
        };
        const BAD_DIGIT_START: ToolManifest = ToolManifest {
            id: "1x",
            name: "",
            version: "",
            requires: &[],
        };
        const BAD_EMPTY: ToolManifest = ToolManifest {
            id: "",
            name: "",
            version: "",
            requires: &[],
        };
        let mut reg = ToolRegistry::new();
        assert_eq!(
            reg.register(dummy(&BAD_UPPER)).unwrap_err(),
            RegistryError::InvalidId("Type".to_string())
        );
        assert_eq!(
            reg.register(dummy(&BAD_DIGIT_START)).unwrap_err(),
            RegistryError::InvalidId("1x".to_string())
        );
        assert_eq!(
            reg.register(dummy(&BAD_EMPTY)).unwrap_err(),
            RegistryError::InvalidId(String::new())
        );
    }

    #[test]
    fn missing_capabilities_reported() {
        let mut reg = ToolRegistry::new();
        reg.register(dummy(&TYPE_MANIFEST)).unwrap();
        let missing = reg.missing_capabilities("type", &[Capability::Asr]);
        assert_eq!(missing, Some(vec![Capability::Text]));
    }

    #[test]
    fn missing_capabilities_unknown_tool_is_none() {
        let reg = ToolRegistry::new();
        assert_eq!(reg.missing_capabilities("nope", &[]), None);
    }
}
