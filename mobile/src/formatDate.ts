// Shared date formatting utility.
// Prepends a relative day label (今日, 明日, 明後日, etc.) for readability.

export function formatDate(d: Date | null): string {
  if (!d) return '未設定';
  const dateStr = `${d.getFullYear()}/${(d.getMonth() + 1)
    .toString()
    .padStart(2, '0')}/${d.getDate().toString().padStart(2, '0')}`;
  const timeStr = `${d.getHours().toString().padStart(2, '0')}:${d
    .getMinutes()
    .toString()
    .padStart(2, '0')}`;
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const target = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const dayDiff = Math.round(
    (target.getTime() - today.getTime()) / (1000 * 60 * 60 * 24),
  );
  let label = '';
  if (dayDiff === 0) label = '今日 ';
  else if (dayDiff === 1) label = '明日 ';
  else if (dayDiff === 2) label = '明後日 ';
  else if (dayDiff === -1) label = '昨日 ';
  else if (dayDiff === -2) label = '一昨日 ';
  else if (dayDiff > 0 && dayDiff <= 6) label = `${dayDiff}日後 `;
  return `${label}${dateStr} ${timeStr}`;
}
