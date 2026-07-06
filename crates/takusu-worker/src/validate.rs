//! Input validation at the API boundary.
//!
//! These helpers mirror the validation performed by `takusu-local-lib` so the
//! Worker rejects bad input with `400 Bad Request` instead of storing it and
//! crashing later (e.g. during schedule generation).

use serde::Deserialize;

use crate::error::WorkerError;

/// Mirror of `takusu_habit::RecurrenceRule` used only for JSON validation.
/// We duplicate the shape here to avoid pulling `takusu-habit` (and its
/// `jiff` / `takusu-core` / `rand` dependencies) into the WASM bundle, which
/// matches the existing convention of duplicating storage row types in
/// `models.rs`.
///
/// Field optionality matches the canonical type exactly: the canonical
/// `RecurrenceRule` declares every field as required (no `#[serde(default)]`),
/// so this mirror does the same — JSON missing any field is rejected, keeping
/// the worker as strict as the local server.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RecurrenceRule {
    freq: Frequency,
    interval: u32,
    by_day: Vec<NWeekday>,
    by_month: Vec<i8>,
    by_month_day: Vec<i8>,
    count: Option<u32>,
    #[serde(with = "date_strings")]
    exdates: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct NWeekday {
    n: Option<i8>,
    weekday: Weekday,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// Mirror of `takusu_habit::date_strings` that validates each entry is a
/// real `YYYY-MM-DD` calendar date (matching `jiff::civil::Date::strptime`
/// with `%Y-%m-%d`). We avoid `jiff` here to keep the WASM bundle lean.
mod date_strings {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let strings: Vec<String> = Vec::<String>::deserialize(deserializer)?;
        for s in &strings {
            validate_calendar_date(s).map_err(serde::de::Error::custom)?;
        }
        Ok(strings)
    }

    /// Validate that `s` is a real calendar date in `YYYY-MM-DD` form.
    fn validate_calendar_date(s: &str) -> Result<(), String> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 {
            return Err(format!("invalid date: {s}"));
        }
        let y: i64 = parts[0].parse().map_err(|_| format!("invalid date: {s}"))?;
        let m: u32 = parts[1].parse().map_err(|_| format!("invalid date: {s}"))?;
        let d: u32 = parts[2].parse().map_err(|_| format!("invalid date: {s}"))?;
        if !(1..=12).contains(&m) {
            return Err(format!("invalid date: {s}"));
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let max_day = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        if !(1..=max_day).contains(&d) {
            return Err(format!("invalid date: {s}"));
        }
        Ok(())
    }
}

/// Reject negative `avg_minutes` / `sigma_minutes`, which would wrap to a
/// huge `u64` slot count in the planner and break the schedule (#269).
pub(crate) fn validate_minutes(avg: i64, sigma: Option<i64>) -> Result<(), WorkerError> {
    if avg < 0 {
        return Err(WorkerError::BadRequest(format!(
            "avg_minutes must be >= 0 (got {avg})"
        )));
    }
    if let Some(s) = sigma
        && s < 0
    {
        return Err(WorkerError::BadRequest(format!(
            "sigma_minutes must be >= 0 (got {s})"
        )));
    }
    Ok(())
}

/// Verify the recurrence string parses as a `RecurrenceRule` so that bad JSON
/// is rejected at the API boundary instead of crashing later (#285).
pub(crate) fn validate_recurrence(recurrence: &str) -> Result<(), WorkerError> {
    serde_json::from_str::<RecurrenceRule>(recurrence)
        .map_err(|e| WorkerError::BadRequest(format!("invalid recurrence: {e}")))?;
    Ok(())
}

