//! Translate target list commands.

use crate::settings;

fn sanitize(targets: Vec<String>) -> Result<Vec<String>, String> {
    let mut cleaned: Vec<String> = Vec::new();
    for target in targets {
        let target = target.trim().to_string();
        if target.is_empty() || cleaned.contains(&target) {
            continue;
        }
        cleaned.push(target);
    }
    if cleaned.is_empty() {
        return Err("Pick at least one language".to_string());
    }
    Ok(cleaned)
}

#[tauri::command]
pub fn get_translate_targets() -> Result<Vec<String>, String> {
    Ok(settings::load().translate_targets)
}

#[tauri::command]
pub fn set_translate_targets(targets: Vec<String>) -> Result<(), String> {
    let targets = sanitize(targets)?;
    let mut current = settings::load();
    current.translate_target = targets[0].clone();
    current.translate_targets = targets;
    settings::save(&current).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn sanitize_trims_dedupes_and_rejects_empty() {
        let out = sanitize(vec![
            " English ".to_string(),
            "".to_string(),
            "  ".to_string(),
            "English".to_string(),
            "Japanese".to_string(),
        ])
        .unwrap();
        assert_eq!(out, vec!["English".to_string(), "Japanese".to_string()]);
        assert!(sanitize(vec![" ".to_string(), "".to_string()]).is_err());
        assert!(sanitize(Vec::new()).is_err());
    }
}
