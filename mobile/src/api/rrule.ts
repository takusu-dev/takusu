// rrule — recurrence rule types, (de)serialization, and human-readable summary.
//
// The server stores `recurrence` as a JSON-serialized `takusu_habit::RecurrenceRule`
// (see crates/takusu-habit/src/rule.rs). This module mirrors that struct on the
// client side and provides helpers for the RRULE builder UI.

export type Frequency = 'daily' | 'weekly' | 'monthly' | 'yearly';

export type Weekday = 'mon' | 'tue' | 'wed' | 'thu' | 'fri' | 'sat' | 'sun';

export interface NWeekday {
  n: number | null; // null = every occurrence; 1..5 or -1..-5 = nth
  weekday: Weekday;
}

export interface RecurrenceRule {
  freq: Frequency;
  interval: number;
  by_day: NWeekday[];
  by_month: number[]; // 1..12
  by_month_day: number[]; // 1..31, negative = from end
  count: number | null;
  exdates: string[]; // "YYYY-MM-DD"
}

export const WEEKDAYS: Weekday[] = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];

export const WEEKDAY_LABELS: Record<Weekday, string> = {
  mon: '月',
  tue: '火',
  wed: '水',
  thu: '木',
  fri: '金',
  sat: '土',
  sun: '日',
};

export const MONTHS = Array.from({ length: 12 }, (_, i) => i + 1);

export const FREQUENCIES: Frequency[] = ['daily', 'weekly', 'monthly', 'yearly'];

export const FREQUENCY_LABELS: Record<Frequency, string> = {
  daily: '毎日',
  weekly: '毎週',
  monthly: '毎月',
  yearly: '毎年',
};

export function defaultRule(): RecurrenceRule {
  return {
    freq: 'daily',
    interval: 1,
    by_day: [],
    by_month: [],
    by_month_day: [],
    count: null,
    exdates: [],
  };
}

/** Parse a JSON recurrence string into a RecurrenceRule, falling back to default. */
export function parseRule(s: string): RecurrenceRule {
  if (!s) return defaultRule();
  try {
    const obj = JSON.parse(s);
    return {
      freq: (obj.freq as Frequency) ?? 'daily',
      interval: obj.interval ?? 1,
      by_day: obj.by_day ?? [],
      by_month: obj.by_month ?? [],
      by_month_day: obj.by_month_day ?? [],
      count: obj.count ?? null,
      exdates: obj.exdates ?? [],
    };
  } catch {
    return defaultRule();
  }
}

/** Serialize a RecurrenceRule to the JSON string the server expects. */
export function serializeRule(r: RecurrenceRule): string {
  return JSON.stringify({
    freq: r.freq,
    interval: r.interval,
    by_day: r.by_day,
    by_month: r.by_month,
    by_month_day: r.by_month_day,
    count: r.count,
    exdates: r.exdates,
  });
}

const MONTH_LABELS = [
  '1月', '2月', '3月', '4月', '5月', '6月',
  '7月', '8月', '9月', '10月', '11月', '12月',
];

/** Human-readable Japanese summary of a recurrence rule. */
export function summarizeRule(r: RecurrenceRule): string {
  const unit = r.freq === 'daily' ? '日' : r.freq === 'weekly' ? '週' : r.freq === 'monthly' ? '月' : '年';
  const base = r.interval === 1 ? FREQUENCY_LABELS[r.freq] : `${r.interval}${unit}ごと`;

  const parts: string[] = [base];

  if (r.freq === 'weekly' && r.by_day.length > 0) {
    const days = r.by_day
      .filter((d) => d.n === null)
      .map((d) => WEEKDAY_LABELS[d.weekday]);
    if (days.length > 0) parts.push(days.join('・'));
  }

  if (r.freq === 'monthly' && r.by_month_day.length > 0) {
    parts.push(r.by_month_day.map((d) => `${d}日`).join('・'));
  }

  if (r.freq === 'yearly') {
    if (r.by_month.length > 0) {
      parts.push(r.by_month.map((m) => MONTH_LABELS[m - 1]).join('・'));
    }
    if (r.by_month_day.length > 0) {
      parts.push(r.by_month_day.map((d) => `${d}日`).join('・'));
    }
  }

  if (r.count !== null) parts.push(`× ${r.count}回`);

  return parts.join(' ');
}
