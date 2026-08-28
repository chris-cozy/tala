//! Study-day calculations using local calendar dates, with UTC persistence.
//! Learning delays use elapsed seconds; review intervals use local midnights.

use chrono::{DateTime, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Clone, Copy)]
pub struct Clock {
    pub now: i64,
    pub zone: Tz,
}
impl Clock {
    pub fn system() -> Self {
        Self {
            now: Utc::now().timestamp(),
            zone: iana_time_zone::get_timezone()
                .ok()
                .and_then(|z| z.parse().ok())
                .unwrap_or(chrono_tz::UTC),
        }
    }
    pub fn at(now: i64, zone: Tz) -> Self {
        Self { now, zone }
    }
    pub fn date_at(&self, timestamp: i64) -> NaiveDate {
        DateTime::from_timestamp(timestamp, 0)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
            .with_timezone(&self.zone)
            .date_naive()
    }
    pub fn day(&self) -> String {
        self.date_at(self.now).to_string()
    }
    pub fn midnight(&self, date: NaiveDate) -> i64 {
        // Some zones skip or repeat midnight. Choose the first valid instant of that day.
        for minute in 0..180 {
            let local = date.and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(minute);
            match self.zone.from_local_datetime(&local) {
                LocalResult::Single(d) => return d.timestamp(),
                LocalResult::Ambiguous(a, b) => return a.timestamp().min(b.timestamp()),
                LocalResult::None => {}
            }
        }
        date.and_hms_opt(3, 0, 0).unwrap().and_utc().timestamp()
    }
    pub fn day_start(&self) -> i64 {
        self.midnight(self.date_at(self.now))
    }
    pub fn after_days(&self, days: u32) -> i64 {
        self.midnight(self.date_at(self.now) + Duration::days(days as i64))
    }
    pub fn elapsed_days(&self, then: i64) -> u32 {
        (self.date_at(self.now) - self.date_at(then))
            .num_days()
            .max(0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn midnight_observes_dst() {
        let now = DateTime::parse_from_rfc3339("2026-03-08T00:00:00-05:00")
            .unwrap()
            .timestamp();
        let clock = Clock::at(now, chrono_tz::America::New_York);
        assert_eq!(clock.after_days(1) - clock.day_start(), 23 * 3600);
        assert_eq!(clock.day(), "2026-03-08");
    }
    #[test]
    fn backwards_clock_never_negative() {
        let c = Clock::at(1000, chrono_tz::UTC);
        assert_eq!(c.elapsed_days(200_000), 0);
    }
}
