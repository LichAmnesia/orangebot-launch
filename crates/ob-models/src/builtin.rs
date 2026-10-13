use std::path::{Path, PathBuf};

use crate::catalog::{Catalog, ModelEntry};

pub const BUILTIN_CATALOG_JSON: &str =
    include_str!("../../../docs/spec/model-catalog.candidate.json");

pub fn builtin_catalog() -> Catalog {
    Catalog::from_json_str(BUILTIN_CATALOG_JSON).expect("built-in catalog is valid")
}

pub fn model_dir(models_root: &Path, model_id: &str) -> PathBuf {
    models_root.join(model_id)
}

pub fn model_installed(models_root: &Path, model: &ModelEntry) -> bool {
    let dir = model_dir(models_root, &model.id);
    model
        .assets
        .iter()
        .chain(model.runtime_assets.iter())
        .all(|asset| {
            std::fs::metadata(dir.join(&asset.relative_path))
                .is_ok_and(|meta| meta.len() == asset.bytes)
        })
}

pub fn model_total_bytes(model: &ModelEntry) -> u64 {
    model
        .assets
        .iter()
        .chain(model.runtime_assets.iter())
        .map(|asset| asset.bytes)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Asset;

    #[test]
    fn builtin_catalog_has_five_models() {
        assert_eq!(builtin_catalog().models.len(), 5);
    }

    #[test]
    fn not_installed_on_empty_dir() {
        let root = tempfile::tempdir().unwrap();
        let model = &builtin_catalog().models[0];
        assert!(!model_installed(root.path(), model));
    }

    #[test]
    fn installed_after_asset_written_with_right_size() {
        let root = tempfile::tempdir().unwrap();
        let mut model = builtin_catalog().models[0].clone();
        model.assets = vec![Asset {
            relative_path: "small.bin".to_string(),
            url: "https://example.invalid/small.bin".to_string(),
            bytes: 3,
            sha256: "0".repeat(64),
        }];
        model.runtime_assets = Vec::new();

        assert!(!model_installed(root.path(), &model));
        std::fs::create_dir_all(model_dir(root.path(), &model.id)).unwrap();
        std::fs::write(
            model_dir(root.path(), &model.id).join("small.bin"),
            [1, 2, 3],
        )
        .unwrap();
        assert!(model_installed(root.path(), &model));
    }

    #[test]
    fn total_bytes_sums_assets() {
        let mut model = builtin_catalog().models[0].clone();
        model.assets = vec![Asset {
            relative_path: "a".to_string(),
            url: String::new(),
            bytes: 3,
            sha256: String::new(),
        }];
        model.runtime_assets = vec![Asset {
            relative_path: "b".to_string(),
            url: String::new(),
            bytes: 4,
            sha256: String::new(),
        }];
        assert_eq!(model_total_bytes(&model), 7);
    }
}
