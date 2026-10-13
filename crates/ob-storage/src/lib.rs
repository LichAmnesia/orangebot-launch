//! SQLite storage: history and dictionary.

use std::path::Path;

use rusqlite::{Connection, params};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub id: String,
    pub created_at_ms: i64,
    pub mode: String,
    pub app: Option<String>,
    pub raw_text: String,
    pub final_text: String,
    pub delivery: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeetingRow {
    pub id: String,
    pub title: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub segment_count: i64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRow {
    pub seq: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictEntry {
    pub id: i64,
    pub phrase: String,
    pub aliases: Vec<String>,
    pub source: String,
    pub enabled: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayUsage {
    pub day_start_ms: i64,
    pub sessions: i64,
    pub words: i64,
    pub audio_ms: i64,
}

pub struct Store {
    conn: Connection,
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
}

pub fn count_words(text: &str) -> i64 {
    let mut words = 0i64;
    let mut latin = String::with_capacity(text.len());
    for c in text.chars() {
        if is_cjk(c) {
            words += 1;
            latin.push(' ');
        } else {
            latin.push(c);
        }
    }
    words + latin.split_whitespace().count() as i64
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StorageError> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            self.conn.execute_batch(
                "CREATE TABLE sessions(
                    id TEXT PRIMARY KEY,
                    created_at_ms INTEGER NOT NULL,
                    mode TEXT NOT NULL,
                    app TEXT,
                    raw_text TEXT NOT NULL,
                    final_text TEXT NOT NULL,
                    delivery TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL
                );
                CREATE INDEX sessions_created ON sessions(created_at_ms);
                CREATE TABLE dict_entries(
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    phrase TEXT NOT NULL UNIQUE,
                    aliases TEXT NOT NULL,
                    source TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );",
            )?;
            self.conn.execute_batch("PRAGMA user_version = 1")?;
        }
        if version <= 1 {
            self.conn.execute_batch(
                "CREATE TABLE meetings(
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    started_at_ms INTEGER NOT NULL,
                    ended_at_ms INTEGER
                );
                CREATE TABLE meeting_segments(
                    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                    seq INTEGER NOT NULL,
                    start_ms INTEGER NOT NULL,
                    end_ms INTEGER NOT NULL,
                    text TEXT NOT NULL,
                    PRIMARY KEY(meeting_id, seq)
                );",
            )?;
            self.conn.execute_batch("PRAGMA user_version = 2")?;
        }
        Ok(())
    }

    pub fn insert_session(&self, r: &SessionRecord) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO sessions(id, created_at_ms, mode, app, raw_text, final_text, delivery, duration_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                r.id,
                r.created_at_ms,
                r.mode,
                r.app,
                r.raw_text,
                r.final_text,
                r.delivery,
                r.duration_ms
            ],
        )?;
        Ok(())
    }

    pub fn list_sessions(
        &self,
        limit: u32,
        query: Option<&str>,
    ) -> Result<Vec<SessionRecord>, StorageError> {
        let (sql, pattern) = match query {
            Some(q) => (
                "SELECT id, created_at_ms, mode, app, raw_text, final_text, delivery, duration_ms
                 FROM sessions
                 WHERE final_text LIKE ?1 OR raw_text LIKE ?1
                 ORDER BY created_at_ms DESC LIMIT ?2"
                    .to_string(),
                Some(format!("%{q}%")),
            ),
            None => (
                "SELECT id, created_at_ms, mode, app, raw_text, final_text, delivery, duration_ms
                 FROM sessions
                 ORDER BY created_at_ms DESC LIMIT ?1"
                    .to_string(),
                None,
            ),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let map = |row: &rusqlite::Row<'_>| -> rusqlite::Result<SessionRecord> {
            Ok(SessionRecord {
                id: row.get(0)?,
                created_at_ms: row.get(1)?,
                mode: row.get(2)?,
                app: row.get(3)?,
                raw_text: row.get(4)?,
                final_text: row.get(5)?,
                delivery: row.get(6)?,
                duration_ms: row.get(7)?,
            })
        };
        let rows = match &pattern {
            Some(p) => stmt.query_map(params![p, limit], map)?,
            None => stmt.query_map(params![limit], map)?,
        };
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_session(&self, id: &str) -> Result<bool, StorageError> {
        let n = self
            .conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    pub fn purge_sessions_before(&self, cutoff_ms: i64) -> Result<usize, StorageError> {
        let n = self.conn.execute(
            "DELETE FROM sessions WHERE created_at_ms < ?1",
            params![cutoff_ms],
        )?;
        Ok(n)
    }

    pub fn purge_meetings_before(&self, cutoff_ms: i64) -> Result<usize, StorageError> {
        let n = self.conn.execute(
            "DELETE FROM meetings WHERE started_at_ms < ?1",
            params![cutoff_ms],
        )?;
        Ok(n)
    }

    pub fn usage_by_day(
        &self,
        from_ms: i64,
        tz_offset_ms: i64,
    ) -> Result<Vec<DayUsage>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT created_at_ms, final_text, duration_ms
             FROM sessions WHERE created_at_ms >= ?1 ORDER BY created_at_ms",
        )?;
        let rows = stmt.query_map(params![from_ms], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut days: std::collections::BTreeMap<i64, DayUsage> = std::collections::BTreeMap::new();
        for row in rows {
            let (created_at_ms, final_text, duration_ms) = row?;
            let day_start_ms =
                ((created_at_ms + tz_offset_ms) / 86_400_000) * 86_400_000 - tz_offset_ms;
            let day = days.entry(day_start_ms).or_insert(DayUsage {
                day_start_ms,
                sessions: 0,
                words: 0,
                audio_ms: 0,
            });
            day.sessions += 1;
            day.words += count_words(&final_text);
            day.audio_ms += duration_ms;
        }
        Ok(days.into_values().collect())
    }

    pub fn total_words(&self) -> Result<i64, StorageError> {
        let mut stmt = self.conn.prepare("SELECT final_text FROM sessions")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut total = 0i64;
        for row in rows {
            total += count_words(&row?);
        }
        Ok(total)
    }

    pub fn add_dict_entry(
        &self,
        phrase: &str,
        aliases: &[String],
        source: &str,
        now_ms: i64,
    ) -> Result<i64, StorageError> {
        let aliases_json = serde_json::to_string(aliases)?;
        self.conn.execute(
            "INSERT INTO dict_entries(phrase, aliases, source, enabled, created_at_ms)
             VALUES(?1, ?2, ?3, 1, ?4)",
            params![phrase, aliases_json, source, now_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_dict(&self, only_enabled: bool) -> Result<Vec<DictEntry>, StorageError> {
        let sql = if only_enabled {
            "SELECT id, phrase, aliases, source, enabled, created_at_ms
             FROM dict_entries WHERE enabled = 1 ORDER BY phrase"
        } else {
            "SELECT id, phrase, aliases, source, enabled, created_at_ms
             FROM dict_entries ORDER BY phrase"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, phrase, aliases_json, source, enabled, created_at_ms) = row?;
            out.push(DictEntry {
                id,
                phrase,
                aliases: serde_json::from_str(&aliases_json)?,
                source,
                enabled: enabled != 0,
                created_at_ms,
            });
        }
        Ok(out)
    }

    pub fn set_dict_enabled(&self, id: i64, enabled: bool) -> Result<bool, StorageError> {
        let n = self.conn.execute(
            "UPDATE dict_entries SET enabled = ?1 WHERE id = ?2",
            params![enabled as i64, id],
        )?;
        Ok(n > 0)
    }

    pub fn delete_dict_entry(&self, id: i64) -> Result<bool, StorageError> {
        let n = self
            .conn
            .execute("DELETE FROM dict_entries WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    pub fn create_meeting(
        &self,
        id: &str,
        title: &str,
        started_at_ms: i64,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO meetings(id, title, started_at_ms) VALUES(?1, ?2, ?3)",
            params![id, title, started_at_ms],
        )?;
        Ok(())
    }

    pub fn append_segment(
        &self,
        meeting_id: &str,
        seq: i64,
        start_ms: i64,
        end_ms: i64,
        text: &str,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO meeting_segments(meeting_id, seq, start_ms, end_ms, text)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![meeting_id, seq, start_ms, end_ms, text],
        )?;
        Ok(())
    }

    pub fn finish_meeting(&self, id: &str, ended_at_ms: i64) -> Result<bool, StorageError> {
        let n = self.conn.execute(
            "UPDATE meetings SET ended_at_ms = ?2 WHERE id = ?1",
            params![id, ended_at_ms],
        )?;
        Ok(n > 0)
    }

    pub fn list_meetings(&self, limit: u32) -> Result<Vec<MeetingRow>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, started_at_ms, ended_at_ms,
                    (SELECT COUNT(*) FROM meeting_segments s WHERE s.meeting_id = m.id)
             FROM meetings m
             ORDER BY started_at_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(MeetingRow {
                id: row.get(0)?,
                title: row.get(1)?,
                started_at_ms: row.get(2)?,
                ended_at_ms: row.get(3)?,
                segment_count: row.get(4)?,
                preview: String::new(),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            let mut meeting = row?;
            meeting.preview = self.meeting_preview(&meeting.id)?;
            out.push(meeting);
        }
        Ok(out)
    }

    fn meeting_preview(&self, meeting_id: &str) -> Result<String, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT text FROM meeting_segments WHERE meeting_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map(params![meeting_id], |row| row.get::<_, String>(0))?;
        let mut parts = Vec::new();
        for row in rows {
            parts.push(row?);
        }
        let joined = parts.join(" ");
        Ok(joined.chars().take(120).collect())
    }

    pub fn meeting_segments(&self, id: &str) -> Result<Vec<SegmentRow>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, start_ms, end_ms, text
             FROM meeting_segments WHERE meeting_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![id], |row| {
            Ok(SegmentRow {
                seq: row.get(0)?,
                start_ms: row.get(1)?,
                end_ms: row.get(2)?,
                text: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_meeting(&self, id: &str) -> Result<bool, StorageError> {
        let n = self
            .conn
            .execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, created_at_ms: i64, final_text: &str) -> SessionRecord {
        SessionRecord {
            id: id.to_string(),
            created_at_ms,
            mode: "type".to_string(),
            app: Some("Mail".to_string()),
            raw_text: format!("raw {id}"),
            final_text: final_text.to_string(),
            delivery: "clipboard".to_string(),
            duration_ms: 42,
        }
    }

    #[test]
    fn migrate_twice_is_noop() {
        let store = Store::open_in_memory().unwrap();
        store.migrate().unwrap();
        store.migrate().unwrap();
    }

    #[test]
    fn session_roundtrip_order_limit_query() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&session("s1", 100, "hello alpha"))
            .unwrap();
        store
            .insert_session(&session("s2", 300, "beta world"))
            .unwrap();
        store
            .insert_session(&session("s3", 200, "gamma alpha"))
            .unwrap();

        let all = store.list_sessions(10, None).unwrap();
        assert_eq!(
            all.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["s2", "s3", "s1"]
        );
        assert_eq!(all[0], session("s2", 300, "beta world"));

        let limited = store.list_sessions(2, None).unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].id, "s2");
        assert_eq!(limited[1].id, "s3");

        let matched = store.list_sessions(10, Some("beta")).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "s2");
    }

    #[test]
    fn delete_and_purge_sessions() {
        let store = Store::open_in_memory().unwrap();
        store.insert_session(&session("s1", 100, "a")).unwrap();
        store.insert_session(&session("s2", 300, "b")).unwrap();
        assert!(store.delete_session("s1").unwrap());
        assert!(!store.delete_session("s1").unwrap());
        assert!(!store.delete_session("nope").unwrap());

        store.insert_session(&session("s3", 200, "c")).unwrap();
        let purged = store.purge_sessions_before(250).unwrap();
        assert_eq!(purged, 1);
        let rest = store.list_sessions(10, None).unwrap();
        assert_eq!(
            rest.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["s2"]
        );
    }

    #[test]
    fn dict_roundtrip_and_errors() {
        let store = Store::open_in_memory().unwrap();
        let aliases = vec!["皮蛋".to_string(), "pidan".to_string()];
        let id = store
            .add_dict_entry("皮蛋", &aliases, "manual", 1000)
            .unwrap();
        assert!(id > 0);
        store
            .add_dict_entry("豆腐", &[], "heuristic", 1001)
            .unwrap();

        let list = store.list_dict(false).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].phrase, "皮蛋");
        assert_eq!(list[1].phrase, "豆腐");
        assert_eq!(list[0].aliases, aliases);
        assert_eq!(list[0].source, "manual");
        assert!(list[0].enabled);

        let dup = store.add_dict_entry("皮蛋", &[], "manual", 1002);
        assert!(dup.is_err());
    }

    #[test]
    fn dict_enable_toggle_filters_list() {
        let store = Store::open_in_memory().unwrap();
        let id = store.add_dict_entry("皮蛋", &[], "manual", 1000).unwrap();
        store.add_dict_entry("豆腐", &[], "manual", 1001).unwrap();

        assert!(store.set_dict_enabled(id, false).unwrap());
        let enabled = store.list_dict(true).unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].phrase, "豆腐");

        assert!(store.set_dict_enabled(id, true).unwrap());
        assert_eq!(store.list_dict(true).unwrap().len(), 2);
        assert!(!store.set_dict_enabled(9999, false).unwrap());
    }

    #[test]
    fn dict_delete() {
        let store = Store::open_in_memory().unwrap();
        let id = store.add_dict_entry("皮蛋", &[], "manual", 1000).unwrap();
        assert!(store.delete_dict_entry(id).unwrap());
        assert!(!store.delete_dict_entry(id).unwrap());
        assert!(store.list_dict(false).unwrap().is_empty());
    }

    #[test]
    fn fresh_store_is_version_two() {
        let store = Store::open_in_memory().unwrap();
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        store.migrate().unwrap();
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn meeting_lifecycle() {
        let store = Store::open_in_memory().unwrap();
        store.create_meeting("m1", "Standup", 1000).unwrap();
        store
            .append_segment("m1", 0, 0, 500, "first thing discussed")
            .unwrap();
        store
            .append_segment("m1", 1, 500, 900, "second thing")
            .unwrap();
        store
            .append_segment("m1", 2, 900, 1500, "third thing")
            .unwrap();
        assert!(store.finish_meeting("m1", 1500).unwrap());
        assert!(!store.finish_meeting("nope", 1500).unwrap());

        let list = store.list_meetings(10).unwrap();
        assert_eq!(list.len(), 1);
        let m = &list[0];
        assert_eq!(m.id, "m1");
        assert_eq!(m.segment_count, 3);
        assert_eq!(m.ended_at_ms, Some(1500));
        assert!(m.preview.starts_with("first thing discussed"));

        let segs = store.meeting_segments("m1").unwrap();
        assert_eq!(
            segs.iter().map(|s| s.seq).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(segs[1].text, "second thing");

        assert!(store.delete_meeting("m1").unwrap());
        assert!(!store.delete_meeting("m1").unwrap());
        assert!(store.meeting_segments("m1").unwrap().is_empty());
    }

    #[test]
    fn meeting_list_order_newest_first() {
        let store = Store::open_in_memory().unwrap();
        store.create_meeting("old", "Old", 100).unwrap();
        store.create_meeting("new", "New", 300).unwrap();
        store.create_meeting("mid", "Mid", 200).unwrap();
        let list = store.list_meetings(2).unwrap();
        assert_eq!(
            list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["new", "mid"]
        );
    }

    #[test]
    fn count_words_mixed_scripts() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("你好世界"), 4);
        assert_eq!(count_words("用 Rust 写"), 3);
    }

    #[test]
    fn usage_by_day_groups_same_and_next_day() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&SessionRecord {
                duration_ms: 100,
                ..session("s1", 0, "hello world")
            })
            .unwrap();
        store
            .insert_session(&SessionRecord {
                duration_ms: 200,
                ..session("s2", 86_400_000 - 1, "你好世界")
            })
            .unwrap();
        store
            .insert_session(&SessionRecord {
                duration_ms: 50,
                ..session("s3", 86_400_000, "用 Rust 写")
            })
            .unwrap();
        store
            .insert_session(&SessionRecord {
                duration_ms: 999,
                ..session("s0", -100, "ignored")
            })
            .unwrap();

        let days = store.usage_by_day(0, 0).unwrap();
        assert_eq!(
            days,
            vec![
                DayUsage {
                    day_start_ms: 0,
                    sessions: 2,
                    words: 6,
                    audio_ms: 300,
                },
                DayUsage {
                    day_start_ms: 86_400_000,
                    sessions: 1,
                    words: 3,
                    audio_ms: 50,
                },
            ]
        );
    }

    #[test]
    fn total_words_sums_over_all_sessions() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_session(&session("s1", 100, "hello world"))
            .unwrap();
        store
            .insert_session(&session("s2", 200, "你好世界"))
            .unwrap();
        store
            .insert_session(&session("s3", 300, "用 Rust 写"))
            .unwrap();
        assert_eq!(store.total_words().unwrap(), 9);
        let empty = Store::open_in_memory().unwrap();
        assert_eq!(empty.total_words().unwrap(), 0);
    }
}
