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
  by_month_day: number[]; // 1..31, negative = from end (-1 = last day)
  count: number | null;
  exdates: string[]; // "YYYY-MM-DD"
}

export const WEEKDAYS: Weekday[] = [
  'sun',
  'mon',
  'tue',
  'wed',
  'thu',
  'fri',
  'sat',
];

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

export const FREQUENCIES: Frequency[] = [
  'daily',
  'weekly',
  'monthly',
  'yearly',
];

export const FREQUENCY_LABELS: Record<Frequency, string> = {
  daily: '毎日',
  weekly: '毎週',
  monthly: '毎月',
  yearly: '毎年',
};

// Ordinal labels for nth-weekday: 1..5 = 第1〜第5、-1 = 最終
export const NTH_LABELS: Record<number, string> = {
  1: '第1',
  2: '第2',
  3: '第3',
  4: '第4',
  5: '第5',
  [-1]: '最終',
  [-2]: '最終-1',
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

/**
 * Validate a RecurrenceRule and return an error message, or null if valid.
 * Used for manual JSON input validation.
 */
export function validateRule(r: RecurrenceRule): string | null {
  const validFreqs: Frequency[] = ['daily', 'weekly', 'monthly', 'yearly'];
  if (!validFreqs.includes(r.freq)) return `不正な freq: ${r.freq}`;
  if (!Number.isInteger(r.interval) || r.interval < 1)
    return 'interval は 1 以上の整数が必要です';
  if (r.count !== null && (!Number.isInteger(r.count) || r.count < 1))
    return 'count は 1 以上の整数が必要です';
  for (const m of r.by_month) {
    if (!Number.isInteger(m) || m < 1 || m > 12)
      return `by_month の値が不正です: ${m}`;
  }
  for (const d of r.by_month_day) {
    if (!Number.isInteger(d) || d === 0 || d > 31 || d < -31)
      return `by_month_day の値が不正です: ${d}`;
  }
  for (const nw of r.by_day) {
    const validWds: Weekday[] = WEEKDAYS;
    if (!validWds.includes(nw.weekday))
      return `by_day の weekday が不正です: ${nw.weekday}`;
    if (
      nw.n !== null &&
      (!Number.isInteger(nw.n) || nw.n === 0 || nw.n > 5 || nw.n < -5)
    )
      return `by_day の n が不正です: ${nw.n}`;
  }
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;
  for (const ex of r.exdates) {
    if (!dateRe.test(ex))
      return `exdates の日付形式が不正です: ${ex} (YYYY-MM-DD が必要)`;
  }
  return null;
}

const MONTH_LABELS = [
  '1月',
  '2月',
  '3月',
  '4月',
  '5月',
  '6月',
  '7月',
  '8月',
  '9月',
  '10月',
  '11月',
  '12月',
];

/** Format a single NWeekday in Japanese (e.g. "第2月曜", "最終金曜", "水曜"). */
function formatNWeekday(nw: NWeekday): string {
  const wd = WEEKDAY_LABELS[nw.weekday] + '曜';
  if (nw.n === null) return wd;
  const prefix =
    NTH_LABELS[nw.n] ??
    `${nw.n > 0 ? `第${nw.n}` : `最終${Math.abs(nw.n) - 1 > 0 ? `-${Math.abs(nw.n) - 1}` : ''}`}`;
  return `${prefix}${wd}`;
}

/** Format a single by_month_day value (e.g. 15 → "15日", -1 → "月末"). */
function formatMonthDay(d: number): string {
  if (d === -1) return '月末';
  if (d < 0) return `月末から${Math.abs(d)}日目`;
  return `${d}日`;
}

/** Human-readable Japanese summary of a recurrence rule. */
export function summarizeRule(r: RecurrenceRule): string {
  const unit =
    r.freq === 'daily'
      ? '日'
      : r.freq === 'weekly'
        ? '週'
        : r.freq === 'monthly'
          ? '月'
          : '年';
  const base =
    r.interval === 1 ? FREQUENCY_LABELS[r.freq] : `${r.interval}${unit}ごと`;

  const parts: string[] = [base];

  // by_day: show for all frequencies, including nth weekdays
  if (r.by_day.length > 0) {
    const dayLabels = r.by_day.map(formatNWeekday);
    parts.push(dayLabels.join('・'));
  }

  // by_month: show for all frequencies
  if (r.by_month.length > 0) {
    parts.push(
      r.by_month
        .map((m) => {
          if (m >= 1 && m <= 12) return MONTH_LABELS[m - 1];
          return `?${m}`;
        })
        .join('・'),
    );
  }

  // by_month_day: show for monthly/yearly (including negative = from end)
  if (r.by_month_day.length > 0) {
    parts.push(r.by_month_day.map(formatMonthDay).join('・'));
  }

  if (r.count !== null) parts.push(`× ${r.count}回`);

  if (r.exdates.length > 0) parts.push(`除外 ${r.exdates.length}日`);

  return parts.join(' ');
}
