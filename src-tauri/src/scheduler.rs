//! Pure FSRS transitions plus explicit learning/relearning steps. Preview and
//! review commits call the same transition function so displayed intervals match saved ones.

use crate::{
    clock::Clock,
    error::{AppError, Result},
    models::{DeckSettings, GradeOption, Phase, Schedule},
};
use fsrs::{FSRS, MemoryState};

pub const SCHEDULER_VERSION: &str = "fsrs-6/crate-6.6.1";
pub fn valid_schedule(s: &Schedule) -> bool {
    (0..=32_503_680_000).contains(&s.due)
        && s.last_review
            .is_none_or(|timestamp| (0..=32_503_680_000).contains(&timestamp))
        && s.scheduled_days <= 36_500
        && s.step <= 20
        && s.review_count < u32::MAX
        && s.lapses <= s.review_count
        && if s.phase == Phase::New {
            s.stability.is_none() && s.difficulty.is_none() && s.last_review.is_none()
        } else {
            s.last_review.is_some()
                && s.stability
                    .is_some_and(|value| value.is_finite() && value > 0.0)
                && s.difficulty
                    .is_some_and(|value| value.is_finite() && (1.0..=10.0).contains(&value))
        }
}
pub fn validate_settings(s: &DeckSettings) -> Result<()> {
    if !s.retention.is_finite() || !(0.7..=0.99).contains(&s.retention) {
        return Err(AppError::invalid(
            "Desired retention must be between 70% and 99%.",
        ));
    }
    if s.maximum_interval == 0
        || s.maximum_interval > 36_500
        || s.new_limit > 100_000
        || s.review_limit > 100_000
    {
        return Err(AppError::invalid(
            "Use an interval of 1–36,500 days and daily limits of 0–100,000.",
        ));
    }
    for steps in [&s.learning_steps, &s.relearning_steps] {
        if steps.len() > 20
            || steps.iter().any(|&x| x == 0 || x > 604_800)
            || steps.windows(2).any(|x| x[0] >= x[1])
        {
            return Err(AppError::invalid(
                "Learning steps must be increasing durations, between 1 second and 7 days (up to 20 steps).",
            ));
        }
    }
    if !["created", "random"].contains(&s.new_order.as_str())
        || !["due", "random"].contains(&s.review_order.as_str())
        || !["before", "after", "mix"].contains(&s.new_placement.as_str())
        || s.leech_threshold == 0
    {
        return Err(AppError::invalid(
            "Choose valid study ordering and a positive leech threshold.",
        ));
    }
    Ok(())
}

pub fn next_schedule(
    previous: &Schedule,
    settings: &DeckSettings,
    grade: u8,
    clock: Clock,
) -> Result<Schedule> {
    validate_settings(settings)?;
    if !valid_schedule(previous) {
        return Err(AppError::invalid(
            "This card has invalid scheduling data. Run an integrity check before continuing.",
        ));
    }
    if !(1..=4).contains(&grade) {
        return Err(AppError::invalid("Choose Again, Hard, Good, or Easy."));
    }
    if previous.last_review.is_some_and(|last| last > clock.now) {
        return Err(AppError::conflict(
            "The system clock is earlier than this card’s last review. Correct the clock before grading this card.",
        ));
    }
    let memory = match (previous.stability, previous.difficulty) {
        (Some(stability), Some(difficulty))
            if stability.is_finite()
                && difficulty.is_finite()
                && stability > 0.0
                && (1.0..=10.0).contains(&difficulty) =>
        {
            Some(MemoryState {
                stability,
                difficulty,
            })
        }
        (None, None) if previous.phase == Phase::New => None,
        _ => {
            return Err(AppError::invalid(
                "This card has an invalid memory state. Run an integrity check before continuing.",
            ));
        }
    };
    let fsrs = FSRS::default();
    let states = fsrs
        .next_states(
            memory,
            settings.retention,
            previous
                .last_review
                .map(|last| clock.elapsed_days(last))
                .unwrap_or(0),
        )
        .map_err(|_| {
            AppError::invalid("FSRS could not calculate a valid interval for this card.")
        })?;
    let chosen = match grade {
        1 => states.again,
        2 => states.hard,
        3 => states.good,
        _ => states.easy,
    };
    let days = (chosen.interval.round().max(1.0) as u32).min(settings.maximum_interval);
    let mut next = previous.clone();
    next.stability = Some(chosen.memory.stability);
    next.difficulty = Some(chosen.memory.difficulty);
    next.last_review = Some(clock.now);
    next.review_count += 1;
    next.scheduled_days = days;
    let steps = if matches!(previous.phase, Phase::Review | Phase::Relearning) {
        &settings.relearning_steps
    } else {
        &settings.learning_steps
    };
    let mut delay = None;
    match previous.phase {
        Phase::New | Phase::Learning | Phase::Relearning => {
            next.phase = if previous.phase == Phase::Relearning {
                Phase::Relearning
            } else {
                Phase::Learning
            };
            if grade == 4 || steps.is_empty() {
                next.phase = Phase::Review;
            } else if grade == 1 {
                next.step = 0;
                delay = Some(steps[0]);
            } else if grade == 2 {
                let index = (previous.step as usize).min(steps.len() - 1);
                next.step = index as u32;
                delay = Some(if index == 0 {
                    if steps.len() > 1 {
                        (steps[0] + steps[1]) / 2
                    } else {
                        steps[0].saturating_mul(3) / 2
                    }
                } else {
                    steps[index]
                });
            } else {
                let index = previous.step as usize + 1;
                if index >= steps.len() {
                    next.phase = Phase::Review;
                } else {
                    next.step = index as u32;
                    delay = Some(steps[index]);
                }
            }
        }
        Phase::Review => {
            if grade == 1 {
                next.lapses += 1;
                next.step = 0;
                if let Some(step) = steps.first() {
                    next.phase = Phase::Relearning;
                    delay = Some(*step);
                }
            }
        }
    }
    next.due = match delay {
        Some(seconds) => clock.now + seconds as i64,
        None => {
            next.phase = Phase::Review;
            next.step = 0;
            clock.after_days(days)
        }
    };
    Ok(next)
}

