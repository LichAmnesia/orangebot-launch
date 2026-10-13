//! `ob history` — past dictations.

pub fn open_store() -> anyhow::Result<ob_storage::Store> {
    let dir = crate::data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(ob_storage::Store::open(&dir.join("history.db"))?)
}

pub fn show(limit: u32, query: Option<&str>) -> anyhow::Result<()> {
    let store = open_store()?;
    let sessions = store.list_sessions(limit, query)?;
    if sessions.is_empty() {
        println!("(no history yet)");
        return Ok(());
    }
    for session in sessions {
        let created = session.created_at_ms / 1000;
        let flat = session.final_text.replace('\n', " ");
        let text = if flat.chars().count() > 80 {
            let cut: String = flat.chars().take(80).collect();
            format!("{cut}…")
        } else {
            flat
        };
        println!(
            "{created} [{}] {:<11} {text}",
            session.mode, session.delivery
        );
    }
    Ok(())
}
