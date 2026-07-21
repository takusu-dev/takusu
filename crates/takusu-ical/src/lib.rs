//! # takusu-ical — iCalendar parser
//!
//! iCalendar (.ics) 形式の文字列をパースし、タスク相当の構造体に変換する。
//! HTTP依存なしのpure parser。
//!
//! ## 使用例
//!
//! ```no_run
//! use takusu_ical::parse_ical;
//!
//! let ical = std::fs::read_to_string("calendar.ics").unwrap();
//! let tasks = parse_ical(&ical).unwrap();
//! for task in &tasks {
//!     println!("{}: {} - {}", task.title, task.start_at, task.end_at);
//! }
//! ```
//!
//! ## 変換ルール
//!
//! - `VEVENT` のみ抽出
//! - `DTSTART`/`DTEND`: `YYYYMMDDTHHMMSSZ` → `YYYY-MM-DDTHH:MM:SSZ`
//! - `DTSTART`/`DTEND` (日付のみ): `YYYYMMDD` → `YYYY-MM-DDT00:00:00`
//! - 行折りたたみ (継続行) に対応
//! - 同一 `UID` の重複インポートはスキップ (呼び出し側で制御)

use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IcalError {
    #[error("invalid iCalendar format: {0}")]
    InvalidFormat(String),
    #[error("missing required property: {0}")]
    MissingProperty(String),
    #[error("invalid date format: {0}")]
    InvalidDate(String),
}

/// iCalendarのVEVENTから変換されたタスク表現。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IcalTask {
    pub title: String,
    pub description: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub uid: Option<String>,
}

fn unfold_lines(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();

    for line in input.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            current.push_str(line.trim_start());
        } else {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            current = line.to_string();
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn parse_date<'a>(map: &'a HashMap<String, String>, key: &str) -> Result<&'a str, IcalError> {
    map.get(key)
        .map(|s| s.as_str())
        .ok_or_else(|| IcalError::MissingProperty(key.to_string()))
}

