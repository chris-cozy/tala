//! Aggregates recorded study days and current collection state. Undone reviews
//! are excluded; observed recall and model-estimated retrievability remain separate metrics.

use crate::{
    error::{AppError, Result},
    models::*,
    store::Store,
};
use chrono::{Duration, NaiveDate};
use rusqlite::params;
use std::collections::{BTreeMap, HashSet};

impl Store {
    pub fn streaks(&self) -> Result<(u32, u32)> {
        let days = self
            .conn
            .prepare("SELECT DISTINCT day FROM reviews WHERE undone_at IS NULL ORDER BY day")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let dates: Vec<_> = days
            .iter()
            .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .collect();
        let mut previous = None;
        let mut run = 0;
        let mut longest = 0;
        for date in &dates {
            run = if previous.is_some_and(|p: NaiveDate| *date - p == Duration::days(1)) {
                run + 1
            } else {
                1
            };
            longest = longest.max(run);
            previous = Some(*date);
        }
        let today = self.now().date_at(self.now().now);
        let active: HashSet<_> = dates.into_iter().collect();
        let mut date = if active.contains(&today) {
            today
        } else {
            today - Duration::days(1)
        };
        let mut current = 0;
        while active.contains(&date) {
            current += 1;
            date -= Duration::days(1);
        }
        Ok((current, longest))
    }
    fn period(&self, start: &str, end: &str) -> Result<PeriodSummary> {
        let (reviewed, seconds, correct): (u32, f64, u32) = self.conn.query_row(
            "SELECT count(*),COALESCE(sum(duration_ms)/1000.0,0),COALESCE(sum(grade>1),0) FROM \
                reviews WHERE day>=?1 AND day<?2 AND undone_at IS NULL",
            params![start, end],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let learned = self.conn.query_row(
            "SELECT count(*) FROM (SELECT min(rowid) AS first_id FROM reviews WHERE undone_at IS NULL \
                AND json_extract(after_state,'$.phase')='review' GROUP BY card_id) first JOIN reviews r \
                ON r.rowid=first.first_id WHERE r.day>=?1 AND r.day<?2",
            params![start, end],
            |r| r.get(0),
        )?;
        Ok(PeriodSummary {
            reviewed,
            seconds,
            recall: if reviewed > 0 {
                correct as f64 / reviewed as f64
            } else {
                0.0
            },
            learned,
        })
    }
    pub fn statistics(&self, range: &str) -> Result<Statistics> {
        let clock = self.now();
        let today = clock.date_at(clock.now);
        let end = today + Duration::days(1);
        let first: Option<String> = self.conn.query_row(
            "SELECT min(day) FROM reviews WHERE undone_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        let days = match range {
            "7" => 7,
            "30" => 30,
            "90" => 90,
            "365" => 365,
            "all" => first
                .as_deref()
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .map(|d| (end - d).num_days())
                .unwrap_or(7)
                .max(1),
            _ => return Err(AppError::invalid("Choose a supported statistics range.")),
        };
        let start = end - Duration::days(days);
        let previous_start = start - Duration::days(days);
        let summary = self.period(&start.to_string(), &end.to_string())?;
        let previous = self.period(&previous_start.to_string(), &start.to_string())?;
        let rows = self
            .conn
            .prepare("SELECT day,count(*) FROM reviews WHERE day>=?1 AND day<?2 AND undone_at IS NULL GROUP BY day")?
            .query_map(params![start.to_string(), end.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        // Daily points are combined by month for very long collections, keeping charts bounded.
        let mut timeline = BTreeMap::<String, u32>::new();
        for offset in 0..days {
            let date = start + Duration::days(offset);
            let key = if days > 730 {
                date.format("%Y-%m-01").to_string()
            } else {
                date.to_string()
            };
            *timeline.entry(key).or_default() += rows.get(&date.to_string()).copied().unwrap_or(0);
        }
        let days = timeline
            .into_iter()
            .map(|(day, count)| DayStat { day, count })
            .collect();
        let grade_counts = self
            .conn
            .prepare("SELECT grade,count(*) FROM reviews WHERE day>=?1 AND day<?2 AND undone_at IS NULL GROUP BY grade")?
            .query_map(params![start.to_string(), end.to_string()], |r| Ok((r.get::<_, usize>(0)?, r.get::<_, u32>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        let grades = ["Again", "Hard", "Good", "Easy"]
            .iter()
            .enumerate()
            .map(|(i, name)| StateStat {
                name: name.to_string(),
                count: grade_counts.get(&(i + 1)).copied().unwrap_or(0),
            })
            .collect();
        let phase_counts = self
            .conn
            .prepare(
                "SELECT CASE WHEN suspended=1 THEN 'Suspended' WHEN buried_until>?1 THEN 'Buried' ELSE \
                    phase END,count(*) FROM cards WHERE deleted_at IS NULL GROUP BY 1",
            )?
            .query_map([clock.now], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        let states = [
            ("new", "New"),
            ("learning", "Learning"),
            ("review", "Review"),
            ("relearning", "Relearning"),
            ("Suspended", "Suspended"),
            ("Buried", "Buried"),
        ]
        .iter()
        .map(|(key, name)| StateStat {
            name: name.to_string(),
            count: phase_counts.get(*key).copied().unwrap_or(0),
        })
        .collect();
        let due_dates = self
            .conn
            .prepare(
                "SELECT max(due,COALESCE(buried_until,due)) FROM cards WHERE deleted_at IS NULL AND \
                    suspended=0 AND phase!='new' AND max(due,COALESCE(buried_until,due))<?2",
            )?
            .query_map(params![clock.now, clock.after_days(14)], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut forecast_counts = BTreeMap::<String, u32>::new();
        for due in due_dates {
            *forecast_counts
                .entry(clock.date_at(due).max(today).to_string())
                .or_default() += 1;
        }
        let forecast = (0..14)
            .map(|d| {
                let day = (today + Duration::days(d)).to_string();
                DayStat {
                    count: forecast_counts.get(&day).copied().unwrap_or(0),
                    day,
                }
            })
            .collect();
        let memories = self
            .conn
            .prepare("SELECT schedule FROM cards WHERE phase='review' AND deleted_at IS NULL AND suspended=0")?
            .query_map([], |r| crate::store::json_column::<Schedule>(r, 0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut retention = 0.0;
        let mut stability = 0.0;
        let mut difficulty = 0.0;
        let mut count = 0.0;
        for s in memories {
            if let (Some(st), Some(di), Some(last)) = (s.stability, s.difficulty, s.last_review) {
                let value = fsrs::current_retrievability(
                    fsrs::MemoryState {
                        stability: st,
                        difficulty: di,
                    },
                    clock.elapsed_days(last) as f32,
                    fsrs::FSRS6_DEFAULT_DECAY,
                );
                if value.is_finite() {
                    retention += value as f64;
                    stability += st as f64;
                    difficulty += di as f64;
                    count += 1.0;
                }
            }
        }
        let (current_streak, longest_streak) = self.streaks()?;
        Ok(Statistics {
            summary,
            previous,
            current_streak,
            longest_streak,
            days,
            grades,
            states,
            decks: self.decks()?,
            forecast,
            estimated_retention: if count > 0.0 {
                Some(retention / count)
            } else {
                None
            },
            average_stability: if count > 0.0 {
                Some(stability / count)
            } else {
                None
            },
            average_difficulty: if count > 0.0 {
                Some(difficulty / count)
            } else {
                None
            },
        })
    }
}
