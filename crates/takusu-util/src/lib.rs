use std::str::FromStr;
use uuid::Uuid;

pub fn generate_root_token() -> String {
    format!("tsk_{}", Uuid::now_v7())
}

pub fn parse_duration(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".to_string());
    }

    if s.chars().all(|c| c.is_ascii_digit()) {
        let mins: i64 = s.parse().map_err(|_| format!("invalid number: {s}"))?;
        return Ok(mins);
    }

    let mut total_minutes: i64 = 0;
    let mut num_start = 0;
    let mut chars = s.char_indices().peekable();
    let mut parsed_something = false;
    let mut pending_number = false;

    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_digit() {
            while let Some(&(.., c)) = chars.peek() {
                if c.is_ascii_digit() {
                    chars.next();
                } else {
                    break;
                }
            }
            num_start = i;
            pending_number = true;
        } else {
            let unit = c;
            chars.next();
            let num_str = &s[num_start..i];
            let num: i64 = num_str
                .parse()
                .map_err(|_| format!("invalid number in duration: {num_str}"))?;
            let value = match unit {
                'h' => num.checked_mul(60),
                'm' => Some(num),
                's' => num.checked_mul(5),
                _ => {
                    return Err(format!(
                        "unknown unit '{unit}' in duration (use h, m, s for slots)"
                    ));
                }
            }
            .ok_or_else(|| format!("duration overflow in {num}{unit}"))?;
            total_minutes = total_minutes
                .checked_add(value)
                .ok_or_else(|| "duration overflow".to_string())?;
            parsed_something = true;
            pending_number = false;
        }
    }

    if !parsed_something {
        return Err(format!("could not parse duration: {s}"));
    }
    if pending_number {
        return Err(format!(
            "trailing number without unit in duration: {s} (use h, m, s for slots)"
        ));
    }
    Ok(total_minutes)
}

pub fn parse_datetime(s: &str) -> Result<String, String> {
    parse_datetime_to_timestamp(s, &jiff::tz::TimeZone::UTC).map(|ts| ts.to_string())
}

pub fn parse_datetime_tz(s: &str, tz: &jiff::tz::TimeZone) -> Result<String, String> {
    parse_datetime_to_timestamp(s, tz).map(|ts| ts.to_string())
}

pub fn parse_datetime_to_timestamp(
    s: &str,
    tz: &jiff::tz::TimeZone,
) -> Result<jiff::Timestamp, String> {
    let s = s.trim();

    if s.eq_ignore_ascii_case("now") {
        return Ok(jiff::Timestamp::now());
    }

    let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();

    // Full ISO 8601 timestamp
    if let Ok(ts) = jiff::Timestamp::from_str(s) {
        return Ok(ts);
    }

    // Normalize "2025-06-05 14:00" → "2025-06-05T14:00" so the civil
    // datetime fallback below can parse it. Datetimes without an explicit
    // timezone are interpreted in the configured tz (see the fallback).
    let normalized = if s.contains(' ') && !s.contains('T') {
        s.replace(' ', "T")
    } else {
        s.to_string()
    };

    // "06-15" → this year June 15th end-of-day (in configured tz)
    // "-06" → this year this month day 6 end-of-day (in configured tz)
    // "06-15T14:00" → this year June 15 14:00 (in configured tz)
    // "-06T14:00" → this year this month day 6 14:00 (in configured tz)
    if s.starts_with('-') && !s.starts_with("--") {
        let rest = &s[1..];
        let (day_str, time_part) = rest
            .split_once('T')
            .or_else(|| rest.split_once(' '))
            .unwrap_or((rest, ""));
        let day: i8 = day_str
            .parse()
            .map_err(|_| format!("invalid day: {day_str}"))?;
        let dt = try_build_datetime(today.year(), today.month(), day, time_part)?;
        return dt_to_timestamp(dt, tz);
    }

    if let Some(idx) = s.find('-') {
        let prefix = &s[..idx];
        let rest = &s[idx + 1..];
        if prefix.len() == 2 && prefix.chars().all(|c| c.is_ascii_digit()) {
            let month: i8 = prefix
                .parse()
                .map_err(|_| format!("invalid month: {prefix}"))?;
            let (day_str, time_part) = rest
                .split_once('T')
                .or_else(|| rest.split_once(' '))
                .unwrap_or((rest, ""));
            let (day_str, time_part) = if day_str.contains('-') {
                return Err(format!("ambiguous date format: {s}"));
            } else {
                (day_str, time_part)
            };
            let day: i8 = day_str
                .parse()
                .map_err(|_| format!("invalid day: {day_str}"))?;
            let dt = try_build_datetime(today.year(), month, day, time_part)?;
            return dt_to_timestamp(dt, tz);
        }
    }

    // Full datetime without timezone: "2025-06-05T14:00", interpreted in
    // the configured tz. Checked before the date-only branch because
    // `civil::Date::from_str` accepts datetime strings and truncates the
    // time part.
    if normalized.contains('T') {
        if let Ok(dt) = jiff::civil::DateTime::from_str(&normalized) {
            let zdt = dt
                .to_zoned(tz.clone())
                .map_err(|e| format!("could not convert datetime: {e}"))?;
            return Ok(zdt.timestamp());
        }
    } else if let Ok(d) = jiff::civil::Date::from_str(s) {
        // Full date without timezone: "2025-06-05" → end-of-day in tz
        let zdt = d
            .at(23, 59, 59, 0)
            .to_zoned(tz.clone())
            .map_err(|e| format!("could not convert date: {e}"))?;
        return Ok(zdt.timestamp());
    }

    Err(format!(
        "could not parse datetime: {s} (e.g. 2025-06-05, 06-15, -06, 06-15T14:00)"
    ))
}

