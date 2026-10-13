//! Usage statistics commands.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ob_ipc::{DayCount, Stats};
use ob_storage::{DayUsage, Store};

const DAY_MS: i64 = 86_400_000;

fn store() -> Result<Store, String> {
    let dir = ob_domain::paths::data_dir();
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Store::open(&ob_domain::paths::history_db()).map_err(|err| err.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn date_of_ms(ms: i64) -> String {
    // Howard Hinnant's civil-from-days, no chrono.
    let days = ms.div_euclid(DAY_MS);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[tauri::command]
pub fn get_stats(tz_offset_minutes: i32) -> Result<Stats, String> {
    // JS getTimezoneOffset: minutes to add to local time to get UTC (UTC − local).
    let tz = -(tz_offset_minutes as i64) * 60_000;
    let now = now_ms();
    let store = store()?;
    let usage = store
        .usage_by_day(now - 84 * DAY_MS, tz)
        .map_err(|err| err.to_string())?;
    let words_total = store.total_words().map_err(|err| err.to_string())?;

    let by_day: BTreeMap<i64, DayUsage> = usage.into_iter().map(|d| (d.day_start_ms, d)).collect();
    let today_start = (now + tz).div_euclid(DAY_MS) * DAY_MS - tz;

    let mut days = Vec::with_capacity(84);
    for i in 0..84i64 {
        let day_start = today_start - (83 - i) * DAY_MS;
        let used = by_day.get(&day_start);
        days.push(DayCount {
            date: date_of_ms(day_start + tz),
            sessions: used.map(|d| d.sessions.max(0) as u32).unwrap_or(0),
            words: used.map(|d| d.words.max(0) as u64).unwrap_or(0),
        });
    }

    let week = &days[days.len() - 7..];
    let words_week = week.iter().map(|d| d.words).sum();
    let sessions_week = week.iter().map(|d| d.sessions).sum();
    let week_start = today_start - 6 * DAY_MS;
    let minutes_week = by_day
        .range(week_start..)
        .map(|(_, d)| d.audio_ms.max(0) as f64)
        .sum::<f64>()
        / 60_000.0;

    let mut streak_days = 0u32;
    let mut day = today_start;
    if by_day.get(&day).map(|d| d.sessions <= 0).unwrap_or(true) {
        day -= DAY_MS;
    }
    while let Some(d) = by_day.get(&day) {
        if d.sessions > 0 {
            streak_days += 1;
            day -= DAY_MS;
        } else {
            break;
        }
    }

    Ok(Stats {
        words_week,
        minutes_week,
        sessions_week,
        words_total: words_total.max(0) as u64,
        streak_days,
        days,
    })
}

#[cfg(test)]
mod tests {
    use super::date_of_ms;

    #[test]
    fn date_of_ms_converts_epoch_days() {
        assert_eq!(date_of_ms(0), "1970-01-01");
        assert_eq!(date_of_ms(1_760_000_000_000), "2025-10-09");
    }
}
