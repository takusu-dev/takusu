//! Human-readable Japanese summary of a `RecurrenceRule`.
//!
//! Mirrors `mobile/src/api/rrule.ts::summarizeRule` so the CLI and the mobile
//! app show the same string for the same rule.

use crate::rule::{Frequency, NWeekday, RecurrenceRule, Weekday};

const MONTH_LABELS: [&str; 12] = [
    "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
];

fn weekday_label(wd: Weekday) -> &'static str {
    match wd {
        Weekday::Mon => "月",
        Weekday::Tue => "火",
        Weekday::Wed => "水",
        Weekday::Thu => "木",
        Weekday::Fri => "金",
        Weekday::Sat => "土",
        Weekday::Sun => "日",
    }
}

fn nth_label(n: i8) -> String {
    match n {
        1 => "第1".to_string(),
        2 => "第2".to_string(),
        3 => "第3".to_string(),
        4 => "第4".to_string(),
        5 => "第5".to_string(),
        -1 => "最終".to_string(),
        n if n < 0 => {
            let abs = (n as i32).abs();
            if abs - 1 > 0 {
                format!("最終-{}", abs - 1)
            } else {
                "最終".to_string()
            }
        }
        n => format!("第{}", n),
    }
}

fn format_nweekday(nw: &NWeekday) -> String {
    let wd = format!("{}曜", weekday_label(nw.weekday));
    match nw.n {
        None => wd,
        Some(n) => format!("{}{}", nth_label(n), wd),
    }
}

fn format_month_day(d: i8) -> String {
    if d == -1 {
        return "月末".to_string();
    }
    if d < 0 {
        return format!("月末から{}日目", (d as i32).abs());
    }
    format!("{}日", d)
}

/// Human-readable Japanese summary of a recurrence rule.
///
/// Example: `{"freq":"weekly","by_day":[Mon,Wed,Fri]}` → "毎週 月曜・水曜・金曜"
pub fn summarize(r: &RecurrenceRule) -> String {
    let unit = match r.freq {
        Frequency::Daily => "日",
        Frequency::Weekly => "週",
        Frequency::Monthly => "月",
        Frequency::Yearly => "年",
    };
    let base = if r.interval == 1 {
        frequency_label(r.freq).to_string()
    } else {
        format!("{}{}ごと", r.interval, unit)
    };

    let mut parts: Vec<String> = vec![base];

    if !r.by_day.is_empty() {
        let labels: Vec<String> = r.by_day.iter().map(format_nweekday).collect();
        parts.push(labels.join("・"));
    }

    if !r.by_month.is_empty() {
        let labels: Vec<String> = r
            .by_month
            .iter()
            .map(|&m| {
                // by_month is Vec<i8> deserialized from user-provided JSON with no
                // server-side validation. Out-of-range values (0, 13, negative)
                // would panic on direct indexing, so guard with get().
                if (1..=12).contains(&m) {
                    MONTH_LABELS[(m - 1) as usize].to_string()
                } else {
                    format!("?{}", m)
                }
            })
            .collect();
        parts.push(labels.join("・"));
    }

    if !r.by_month_day.is_empty() {
        let labels: Vec<String> = r
            .by_month_day
            .iter()
            .map(|&d| format_month_day(d))
            .collect();
        parts.push(labels.join("・"));
    }

    if let Some(count) = r.count {
        parts.push(format!("× {}回", count));
    }

    if !r.exdates.is_empty() {
        parts.push(format!("除外 {}日", r.exdates.len()));
    }

    parts.join(" ")
}

fn frequency_label(f: Frequency) -> &'static str {
    match f {
        Frequency::Daily => "毎日",
        Frequency::Weekly => "毎週",
        Frequency::Monthly => "毎月",
        Frequency::Yearly => "毎年",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn summarize_daily() {
        let r = RecurrenceRule::daily();
        assert_eq!(summarize(&r), "毎日");
    }

    #[test]
    fn summarize_weekly_with_by_day() {
        let r = RecurrenceRule::weekly().by_day(vec![
            NWeekday::every(Weekday::Mon),
            NWeekday::every(Weekday::Wed),
            NWeekday::every(Weekday::Fri),
        ]);
        assert_eq!(summarize(&r), "毎週 月曜・水曜・金曜");
    }

    #[test]
    fn summarize_daily_with_weekday_filter() {
        // The "平日" pattern: daily + by_day=[Mon-Fri]
        let r = RecurrenceRule::daily().by_day(vec![
            NWeekday::every(Weekday::Mon),
            NWeekday::every(Weekday::Tue),
            NWeekday::every(Weekday::Wed),
            NWeekday::every(Weekday::Thu),
            NWeekday::every(Weekday::Fri),
        ]);
        assert_eq!(summarize(&r), "毎日 月曜・火曜・水曜・木曜・金曜");
    }

    #[test]
    fn summarize_interval() {
        let r = RecurrenceRule::daily().interval(3);
        assert_eq!(summarize(&r), "3日ごと");
    }

    #[test]
    fn summarize_monthly_nth_weekday() {
        let r = RecurrenceRule::monthly().by_day(vec![NWeekday::nth(2, Weekday::Fri)]);
        assert_eq!(summarize(&r), "毎月 第2金曜");
    }

    #[test]
    fn summarize_monthly_last_weekday() {
        let r = RecurrenceRule::monthly().by_day(vec![NWeekday::nth(-1, Weekday::Mon)]);
        assert_eq!(summarize(&r), "毎月 最終月曜");
    }

    #[test]
    fn summarize_monthly_by_month_day() {
        let r = RecurrenceRule::monthly().by_month_day(vec![1, 15]);
        assert_eq!(summarize(&r), "毎月 1日・15日");
    }

    #[test]
    fn summarize_monthly_last_day() {
        let r = RecurrenceRule::monthly().by_month_day(vec![-1]);
        assert_eq!(summarize(&r), "毎月 月末");
    }

    #[test]
    fn summarize_with_count() {
        let r = RecurrenceRule::daily().count(5);
        assert_eq!(summarize(&r), "毎日 × 5回");
    }

    #[test]
    fn summarize_with_exdates() {
        let r = RecurrenceRule::daily().exdates(vec![date(2026, 7, 5), date(2026, 7, 6)]);
        assert_eq!(summarize(&r), "毎日 除外 2日");
    }

    #[test]
    fn summarize_yearly_by_month() {
        let r = RecurrenceRule::yearly().by_month(vec![1, 4]);
        assert_eq!(summarize(&r), "毎年 1月・4月");
    }

    #[test]
    fn summarize_by_month_out_of_range_does_not_panic() {
        // by_month values outside 1..=12 should not panic (user-provided JSON
        // is not validated server-side). See Devin Review on PR #243.
        let mut r = RecurrenceRule::yearly();
        r.by_month = vec![0, 13, -1, 6];
        let s = summarize(&r);
        assert!(s.contains("?0"));
        assert!(s.contains("?13"));
        assert!(s.contains("?-1"));
        assert!(s.contains("6月"));
    }
}