pub fn preview(
    schedule: &Schedule,
    settings: &DeckSettings,
    clock: Clock,
) -> Result<Vec<GradeOption>> {
    (1..=4)
        .map(|grade| {
            let next = next_schedule(schedule, settings, grade, clock)?;
            let seconds = (next.due - clock.now).max(1);
            let interval = if next.phase == Phase::Review {
                format!("{} d", next.scheduled_days)
            } else if seconds < 60 {
                format!("{seconds} sec")
            } else if seconds < 3600 {
                format!("{} min", (seconds as f64 / 60.0).ceil() as i64)
            } else if seconds < 86400 {
                format!("{} h", (seconds as f64 / 3600.0).ceil() as i64)
            } else {
                format!("{} d", (seconds as f64 / 86400.0).ceil() as i64)
            };
            Ok(GradeOption {
                grade,
                label: ["Again", "Hard", "Good", "Easy"][grade as usize - 1].into(),
                interval,
                schedule: next,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock() -> Clock {
        Clock::at(1_780_000_000, chrono_tz::UTC)
    }
    #[test]
    fn all_grades_use_fsrs_memory_and_learning_steps() {
        let s = Schedule::new(clock().now);
        let settings = DeckSettings::default();
        let p = preview(&s, &settings, clock()).unwrap();
        assert_eq!(p[0].schedule.due, clock().now + 60);
        assert_eq!(p[1].schedule.due, clock().now + 330);
        assert_eq!(p[2].schedule.due, clock().now + 600);
        assert_eq!(p[3].schedule.phase, Phase::Review);
        assert!(p.iter().all(|x| x.schedule.stability.unwrap() > 0.0));
        let graduated = next_schedule(
            &p[2].schedule,
            &settings,
            3,
            Clock::at(clock().now + 600, clock().zone),
        )
        .unwrap();
        assert_eq!(graduated.phase, Phase::Review);
        let lapse = next_schedule(
            &graduated,
            &settings,
            1,
            Clock::at(graduated.due, clock().zone),
        )
        .unwrap();
        assert_eq!(lapse.phase, Phase::Relearning);
        assert_eq!(lapse.lapses, 1);
    }
    #[test]
    fn empty_steps_graduate_and_maximum_interval_is_honored() {
        let s = DeckSettings {
            learning_steps: vec![],
            maximum_interval: 1,
            ..Default::default()
        };
        for grade in 1..=4 {
            let n = next_schedule(&Schedule::new(clock().now), &s, grade, clock()).unwrap();
            assert_eq!(n.phase, Phase::Review);
            assert_eq!(n.scheduled_days, 1);
        }
    }
    #[test]
    fn learning_steps_can_cross_midnight() {
        let c = Clock::at(1_780_012_790, chrono_tz::UTC);
        let n = next_schedule(&Schedule::new(c.now), &DeckSettings::default(), 1, c).unwrap();
        assert_eq!(n.due - c.now, 60);
    }
}
