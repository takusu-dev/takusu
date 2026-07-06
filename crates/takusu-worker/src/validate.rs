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
}
