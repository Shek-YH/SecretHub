export type RelativeTimeLabels = { justNow: string; minutesAgo: string; hoursAgo: string; daysAgo: string };

/** Relative label for a unix-seconds timestamp; older than 30 days falls back to a local YYYY-MM-DD date. */
export function formatRelativeTime(seconds: number | undefined, labels: RelativeTimeLabels, nowMs: number = Date.now()): string {
  if (!seconds || seconds <= 0) return '';
  const elapsed = Math.max(0, Math.floor(nowMs / 1000) - seconds);
  const fill = (template: string, count: number) => template.replace('{n}', String(count));
  if (elapsed < 60) return labels.justNow;
  if (elapsed < 3600) return fill(labels.minutesAgo, Math.floor(elapsed / 60));
  if (elapsed < 86_400) return fill(labels.hoursAgo, Math.floor(elapsed / 3600));
  const days = Math.floor(elapsed / 86_400);
  if (days <= 30) return fill(labels.daysAgo, days);
  const date = new Date(seconds * 1000);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}
