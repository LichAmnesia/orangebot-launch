use std::path::PathBuf;

/// Shared data directory root for all app data (history, models, config).
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let home = std::env::var("APPDATA").ok();
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var("HOME").ok();
    data_dir_from(std::env::var("OB_DATA_DIR").ok(), home)
}

fn data_dir_from(env: Option<String>, home: Option<String>) -> PathBuf {
    if let Some(dir) = env.filter(|value| !value.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = home.filter(|value| !value.is_empty()).unwrap_or_default();
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Application Support/orangebot")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(home).join("orangebot")
    } else {
        PathBuf::from(home).join(".local/share/orangebot")
    }
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn history_db() -> PathBuf {
    data_dir().join("history.db")
}

pub fn config_file() -> PathBuf {
    data_dir().join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_wins() {
        assert_eq!(
            data_dir_from(Some("/x".to_string()), None),
            PathBuf::from("/x")
        );
    }

    #[test]
    fn empty_env_falls_back_to_home() {
        let dir = data_dir_from(Some(String::new()), Some("/h".to_string()));
        assert!(dir.starts_with("/h"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_home_layout() {
        assert_eq!(
            data_dir_from(None, Some("/h".to_string())),
            PathBuf::from("/h/Library/Application Support/orangebot")
        );
    }

    #[test]
    fn derived_paths_hang_off_data_dir() {
        assert!(models_dir().ends_with("models"));
        assert!(history_db().ends_with("history.db"));
        assert!(config_file().ends_with("config.toml"));
    }
}
