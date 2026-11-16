import type { MediaAsset } from './lib/backend';

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

/** m:ss, or h:mm:ss from one hour on. */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${seconds}` : `${minutes}:${seconds}`;
}

export type Orientation = 'landscape' | 'portrait' | 'square';

export function orientationOf(width: number | null, height: number | null): Orientation | null {
  if (!width || !height) return null;
  return width > height ? 'landscape' : width < height ? 'portrait' : 'square';
}

export type MediaFilters = { kind: string; tag: string; query: string };

/** Kind, tag (case-insensitive) and text search over name, description and tags. */
export function filterAssets(assets: MediaAsset[], filters: MediaFilters): MediaAsset[] {
  const query = filters.query.trim().toLowerCase();
  const tag = filters.tag.toLowerCase();
  return assets.filter((asset) => (filters.kind === 'all' || asset.kind === filters.kind)
    && (!tag || asset.tags.some((item) => item.toLowerCase() === tag))
    && (!query || `${asset.filename} ${asset.description} ${asset.tags.join(' ')}`.toLowerCase().includes(query)));
}

export function allTags(assets: MediaAsset[]): string[] {
  const seen = new Map<string, string>();
  for (const asset of assets) for (const tag of asset.tags) if (!seen.has(tag.toLowerCase())) seen.set(tag.toLowerCase(), tag);
  return [...seen.values()].sort((left, right) => left.toLowerCase().localeCompare(right.toLowerCase()));
}
