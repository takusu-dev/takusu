//! # takusu-ical — iCalendar parser
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

fn format_ical_date(raw: &str) -> Result<String, IcalError> {
    if raw.len() < 8 {
        return Err(IcalError::InvalidDate(raw.to_string()));
    }

    let s = raw.strip_suffix('Z').unwrap_or(raw);

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
                if raw.ends_with('Z') { "Z" } else { "" }
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
            if raw.ends_with('Z') { "Z" } else { "" }
        ));
    }

    if s.len() >= 8 {
        return Ok(format!(
            "{}-{}-{}T00:00:00{}",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            if raw.ends_with('Z') { "Z" } else { "" }
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
                        .cloned()
                        .unwrap_or_else(|| "Untitled".to_string());
                    let description = properties.get("DESCRIPTION").cloned();
                    let uid = properties.get("UID").cloned();

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
}