/// Validate that `start` and `end` are real `YYYY-MM-DD` calendar dates and
/// that `start <= end`. Mirrors the sqlite-side `validate_pause_dates`.
pub(crate) fn validate_pause_dates(start: &str, end: &str) -> Result<(), WorkerError> {
    let s = parse_calendar_date(start)
        .ok_or_else(|| WorkerError::BadRequest(format!("invalid start_date: {start}")))?;
    let e = parse_calendar_date(end)
        .ok_or_else(|| WorkerError::BadRequest(format!("invalid end_date: {end}")))?;
    if s > e {
        return Err(WorkerError::BadRequest(format!(
            "start_date ({start}) must be <= end_date ({end})"
        )));
    }
    Ok(())
}

/// Parse a `YYYY-MM-DD` string into a `(year, month, day)` tuple if it is a
/// real calendar date, else `None`.
///
/// Enforces zero-padded fields (4-digit year, 2-digit month/day) so that
/// lexicographic comparison against `jiff`'s zero-padded `Date::to_string()`
/// works correctly during pause matching (#303).
fn parse_calendar_date(s: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let max_day = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=max_day).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_reject_negative_avg() {
        assert!(validate_minutes(-1, None).is_err());
        assert!(validate_minutes(0, None).is_ok());
    }

    #[test]
    fn minutes_reject_negative_sigma() {
        assert!(validate_minutes(10, Some(-1)).is_err());
        assert!(validate_minutes(10, Some(0)).is_ok());
    }

    #[test]
    fn recurrence_rejects_garbage() {
        assert!(validate_recurrence("not json").is_err());
    }

    #[test]
    fn recurrence_accepts_valid_rule() {
        let rule = r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#;
        assert!(validate_recurrence(rule).is_ok());
    }

    #[test]
    fn recurrence_rejects_missing_required_field() {
        // Missing interval/by_day/etc. — the canonical type requires them.
        let rule = r#"{"freq":"daily"}"#;
        assert!(validate_recurrence(rule).is_err());
    }

    #[test]
    fn recurrence_rejects_invalid_freq() {
        let rule = r#"{"freq":"hourly","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":[]}"#;
        assert!(validate_recurrence(rule).is_err());
    }

    #[test]
    fn recurrence_rejects_invalid_exdate() {
        let rule = r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":["notadate"]}"#;
        assert!(validate_recurrence(rule).is_err());
    }

    #[test]
    fn recurrence_rejects_impossible_calendar_date() {
        // 2026-02-30 is not a real date.
        let rule = r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":["2026-02-30"]}"#;
        assert!(validate_recurrence(rule).is_err());
    }

    #[test]
    fn recurrence_accepts_leap_day() {
        let rule = r#"{"freq":"daily","interval":1,"by_day":[],"by_month":[],"by_month_day":[],"count":null,"exdates":["2024-02-29"]}"#;
        assert!(validate_recurrence(rule).is_ok());
    }

    #[test]
    fn pause_dates_accepts_valid_range() {
        assert!(validate_pause_dates("2026-08-01", "2026-08-07").is_ok());
        assert!(validate_pause_dates("2026-08-07", "2026-08-07").is_ok());
    }

    #[test]
    fn pause_dates_rejects_reversed() {
        assert!(validate_pause_dates("2026-08-07", "2026-08-01").is_err());
    }

    #[test]
    fn pause_dates_rejects_bad_format() {
        assert!(validate_pause_dates("2026/08/01", "2026-08-07").is_err());
        assert!(validate_pause_dates("2026-08-01", "notadate").is_err());
        assert!(validate_pause_dates("2026-13-01", "2026-08-07").is_err());
        assert!(validate_pause_dates("2026-02-30", "2026-08-07").is_err());
    }

    #[test]
    fn pause_dates_rejects_non_zero_padded() {
        // Non-zero-padded dates would pass numeric parsing but break the
        // lexicographic comparison against jiff's zero-padded Date::to_string,
        // so they must be rejected (#303).
        assert!(validate_pause_dates("2026-8-1", "2026-08-07").is_err());
        assert!(validate_pause_dates("2026-08-01", "2026-8-7").is_err());
        assert!(validate_pause_dates("026-08-01", "2026-08-07").is_err());
    }
}
