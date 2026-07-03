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
            match unit {
                'h' => total_minutes += num * 60,
                'm' => total_minutes += num,
                's' => total_minutes += num * 5,
                _ => {
                    return Err(format!(
                        "unknown unit '{unit}' in duration (use h, m, s for slots)"
                    ));
                }
            }
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
    parse_datetime_tz(s, &jiff::tz::TimeZone::UTC)
}

pub fn parse_datetime_tz(s: &str, tz: &jiff::tz::TimeZone) -> Result<String, String> {
    let s = s.trim();

    if s.eq_ignore_ascii_case("now") {
        return Ok(jiff::Timestamp::now().to_string());
    }

    let today = jiff::Timestamp::now().to_zoned(tz.clone()).date();

    // Full ISO 8601 timestamp
    if let Ok(ts) = jiff::Timestamp::from_str(s) {
        return Ok(ts.to_string());
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
        return dt_to_iso(dt, tz);
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
            return dt_to_iso(dt, tz);
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
            return Ok(zdt.timestamp().to_string());
        }
    } else if let Ok(d) = jiff::civil::Date::from_str(s) {
        // Full date without timezone: "2025-06-05" → end-of-day in tz
        let zdt = d
            .at(23, 59, 59, 0)
            .to_zoned(tz.clone())
            .map_err(|e| format!("could not convert date: {e}"))?;
        return Ok(zdt.timestamp().to_string());
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

fn dt_to_iso(dt: jiff::civil::DateTime, tz: &jiff::tz::TimeZone) -> Result<String, String> {
    let zdt = dt
        .to_zoned(tz.clone())
        .map_err(|e| format!("could not convert to timezone: {e}"))?;
    Ok(zdt.timestamp().to_string())
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

    // ── parse_range edge cases ──────────────────────────────────────────

    #[test]
    fn parse_range_to_separator() {
        let (from, until) = parse_range("2025-06-05T09:00:00Z to 2025-06-06T09:00:00Z").unwrap();
        assert!(from.starts_with("2025-06-05T09:00:00"));
        assert!(until.starts_with("2025-06-06T09:00:00"));
    }

    #[test]
    fn parse_range_duration_hours() {
        let now = jiff::Timestamp::now();
        let (from, until) = parse_range("1.5h").unwrap();
        let from_ts = jiff::Timestamp::from_str(&from).unwrap();
        let until_ts = jiff::Timestamp::from_str(&until).unwrap();
        // from ≈ now (within a second), until = from + 5400s
        assert!((from_ts.as_second() - now.as_second()).abs() <= 2);
        assert_eq!(until_ts.as_second() - from_ts.as_second(), 5400);
    }

    #[test]
    fn parse_range_duration_days_and_weeks() {
        let (from_d, until_d) = parse_range("2d").unwrap();
        let f = jiff::Timestamp::from_str(&from_d).unwrap();
        let u = jiff::Timestamp::from_str(&until_d).unwrap();
        assert_eq!(u.as_second() - f.as_second(), 2 * 86400);

        let (from_w, until_w) = parse_range("1w").unwrap();
        let f = jiff::Timestamp::from_str(&from_w).unwrap();
        let u = jiff::Timestamp::from_str(&until_w).unwrap();
        assert_eq!(u.as_second() - f.as_second(), 7 * 86400);
    }

    #[test]
    fn parse_range_duration_minutes_unit() {
        let (from, until) = parse_range("90min").unwrap();
        let f = jiff::Timestamp::from_str(&from).unwrap();
        let u = jiff::Timestamp::from_str(&until).unwrap();
        assert_eq!(u.as_second() - f.as_second(), 90 * 60);
    }

    #[test]
    fn parse_range_empty_errors() {
        assert!(parse_range("").is_err());
    }

    #[test]
    fn parse_range_unknown_unit_errors() {
        assert!(parse_range("5x").is_err());
    }

    #[test]
    fn parse_range_single_datetime_uses_now_as_from() {
        let now = jiff::Timestamp::now();
        let (from, until) = parse_range("2025-06-05T09:00:00Z").unwrap();
        let from_ts = jiff::Timestamp::from_str(&from).unwrap();
        // from ≈ now
        assert!((from_ts.as_second() - now.as_second()).abs() <= 2);
        assert!(until.starts_with("2025-06-05T09:00:00"));
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
}
