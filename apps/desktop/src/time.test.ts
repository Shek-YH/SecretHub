import { describe, expect, it } from 'vitest';
import { getTranslation } from './i18n/translations';
import { formatRelativeTime } from './time';

describe('formatRelativeTime', () => {
  const now = Date.UTC(2026, 10, 9, 12, 0, 0);
  const at = (secondsAgo: number) => Math.floor(now / 1000) - secondsAgo;
  const zh = getTranslation('zh-CN').time;
  const en = getTranslation('en-US').time;

  it('uses coarse relative units', () => {
    expect(formatRelativeTime(at(5), zh, now)).toBe('刚刚');
    expect(formatRelativeTime(at(5 * 60), zh, now)).toBe('5 分钟前');
    expect(formatRelativeTime(at(3 * 3600), zh, now)).toBe('3 小时前');
    expect(formatRelativeTime(at(2 * 86_400), zh, now)).toBe('2 天前');
    expect(formatRelativeTime(at(5 * 60), en, now)).toBe('5 min ago');
    expect(formatRelativeTime(at(5), en, now)).toBe('Just now');
  });

  it('falls back to a date after 30 days and to empty for unknown times', () => {
    expect(formatRelativeTime(at(30 * 86_400), zh, now)).toBe('30 天前');
    expect(formatRelativeTime(at(40 * 86_400), zh, now)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(formatRelativeTime(0, zh, now)).toBe('');
    expect(formatRelativeTime(undefined, zh, now)).toBe('');
  });
});
