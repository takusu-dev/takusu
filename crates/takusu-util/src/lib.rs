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

    if let Ok(ts) = jiff::Timestamp::from_str(s) {
        return Ok(ts.to_string());
    }

    if let Ok(dt) = jiff::civil::DateTime::from_str(s) {
        let ts = dt
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_err(|e| format!("could not convert to UTC: {e}"))?;
        return Ok(ts.timestamp().to_string());
    }

    if let Ok(d) = jiff::civil::Date::from_str(s) {
        let dt = d
            .at(23, 59, 59, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_err(|e| format!("could not convert date to UTC: {e}"))?;
        return Ok(dt.timestamp().to_string());
    }

    let with_z = format!("{s}Z");
    if let Ok(ts) = jiff::Timestamp::from_str(&with_z) {
        return Ok(ts.to_string());
    }

    let with_tz = if s.contains(' ') && !s.contains('T') {
        format!("{}Z", s.replace(' ', "T"))
    } else {
        format!("{s}Z")
    };
    if let Ok(ts) = jiff::Timestamp::from_str(&with_tz) {
        return Ok(ts.to_string());
    }

    Err(format!(
        "could not parse datetime: {s} (expected ISO 8601, e.g. 2025-06-05T14:00:00Z or 2025-06-05 14:00)"
    ))
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
}
