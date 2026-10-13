use leptos::prelude::*;
use ob_ipc::{DayCount, Stats, cmd};

use crate::i18n::pick;
use crate::ipc::call;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Args {
    tz_offset_minutes: i32,
}

fn saved_label(words_week: u64, minutes_week: f64) -> String {
    let saved = words_week as f64 / 40.0 - minutes_week;
    let mins = if saved < 0.0 { 0 } else { saved.floor() as u64 };
    if mins >= 60 {
        pick("{h} h {m} min", "{h} 小时 {m} 分")
            .replace("{h}", &(mins / 60).to_string())
            .replace("{m}", &(mins % 60).to_string())
    } else {
        pick("{mins} min", "{mins} 分钟").replace("{mins}", &mins.to_string())
    }
}

fn level(sessions: u32) -> &'static str {
    match sessions {
        0 => "lvl0",
        1 => "lvl1",
        2..=3 => "lvl2",
        4..=6 => "lvl3",
        _ => "lvl4",
    }
}

#[component]
pub fn StatsCard() -> impl IntoView {
    let stats = AsyncDerived::new_unsync(move || async move {
        let offset = js_sys::Date::new_0().get_timezone_offset() as i32;
        call::<_, Stats>(
            cmd::GET_STATS,
            &Args {
                tz_offset_minutes: offset,
            },
        )
        .await
    });

    view! {
        {move || match stats.get() {
            Some(Ok(s)) if s.words_total > 0 => {
                let start = s.days.len().saturating_sub(84);
                let cells: Vec<Option<DayCount>> = (0..84)
                    .map(|i| s.days.get(start + i).cloned())
                    .collect();
                let streak = s.streak_days;
                view! {
                    <div class="card stats">
                        <div class="fig">
                            <b>{s.words_week.to_string()}</b>
                            <span>{move || pick("Words this week", "本周字数")}</span>
                        </div>
                        <div class="fig">
                            <b>{format!("{:.1}", s.minutes_week)}</b>
                            <span>{move || pick("Minutes spoken", "说话分钟")}</span>
                        </div>
                        <div class="fig">
                            <b>{saved_label(s.words_week, s.minutes_week)}</b>
                            <span>{move || pick("Time saved", "节省时间")}</span>
                        </div>
                        <div>
                            <div class="heat">
                                {cells
                                    .into_iter()
                                    .map(|cell| match cell {
                                        Some(d) => {
                                            let day = d.clone();
                                            view! {
                                                <i
                                                    class=level(d.sessions)
                                                    title=move || {
                                                        pick(
                                                            "{date}: {n} sessions",
                                                            "{date}：{n} 次",
                                                        )
                                                        .replace("{date}", &day.date)
                                                        .replace("{n}", &day.sessions.to_string())
                                                    }
                                                >
                                                </i>
                                            }
                                                .into_any()
                                        }
                                        None => view! { <i class="lvl0"></i> }.into_any(),
                                    })
                                    .collect_view()}
                            </div>
                            <p class="muted">{move || {
                                let n = streak;
                                pick("Streak: {n} days", "连续 {n} 天")
                                    .replace("{n}", &n.to_string())
                            }}</p>
                        </div>
                        <p class="muted">
                            {move || pick(
                                "Time saved assumes typing at 40 words per minute.",
                                "节省时间按每分钟打字 40 词估算。",
                            )}
                        </p>
                    </div>
                }
                    .into_any()
            }
            Some(Ok(_)) => view! {
                <p class="muted">
                    {move || pick(
                        "Your stats appear after your first dictation.",
                        "首次听写后，这里会显示你的统计。",
                    )}
                </p>
            }
                .into_any(),
            _ => ().into_any(),
        }}
    }
}