fn try_build_datetime(
    year: i16,
    month: i8,
    day: i8,
    time_part: &str,
) -> Result<jiff::civil::DateTime, String> {
    let date =
        jiff::civil::Date::new(year, month, day).map_err(|e| format!("invalid date: {e}"))?;
    let dt = if time_part.is_empty() {
        date.at(23, 59, 59, 0)
    } else {
        let t: jiff::civil::Time = time_part
            .parse()
            .map_err(|e| format!("invalid time '{time_part}': {e}"))?;
        date.at(t.hour(), t.minute(), t.second(), t.subsec_nanosecond())
    };
    Ok(dt)
}

fn dt_to_timestamp(
    dt: jiff::civil::DateTime,
    tz: &jiff::tz::TimeZone,
) -> Result<jiff::Timestamp, String> {
    let zdt = dt
        .to_zoned(tz.clone())
        .map_err(|e| format!("could not convert to timezone: {e}"))?;
    Ok(zdt.timestamp())
}

/// Parse a flexible date expression into a timestamp.
///
/// Supported inputs:
/// - `"now"` -> current time.
/// - `"today"` -> start or end of today in the configured timezone.
/// - `"7d"` or `"+7d"` or `"-7d"` -> start or end of the date N days from today.
/// - `"2026-07-20"` -> start or end of that date in the configured timezone.
/// - Full RFC 3339 / ISO 8601 timestamps or naive datetimes are passed through.
///
/// `end_of_day` controls whether date-only expressions resolve to the start
/// (`00:00:00`) or end (`23:59:59`) of the day. It is ignored for absolute
/// timestamps and `now`.
pub fn parse_date_expression(
    s: &str,
    tz: &jiff::tz::TimeZone,
    end_of_day: bool,
) -> Result<jiff::Timestamp, String> {
    let s = s.trim();

    if s.eq_ignore_ascii_case("now") {
        return Ok(jiff::Timestamp::now());
    }

    let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();

    if s.eq_ignore_ascii_case("today") {
        let dt = if end_of_day {
            today.at(23, 59, 59, 0)
        } else {
            today.at(0, 0, 0, 0)
        };
        return dt_to_timestamp(dt, tz);
    }

    // Relative days: "7d", "+7d", "-7d".
    if let Some(days_str) = s.strip_suffix('d').or_else(|| s.strip_suffix('D'))
        && let Ok(days) = days_str.trim().parse::<i64>()
    {
        let date = today
            .checked_add(jiff::Span::new().days(days))
            .map_err(|_| format!("day offset out of range: {s}"))?;
        let dt = if end_of_day {
            date.at(23, 59, 59, 0)
        } else {
            date.at(0, 0, 0, 0)
        };
        return dt_to_timestamp(dt, tz);
    }

    // Absolute date: "2026-07-20".
    if let Ok(date) = jiff::civil::Date::from_str(s) {
        let dt = if end_of_day {
            date.at(23, 59, 59, 0)
        } else {
            date.at(0, 0, 0, 0)
        };
        return dt_to_timestamp(dt, tz);
    }

    // Fallback to full datetime parsing.
    parse_datetime_to_timestamp(s, tz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_number() {
        assert_eq!(parse_duration("30").unwrap(), 30);
    }

    #[test]
    fn test_hours_and_minutes() {
        assert_eq!(parse_duration("1h30m").unwrap(), 90);
    }

    #[test]
    fn test_minutes() {
        assert_eq!(parse_duration("30m").unwrap(), 30);
    }

    #[test]
    fn test_slots() {
        assert_eq!(parse_duration("30s").unwrap(), 150);
    }

    #[test]
    fn test_hours_only() {
        assert_eq!(parse_duration("2h").unwrap(), 120);
    }

    #[test]
    fn test_combined() {
        assert_eq!(parse_duration("1h15m").unwrap(), 75);
    }

    #[test]
    fn test_slots_and_minutes() {
        assert_eq!(parse_duration("6s").unwrap(), 30);
    }

    #[test]
    fn test_parse_datetime_iso() {
        let result = parse_datetime("2025-06-05T14:00:00Z").unwrap();
        assert!(result.starts_with("2025-06-05T14:00:00"));
    }

    #[test]
    fn test_parse_datetime_space() {
        let result = parse_datetime("2025-06-05 14:00").unwrap();
        assert!(result.starts_with("2025-06-05T14:00"));
    }

    #[test]
    fn test_parse_datetime_date_only() {
        let result = parse_datetime("2025-06-05").unwrap();
        assert!(result.starts_with("2025-06-05"));
    }

    #[test]
    fn test_parse_datetime_day_only() {
        let now = jiff::Zoned::now();
        let result = parse_datetime("-06").unwrap();
        let ts = jiff::Timestamp::from_str(&result).unwrap();
        let expected = jiff::civil::Date::new(now.year(), now.month(), 6)
            .unwrap()
            .at(23, 59, 59, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .unwrap()
            .timestamp();
        assert_eq!(ts, expected);
    }

    #[test]
    fn test_parse_datetime_month_day() {
        let result = parse_datetime("06-15").unwrap();
        let ts = jiff::Timestamp::from_str(&result).unwrap();
        let expected = jiff::civil::Date::new(jiff::Zoned::now().year(), 6, 15)
            .unwrap()
            .at(23, 59, 59, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .unwrap()
            .timestamp();
        assert_eq!(ts, expected);
    }

    #[test]
    fn test_parse_datetime_month_day_with_time() {
        let result = parse_datetime("06-15T14:00").unwrap();
        assert!(result.contains("T14:00"));
    }

    #[test]
    fn test_parse_datetime_day_with_time() {
        let result = parse_datetime("-06T14:30").unwrap();
        assert!(result.contains("T14:30"));
    }

    #[test]
    fn test_trailing_number_without_unit_errors() {
        assert!(parse_duration("1h30").is_err());
    }

    #[test]
    fn test_parse_datetime_naive_uses_configured_tz() {
        let tz = jiff::tz::TimeZone::get("Asia/Tokyo").unwrap();
        let result = parse_datetime_tz("2025-06-05T14:00", &tz).unwrap();
        // 14:00 JST == 05:00 UTC
        let ts = jiff::Timestamp::from_str(&result).unwrap();
        let expected = jiff::civil::date(2025, 6, 5)
            .at(14, 0, 0, 0)
            .to_zoned(tz)
            .unwrap()
            .timestamp();
        assert_eq!(ts, expected);
    }

    #[test]
    fn test_parse_datetime_explicit_offset_preserved() {
        let tz = jiff::tz::TimeZone::get("Asia/Tokyo").unwrap();
        let result = parse_datetime_tz("2025-06-05T14:00:00Z", &tz).unwrap();
        assert!(result.starts_with("2025-06-05T14:00:00"));
    }

    // ── parse_duration edge cases ───────────────────────────────────────

    #[test]
    fn parse_duration_empty_errors() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("   ").is_err());
    }

    #[test]
    fn parse_duration_overflow_errors() {
        let max = i64::MAX.to_string();
        assert!(parse_duration(&format!("{max}h")).is_err());
        assert!(parse_duration(&format!("{max}m1m")).is_err());
    }

    #[test]
    fn parse_duration_unknown_unit_errors() {
        assert!(parse_duration("5x").is_err());
        assert!(
            parse_duration("1d").is_err(),
            "'d' is not a duration unit here"
        );
    }

    #[test]
    fn parse_duration_unit_without_number_errors() {
        assert!(parse_duration("h").is_err());
        assert!(parse_duration("m").is_err());
    }

    #[test]
    fn parse_duration_zero_pure_number() {
        assert_eq!(parse_duration("0").unwrap(), 0);
    }

    #[test]
    fn parse_duration_trims_whitespace() {
        assert_eq!(parse_duration("  30m  ").unwrap(), 30);
    }

    #[test]
    fn parse_duration_s_is_slots_not_seconds() {
        // Documented footgun: 's' means 5-min slots, not seconds.
        // 1s = 1 slot = 5 minutes.
        assert_eq!(parse_duration("1s").unwrap(), 5);
        assert_eq!(parse_duration("12s").unwrap(), 60);
    }

    #[test]
    fn parse_duration_multiple_units() {
        assert_eq!(parse_duration("1h30m15s").unwrap(), 60 + 30 + 75);
    }

    // ── parse_datetime edge cases ───────────────────────────────────────

    #[test]
    fn parse_datetime_now_keyword() {
        let now = jiff::Timestamp::now();
        let result = parse_datetime("now").unwrap();
        let ts = jiff::Timestamp::from_str(&result).unwrap();
        assert!((ts.as_second() - now.as_second()).abs() <= 2);
    }

    #[test]
    fn parse_datetime_ambiguous_dash_format_errors() {
        // "06-15-2025" looks like month-day-year but is ambiguous → error
        assert!(parse_datetime("06-15-2025").is_err());
    }

    #[test]
    fn parse_datetime_garbage_errors() {
        assert!(parse_datetime("hello world").is_err());
        assert!(parse_datetime("2025-13-45").is_err());
    }

    #[test]
    fn generate_root_token_format() {
        let t = generate_root_token();
        assert!(t.starts_with("tsk_"), "token must start with tsk_: {t}");
        // UUID v7 is 36 chars including dashes; prefix is 4 chars.
        assert_eq!(t.len(), 4 + 36);
    }

    // ── parse_date_expression ───────────────────────────────────────────

    #[test]
    fn parse_date_expression_now() {
        let now = jiff::Timestamp::now();
        let tz = jiff::tz::TimeZone::UTC;
        let result = parse_date_expression("now", &tz, false).unwrap();
        assert!((result.as_second() - now.as_second()).abs() <= 2);
    }

    #[test]
    fn parse_date_expression_today_start_and_end() {
        let tz = jiff::tz::TimeZone::UTC;
        let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();
        let start = parse_date_expression("today", &tz, false).unwrap();
        let end = parse_date_expression("today", &tz, true).unwrap();
        assert_eq!(
            start.to_zoned(tz.clone()).date().to_string(),
            today.to_string()
        );
        assert_eq!(start.to_zoned(tz.clone()).time().to_string(), "00:00:00");
        assert_eq!(
            end.to_zoned(tz.clone()).date().to_string(),
            today.to_string()
        );
        assert_eq!(end.to_zoned(tz.clone()).time().to_string(), "23:59:59");
    }

    #[test]
    fn parse_date_expression_relative_days() {
        let tz = jiff::tz::TimeZone::UTC;
        let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();
        let expected = today.checked_add(jiff::Span::new().days(7)).unwrap();
        let start = parse_date_expression("7d", &tz, false).unwrap();
        let end = parse_date_expression("7d", &tz, true).unwrap();
        assert_eq!(
            start.to_zoned(tz.clone()).date().to_string(),
            expected.to_string()
        );
        assert_eq!(start.to_zoned(tz.clone()).time().to_string(), "00:00:00");
        assert_eq!(
            end.to_zoned(tz.clone()).date().to_string(),
            expected.to_string()
        );
        assert_eq!(end.to_zoned(tz.clone()).time().to_string(), "23:59:59");

        // "+7d" must produce the same timestamp as "7d".
        assert_eq!(parse_date_expression("+7d", &tz, false).unwrap(), start);
        assert_eq!(parse_date_expression("+7d", &tz, true).unwrap(), end);
    }

    #[test]
    fn parse_date_expression_today_and_relative_in_non_utc_timezone() {
        let tz = jiff::tz::TimeZone::get("Asia/Tokyo").unwrap();
        let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();

        let start = parse_date_expression("today", &tz, false).unwrap();
        let end = parse_date_expression("today", &tz, true).unwrap();
        assert_eq!(
            start.to_zoned(tz.clone()).date().to_string(),
            today.to_string()
        );
        assert_eq!(start.to_zoned(tz.clone()).time().to_string(), "00:00:00");
        assert_eq!(
            end.to_zoned(tz.clone()).date().to_string(),
            today.to_string()
        );
        assert_eq!(end.to_zoned(tz.clone()).time().to_string(), "23:59:59");

        let expected = today.checked_add(jiff::Span::new().days(7)).unwrap();
        let start = parse_date_expression("7d", &tz, false).unwrap();
        let end = parse_date_expression("7d", &tz, true).unwrap();
        assert_eq!(
            start.to_zoned(tz.clone()).date().to_string(),
            expected.to_string()
        );
        assert_eq!(start.to_zoned(tz.clone()).time().to_string(), "00:00:00");
        assert_eq!(
            end.to_zoned(tz.clone()).date().to_string(),
            expected.to_string()
        );
        assert_eq!(end.to_zoned(tz.clone()).time().to_string(), "23:59:59");
    }

    #[test]
    fn parse_date_expression_negative_days() {
        let tz = jiff::tz::TimeZone::UTC;
        let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();
        let expected = today.checked_add(jiff::Span::new().days(-3)).unwrap();
        let start = parse_date_expression("-3d", &tz, false).unwrap();
        assert_eq!(
            start.to_zoned(tz.clone()).date().to_string(),
            expected.to_string()
        );
        assert_eq!(start.to_zoned(tz.clone()).time().to_string(), "00:00:00");
    }

    #[test]
    fn parse_date_expression_absolute_date() {
        let tz = jiff::tz::TimeZone::UTC;
        let start = parse_date_expression("2026-07-20", &tz, false).unwrap();
        let end = parse_date_expression("2026-07-20", &tz, true).unwrap();
        assert_eq!(start.to_zoned(tz.clone()).date().to_string(), "2026-07-20");
        assert_eq!(start.to_zoned(tz.clone()).time().to_string(), "00:00:00");
        assert_eq!(end.to_zoned(tz.clone()).date().to_string(), "2026-07-20");
        assert_eq!(end.to_zoned(tz.clone()).time().to_string(), "23:59:59");
    }

    #[test]
    fn parse_date_expression_full_datetime_passthrough() {
        let tz = jiff::tz::TimeZone::UTC;
        let expected = jiff::Timestamp::from_str("2026-07-20T12:34:56Z").unwrap();
        let result = parse_date_expression("2026-07-20T12:34:56Z", &tz, true).unwrap();
        assert_eq!(result.as_second(), expected.as_second());
    }

    #[test]
    fn parse_date_expression_invalid_errors() {
        let tz = jiff::tz::TimeZone::UTC;
        assert!(parse_date_expression("hello", &tz, false).is_err());
        assert!(parse_date_expression("", &tz, false).is_err());
    }
}
