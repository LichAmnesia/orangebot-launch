use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub purpose: String,
    pub checked_at: String,
    pub models: Vec<ModelEntry>,
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Asset {
    pub relative_path: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub role: String,
    pub repo: String,
    pub publisher: String,
    pub revision: String,
    pub license: String,
    pub source_card_url: String,
    pub assets: Vec<Asset>,
    pub runtime_id: String,
    pub runtime_revision: Option<String>,
    #[serde(default)]
    pub runtime_assets: Vec<Asset>,
    pub platform_targets: Vec<String>,
    pub qualification_status: String,
    pub runtime_smoke_verified: bool,
    pub full_asset_inventory_verified: bool,
    pub catalog_release_ready: bool,
    pub reason: String,
    #[serde(default)]
    pub required_auxiliary_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Recipe {
    pub id: String,
    pub asr: String,
    pub text: String,
    pub target_ram_gib: u32,
    pub qualification_status: String,
    pub one_click_ready: bool,
    pub ram_measured: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("recipe {recipe} references missing model {model}")]
    MissingModel { recipe: String, model: String },
    #[error("model {model} has role {found}, recipe {recipe} needs {expected}")]
    WrongRole {
        recipe: String,
        model: String,
        expected: String,
        found: String,
    },
    #[error("asset {path} of {model} has invalid sha256")]
    BadHash { model: String, path: String },
}

impl Catalog {
    pub fn from_json_str(s: &str) -> Result<Self, CatalogError> {
        let catalog: Catalog = serde_json::from_str(s)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        for recipe in &self.recipes {
            for (model_id, expected_role) in [(&recipe.asr, "asr"), (&recipe.text, "text")] {
                let model = self
                    .model(model_id)
                    .ok_or_else(|| CatalogError::MissingModel {
                        recipe: recipe.id.clone(),
                        model: model_id.clone(),
                    })?;
                if model.role != expected_role {
                    return Err(CatalogError::WrongRole {
                        recipe: recipe.id.clone(),
                        model: model_id.clone(),
                        expected: expected_role.to_string(),
                        found: model.role.clone(),
                    });
                }
            }
        }
        for model in &self.models {
            for asset in model.assets.iter().chain(model.runtime_assets.iter()) {
                if !is_valid_sha256(&asset.sha256) {
                    return Err(CatalogError::BadHash {
                        model: model.id.clone(),
                        path: asset.relative_path.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn model(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn recipe(&self, id: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }

    pub fn recipe_total_bytes(&self, recipe_id: &str) -> Option<u64> {
        let recipe = self.recipe(recipe_id)?;
        let asr = self.model(&recipe.asr)?;
        let text = self.model(&recipe.text)?;
        let sum = |m: &ModelEntry| -> u64 {
            m.assets
                .iter()
                .chain(m.runtime_assets.iter())
                .map(|a| a.bytes)
                .sum()
        };
        Some(sum(asr) + sum(text))
    }
}

fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = include_str!("../data/model-catalog.json");

    fn parsed() -> Catalog {
        Catalog::from_json_str(JSON).expect("candidate catalog should parse")
    }

    #[test]
    fn parses_candidate_catalog() {
        let catalog = parsed();
        assert_eq!(catalog.models.len(), 5);
        assert_eq!(catalog.recipes.len(), 4);
        catalog
            .validate()
            .expect("candidate catalog should validate");
    }

    #[test]
    fn recipe_total_bytes_matches_manual_sum() {
        let catalog = parsed();
        let recipe = catalog.recipe("local-light").expect("local-light");
        let asr = catalog.model(&recipe.asr).expect("asr model");
        let text = catalog.model(&recipe.text).expect("text model");
        let manual: u64 = asr
            .assets
            .iter()
            .chain(asr.runtime_assets.iter())
            .chain(text.assets.iter())
            .chain(text.runtime_assets.iter())
            .map(|a| a.bytes)
            .sum();
        assert_eq!(catalog.recipe_total_bytes("local-light"), Some(manual));
        assert_eq!(catalog.recipe_total_bytes("nope"), None);
    }

    #[test]
    fn missing_model_error() {
        let mut catalog = parsed();
        catalog.models.retain(|m| m.id != "whisper-turbo-q5");
        let err = catalog.validate().unwrap_err();
        assert!(matches!(err, CatalogError::MissingModel { .. }));
        assert!(Catalog::from_json_str(&serde_json::to_string(&catalog).unwrap()).is_err());
    }

    fn model_mut<'a>(catalog: &'a mut Catalog, id: &str) -> &'a mut ModelEntry {
        catalog.models.iter_mut().find(|m| m.id == id).unwrap()
    }

    #[test]
    fn wrong_role_error() {
        let mut catalog = parsed();
        model_mut(&mut catalog, "qwen35-2b-q4").role = "asr".to_string();
        let err = catalog.validate().unwrap_err();
        assert!(matches!(err, CatalogError::WrongRole { .. }));
    }

    #[test]
    fn bad_hash_error() {
        let mut catalog = parsed();
        model_mut(&mut catalog, "whisper-turbo-q5").assets[0].sha256 = "deadbeef".to_string();
        let err = catalog.validate().unwrap_err();
        assert!(matches!(err, CatalogError::BadHash { .. }));
    }
}
