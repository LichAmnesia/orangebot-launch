const SERVICE: &str = "ai.orangebot.desktop";

fn entry(role: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("{role}-api-key")).map_err(|e| e.to_string())
}

pub fn set_key(role: &str, key: &str) -> Result<(), String> {
    entry(role)?.set_password(key).map_err(|e| e.to_string())
}

pub fn get_key(role: &str) -> Option<String> {
    entry(role).ok()?.get_password().ok()
}

#[allow(dead_code)]
pub fn delete_key(role: &str) {
    if let Ok(entry) = entry(role) {
        let _ = entry.delete_credential();
    }
}
