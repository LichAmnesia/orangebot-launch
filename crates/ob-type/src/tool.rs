use ob_tools::{Capability, Tool, ToolManifest};

pub static TYPE_MANIFEST: ToolManifest = ToolManifest {
    id: "type",
    name: "Launch Type",
    version: env!("CARGO_PKG_VERSION"),
    requires: &[Capability::Asr, Capability::Text],
};

#[derive(Debug, Default)]
pub struct TypeTool;

impl Tool for TypeTool {
    fn manifest(&self) -> &ToolManifest {
        &TYPE_MANIFEST
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ob_tools::ToolRegistry;

    #[test]
    fn registers_and_reports_missing_capabilities() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(TypeTool)).unwrap();
        assert!(registry.get("type").is_some());
        assert_eq!(
            registry.missing_capabilities("type", &[Capability::Asr]),
            Some(vec![Capability::Text])
        );
    }
}
