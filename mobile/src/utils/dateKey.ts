// Convert UTC ISO timestamps to YYYY-MM-DD date keys in a configured
// timezone. The server uses the same timezone for `sync_habit_tasks` and
// schedule date keys, so clients must match it to keep grouping consistent.
// Falls back to the device timezone when no `tz` is provided or when the
// supplied timezone is invalid.

const formatterCache = new Map<string, Intl.DateTimeFormat>();

function getFormatter(tz?: string): Intl.DateTimeFormat {
  const key = tz ?? '';
  let fmt = formatterCache.get(key);
  if (!fmt) {
    try {
      fmt = new Intl.DateTimeFormat('en-CA', {
        timeZone: tz || undefined,
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
      });
    } catch {
      // Invalid timezone string — fall back to the device timezone and cache
      // that fallback so repeated invalid calls don't keep throwing.
      fmt = new Intl.DateTimeFormat('en-CA', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
      });
    }
    formatterCache.set(key, fmt);
  }
  return fmt;
}

export function dateKey(iso: string, tz?: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso.slice(0, 10);
  return getFormatter(tz).format(d);
}

export function todayDateKey(tz?: string): string {
  return dateKey(new Date().toISOString(), tz);
}