/// Unescape RFC 5545 §3.3.11 TEXT escaping:
/// `\n` / `\N` → newline, `\,` → comma, `\;` → semicolon, `\\` → backslash.
/// Other escaped characters are kept literally (backslash preserved).
fn unescape_ical_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split a trailing UTC designator (`Z`) or explicit UTC offset (`+HHMM` /
/// `-HHMM`, RFC 5545 §3.3.5) off the end of an iCal date-time value.
/// Returns the body (with the suffix removed) and the ISO 8601 suffix to
/// append to the formatted output (`Z`, `+09:00`, `-05:30`, …). When no
/// suffix is present the body is the original input and the suffix is empty.
fn split_offset(raw: &str) -> (&str, String) {
    if let Some(stripped) = raw.strip_suffix('Z') {
        return (stripped, "Z".to_string());
    }
    let bytes = raw.as_bytes();
    if bytes.len() >= 6 {
        let sign_idx = bytes.len() - 5;
        if matches!(bytes[sign_idx], b'+' | b'-') {
            let off = &raw[sign_idx..];
            // off is "+HHMM" / "-HHMM"; the four chars after the sign must
            // be digits for this to be a real offset rather than a date
            // whose 5th-from-last char happens to be a sign.
            if off[1..].chars().all(|c| c.is_ascii_digit()) {
                let body = &raw[..sign_idx];
                let norm = format!("{}:{}", &off[..3], &off[3..5]);
                return (body, norm);
            }
        }
    }
    (raw, String::new())
}

fn format_ical_date(raw: &str) -> Result<String, IcalError> {
    if raw.len() < 8 {
        return Err(IcalError::InvalidDate(raw.to_string()));
    }

    let (s, suffix) = split_offset(raw);

    if let Some(idx) = s.find('T') {
        let date_part = &s[..idx];
        let rest = &s[idx + 1..];

        if date_part.len() >= 8 && rest.len() >= 6 {
            return Ok(format!(
                "{}-{}-{}T{}:{}:{}{}",
                &date_part[0..4],
                &date_part[4..6],
                &date_part[6..8],
                &rest[0..2],
                &rest[2..4],
                &rest[4..6],
                suffix
            ));
        }
    }

    if s.len() >= 14 {
        return Ok(format!(
            "{}-{}-{}T{}:{}:{}{}",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            &s[8..10],
            &s[10..12],
            &s[12..14],
            suffix
        ));
    }

    if s.len() >= 8 {
        return Ok(format!(
            "{}-{}-{}T00:00:00{}",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            suffix
        ));
    }

    Err(IcalError::InvalidDate(raw.to_string()))
}

/// iCalendar文字列をパースして`IcalTask`のリストを返す。
pub fn parse_ical(input: &str) -> Result<Vec<IcalTask>, IcalError> {
    let lines = unfold_lines(input);
    let mut tasks = Vec::new();
    let mut in_vevent = false;
    let mut properties: HashMap<String, String> = HashMap::new();

    for line in &lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (prop_name, prop_value) = if let Some(colon_idx) = line.find(':') {
            let name = &line[..colon_idx];
            let value = &line[colon_idx + 1..];
            (name.to_string(), value.to_string())
        } else {
            continue;
        };

        let key = prop_name.split(';').next().unwrap_or("").to_uppercase();

        match key.as_str() {
            "BEGIN" => {
                if prop_value == "VEVENT" {
                    in_vevent = true;
                    properties.clear();
                }
            }
            "END" => {
                if prop_value == "VEVENT" && in_vevent {
                    in_vevent = false;

                    let title = properties
                        .get("SUMMARY")
                        .map(|s| unescape_ical_text(s))
                        .unwrap_or_else(|| "Untitled".to_string());
                    let description = properties.get("DESCRIPTION").map(|s| unescape_ical_text(s));
                    let uid = properties.get("UID").map(|s| unescape_ical_text(s));

                    let start_raw = parse_date(&properties, "DTSTART")?;
                    let end_raw = parse_date(&properties, "DTEND")?;

                    let start_at = format_ical_date(start_raw)?;
                    let end_at = format_ical_date(end_raw)?;

                    tasks.push(IcalTask {
                        title,
                        description,
                        start_at,
                        end_at,
                        uid,
                    });
                }
            }
            _ => {
                if in_vevent {
                    properties.insert(key, prop_value);
                }
            }
        }
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_event() {
        let ical = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20260605T090000Z
DTEND:20260605T110000Z
SUMMARY:企画書作成
DESCRIPTION:Q3企画書のドラフト
UID:abc123@example.com
END:VEVENT
END:VCALENDAR";

        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "企画書作成");
        assert_eq!(tasks[0].description, Some("Q3企画書のドラフト".to_string()));
        assert_eq!(tasks[0].uid, Some("abc123@example.com".to_string()));
        assert_eq!(tasks[0].start_at, "2026-06-05T09:00:00Z");
        assert_eq!(tasks[0].end_at, "2026-06-05T11:00:00Z");
    }

    #[test]
    fn parse_multiple_events() {
        let ical = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20260605T090000Z
DTEND:20260605T110000Z
SUMMARY:Meeting
UID:a@example.com
END:VEVENT
BEGIN:VEVENT
DTSTART:20260606T140000Z
DTEND:20260606T150000Z
SUMMARY:Review
UID:b@example.com
END:VEVENT
END:VCALENDAR";

        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "Meeting");
        assert_eq!(tasks[1].title, "Review");
    }

    #[test]
    fn parse_date_only() {
        let ical = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20260605
DTEND:20260606
SUMMARY:All-day event
END:VEVENT
END:VCALENDAR";

        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks[0].start_at, "2026-06-05T00:00:00");
        assert_eq!(tasks[0].end_at, "2026-06-06T00:00:00");
    }

    #[test]
    fn regression_ical_all_day_without_dtend() {
        // RFC 5545 allows a date-only VEVENT to omit DTEND/DURATION; it is
        // interpreted as a one-day event ending on the following day.
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:all-day-001\r\nDTSTART;VALUE=DATE:20260605\r\nSUMMARY:All-day meeting\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].start_at, "2026-06-05T00:00:00");
        assert_eq!(tasks[0].end_at, "2026-06-06T00:00:00");
    }

    #[test]
    fn parse_with_line_folding() {
        let ical = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20260605T090000Z
DTEND:20260605T110000Z
SUMMARY:Long event name
  continued
DESCRIPTION:Line one
 Line two
UID:fold@example.com
END:VEVENT
END:VCALENDAR";

        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks[0].title, "Long event namecontinued");
        assert_eq!(tasks[0].description, Some("Line oneLine two".to_string()));
    }

    #[test]
    fn missing_dtstart_errors() {
        let ical = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
DTEND:20260605T110000Z
SUMMARY:No start
END:VEVENT
END:VCALENDAR";

        let result = parse_ical(ical);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_input_returns_empty_vec() {
        let result = parse_ical("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn parse_only_whitespace_returns_empty() {
        let ical = "  \r\n  \r\n";
        let result = parse_ical(ical);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn parse_missing_end_vevent_drops_that_vevent() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:test\r\nDTSTART:20250101T000000Z\r\nDTEND:20250101T010000Z\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_missing_end_vcalendar_still_parses_events() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:test\r\nDTSTART:20250101T000000Z\r\nDTEND:20250101T010000Z\r\nEND:VEVENT\r\n";
        // Parser is lenient: END:VCALENDAR not required
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn parse_malformed_property_no_colon_skipped_dtstart() {
        // Line without colon is skipped, then DTSTART is missing → error
        let ical =
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART20250101\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert!(parse_ical(ical).is_err());
    }

    #[test]
    fn skip_non_vevent_components() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTODO\r\nUID:todo1\r\nDTSTART:20250101T000000Z\r\nEND:VTODO\r\nBEGIN:VEVENT\r\nUID:event1\r\nDTSTART:20250101T120000Z\r\nDTEND:20250101T130000Z\r\nSUMMARY:Test\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].uid.as_deref(), Some("event1"));
    }

    #[test]
    fn parse_with_description() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:desc-test\r\nDTSTART:20250101T090000Z\r\nDTEND:20250101T100000Z\r\nSUMMARY:Meeting\r\nDESCRIPTION:Discuss project details\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].description.as_deref(),
            Some("Discuss project details")
        );
    }

    #[test]
    fn parse_event_without_description() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:no-desc\r\nDTSTART:20250101T100000Z\r\nDTEND:20250101T110000Z\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].description, None);
    }

    #[test]
    fn parse_dtstamp_ignored() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:stamp-test\r\nDTSTAMP:20250101T000000Z\r\nDTSTART:20250101T140000Z\r\nDTEND:20250101T150000Z\r\nSUMMARY:Test\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].uid.as_deref(), Some("stamp-test"));
    }

    #[test]
    fn parse_multiline_folding_with_newline() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:long\r\nDTSTART:20250101T\r\n 000000Z\r\nDTEND:20250101T120000Z\r\nSUMMARY:Test\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
    }

    // ── Property parameters & edge cases ────────────────────────────────

    #[test]
    fn parse_dtstart_with_tzid_parameter() {
        // Property parameters are separated by ';'. The value is after the
        // first ':'. The parser must keep DTSTART as the key and the naive
        // datetime (no Z) as the value.
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:tzid-test\r\nDTSTART;TZID=America/New_York:20250101T090000\r\nDTEND;TZID=America/New_York:20250101T100000\r\nSUMMARY:TZID event\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
        // No 'Z' suffix because the value had no Z.
        assert_eq!(result[0].start_at, "2025-01-01T09:00:00");
        assert_eq!(result[0].end_at, "2025-01-01T10:00:00");
    }

    #[test]
    fn parse_value_with_colon_in_summary() {
        // A colon inside the property VALUE is fine: the split is on the
        // FIRST colon only, so "SUMMARY:Title: subtitle" keeps the rest.
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:colon\r\nDTSTART:20250101T090000Z\r\nDTEND:20250101T100000Z\r\nSUMMARY:Title: subtitle\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "Title: subtitle");
    }

    #[test]
    fn parse_event_without_uid() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nDTSTART:20250101T090000Z\r\nDTEND:20250101T100000Z\r\nSUMMARY:No UID\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].uid, None);
        assert_eq!(result[0].title, "No UID");
    }

    #[test]
    fn parse_event_without_summary_uses_untitled() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:no-sum\r\nDTSTART:20250101T090000Z\r\nDTEND:20250101T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "Untitled");
    }

    #[test]
    fn parse_missing_dtend_errors() {
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:no-end\r\nDTSTART:20250101T090000Z\r\nSUMMARY:No end\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert!(parse_ical(ical).is_err());
    }

    #[test]
    fn parse_invalid_date_errors() {
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:bad\r\nDTSTART:xyz\r\nDTEND:20250101T100000Z\r\nSUMMARY:Bad\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert!(parse_ical(ical).is_err());
    }

    #[test]
    fn parse_tab_continuation_folding() {
        // RFC 5545 allows tab as a continuation-line prefix too.
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:tab\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:Long\n\tcontinued\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "Longcontinued");
    }

    #[test]
    fn parse_nested_vevent_only_innermost_used() {
        // VCALENDAR wrapping is not strictly required; VEVENTs are collected.
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:A\nEND:VEVENT\nBEGIN:VEVENT\nUID:b\nDTSTART:20250102T090000Z\nDTEND:20250102T100000Z\nSUMMARY:B\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].uid.as_deref(), Some("a"));
        assert_eq!(result[1].uid.as_deref(), Some("b"));
    }

    #[test]
    fn parse_short_date_string_errors() {
        // "2025010" is only 7 chars → invalid date.
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:short\nDTSTART:2025010\nDTEND:20250101T100000Z\nSUMMARY:Short\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_ical(ical).is_err());
    }

    // ── RFC 5545 text unescaping (#274) ─────────────────────────────────

    #[test]
    fn unescape_newline_in_summary() {
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:n\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:Line one\\nLine two\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "Line one\nLine two");
    }

    #[test]
    fn unescape_uppercase_n_in_description() {
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:n\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:S\nDESCRIPTION:Para one\\NPara two\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].description.as_deref(), Some("Para one\nPara two"));
    }

    #[test]
    fn unescape_comma_semicolon_backslash() {
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:n\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:A\\, B\\; C\\\\ D\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "A, B; C\\ D");
    }

    #[test]
    fn unescape_preserves_unknown_escape() {
        // Unknown escape sequences keep the backslash (per the fix's contract).
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:n\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:Path C\\ttemp\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "Path C\\ttemp");
    }

    #[test]
    fn unescape_trailing_backslash() {
        let ical = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:n\nDTSTART:20250101T090000Z\nDTEND:20250101T100000Z\nSUMMARY:Trailing\\\nEND:VEVENT\nEND:VCALENDAR\n";
        let result = parse_ical(ical).unwrap();
        assert_eq!(result[0].title, "Trailing\\");
    }

    // ── Explicit UTC offsets (#345) ─────────────────────────────────────

    #[test]
    fn parse_positive_offset() {
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:t\r\nDTSTART:20250101T090000+0900\r\nDTEND:20250101T100000+0900\r\nSUMMARY:Test\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks[0].start_at, "2025-01-01T09:00:00+09:00");
        assert_eq!(tasks[0].end_at, "2025-01-01T10:00:00+09:00");
    }

    #[test]
    fn parse_negative_offset() {
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:t\r\nDTSTART:20250101T090000-0500\r\nDTEND:20250101T100000-0500\r\nSUMMARY:Test\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks[0].start_at, "2025-01-01T09:00:00-05:00");
        assert_eq!(tasks[0].end_at, "2025-01-01T10:00:00-05:00");
    }

    #[test]
    fn parse_offset_zero() {
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:t\r\nDTSTART:20250101T090000+0000\r\nDTEND:20250101T100000+0000\r\nSUMMARY:Test\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks[0].start_at, "2025-01-01T09:00:00+00:00");
    }

    #[test]
    fn regression_ical_duration_instead_of_dtend() {
        // RFC 5545 allows VEVENT to specify DURATION instead of DTEND.
        let ical = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:dur\r\nDTSTART:20260605T090000Z\r\nDURATION:PT2H\r\nSUMMARY:Meeting with duration\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let tasks = parse_ical(ical).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].start_at, "2026-06-05T09:00:00Z");
        assert_eq!(tasks[0].end_at, "2026-06-05T11:00:00Z");
    }

    #[test]
    fn format_ical_date_offset_unit() {
        assert_eq!(
            format_ical_date("20250101T090000+0900").unwrap(),
            "2025-01-01T09:00:00+09:00"
        );
        assert_eq!(
            format_ical_date("20250101T090000-0530").unwrap(),
            "2025-01-01T09:00:00-05:30"
        );
        // UTC and naive still work.
        assert_eq!(
            format_ical_date("20250101T090000Z").unwrap(),
            "2025-01-01T09:00:00Z"
        );
        assert_eq!(
            format_ical_date("20250101T090000").unwrap(),
            "2025-01-01T09:00:00"
        );
        assert_eq!(format_ical_date("20250101").unwrap(), "2025-01-01T00:00:00");
    }
}
