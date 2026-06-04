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
        }
    }

    if !parsed_something {
        return Err(format!("could not parse duration: {s}"));
    }
    Ok(total_minutes)
}

pub fn parse_datetime(s: &str) -> Result<String, String> {
    let s = s.trim();
    let now = jiff::Zoned::now();
    let today = now.date();

    // Full ISO 8601 timestamp
    if let Ok(ts) = jiff::Timestamp::from_str(s) {
        return Ok(ts.to_string());
    }

    // Full ISO datetime or "2025-06-05 14:00" with explicit timezone
    let normalized = if s.contains(' ') && !s.contains('T') {
        s.replace(' ', "T")
    } else {
        s.to_string()
    };
    if !normalized.ends_with('Z') && !normalized.contains('+') && !normalized.contains('-')
        || normalized.contains('-') && normalized.matches('-').count() >= 2
    {
        let with_z = if !normalized.ends_with('Z') && !normalized.contains('+') {
            format!("{normalized}Z")
        } else {
            normalized.clone()
        };
        if let Ok(ts) = jiff::Timestamp::from_str(&with_z) {
            return Ok(ts.to_string());
        }
    }

    // "06-15" → this year June 15th end-of-day
    // "-06" → this year this month day 6 end-of-day
    // "06-15T14:00" → this year June 15 14:00 UTC
    // "-06T14:00" → this year this month day 6 14:00 UTC
    if s.starts_with('-') && !s.starts_with("--") {
        // "-DD" → today's year & month, given day
        let rest = &s[1..];
        let (day_str, time_part) = rest
            .split_once('T')
            .or_else(|| rest.split_once(' '))
            .unwrap_or((rest, ""));
        let day: i8 = day_str
            .parse()
            .map_err(|_| format!("invalid day: {day_str}"))?;
        let dt = try_build_datetime(today.year(), today.month(), day, time_part)?;
        return dt_to_iso(dt);
    }

    if let Some(idx) = s.find('-') {
        let prefix = &s[..idx];
        let rest = &s[idx + 1..];
        // Check if prefix is a 2-digit month (not a 4-digit year)
        if prefix.len() == 2 && prefix.chars().all(|c| c.is_ascii_digit()) {
            // "MM-DD" or "MM-DDThh:mm"
            let month: i8 = prefix
                .parse()
                .map_err(|_| format!("invalid month: {prefix}"))?;
            let (day_str, time_part) = rest
                .split_once('T')
                .or_else(|| rest.split_once(' '))
                .unwrap_or((rest, ""));
            let (day_str, time_part) = if day_str.contains('-') {
                // "MM-DD-..." is ambiguous, bail out
                return Err(format!("ambiguous date format: {s}"));
            } else {
                (day_str, time_part)
            };
            let day: i8 = day_str
                .parse()
                .map_err(|_| format!("invalid day: {day_str}"))?;
            let dt = try_build_datetime(today.year(), month, day, time_part)?;
            return dt_to_iso(dt);
        }
    }

    // Full date without timezone: "2025-06-05"
    if let Ok(d) = jiff::civil::Date::from_str(s) {
        let dt = d
            .at(23, 59, 59, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_err(|e| format!("could not convert date to UTC: {e}"))?;
        return Ok(dt.timestamp().to_string());
    }

    // Full datetime without timezone: "2025-06-05T14:00"
    if let Ok(dt) = jiff::civil::DateTime::from_str(&normalized) {
        let ts = dt
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_err(|e| format!("could not convert to UTC: {e}"))?;
        return Ok(ts.timestamp().to_string());
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

fn dt_to_iso(dt: jiff::civil::DateTime) -> Result<String, String> {
    let ts = dt
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map_err(|e| format!("could not convert to UTC: {e}"))?;
    Ok(ts.timestamp().to_string())
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
}
