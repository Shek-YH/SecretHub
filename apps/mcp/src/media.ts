import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { z } from 'zod';

// Media library (design M4). The MCP server only reads the media_assets table; adding a file
// is queued as media-*.json and copied in by Desktop after the user confirms.
export const MEDIA_KINDS = ['image', 'video', 'audio', 'text', 'other'] as const;
export const ORIENTATIONS = ['landscape', 'portrait', 'square'] as const;
export const MAX_MEDIA_BYTES = 2 * 1024 * 1024 * 1024;
export const MAX_MEDIA_TAGS = 20;
const MAX_DESCRIPTION = 2000;

export type Orientation = (typeof ORIENTATIONS)[number];
export type MediaItem = {
  id: string; kind: string; filename: string; path: string; mime: string; sizeBytes: number;
  width: number | null; height: number | null; durationMs: number | null;
  orientation: Orientation | null; description: string; tags: string[];
};

export const orientationOf = (width: number | null, height: number | null): Orientation | null =>
  width && height ? (width > height ? 'landscape' : width < height ? 'portrait' : 'square') : null;

const nullableNumber = (value: unknown) => (value === null || value === undefined ? null : Number(value));
const parseTags = (value: unknown): string[] => { try { const parsed = JSON.parse(String(value)); return Array.isArray(parsed) ? parsed.map(String) : []; } catch { return []; } };

function rowToItem(row: Record<string, unknown>): MediaItem {
  const width = nullableNumber(row.width);
  const height = nullableNumber(row.height);
  return {
    id: String(row.id), kind: String(row.kind), filename: String(row.filename), path: String(row.stored_path), mime: String(row.mime ?? ''),
    sizeBytes: Number(row.size_bytes), width, height, durationMs: nullableNumber(row.duration_ms), orientation: orientationOf(width, height),
    description: String(row.description ?? ''), tags: parseTags(row.tags_json),
  };
}

// Newest first; tolerates vaults created before the media library existed.
export function readMedia(dbPath: string): MediaItem[] {
  let db: DatabaseSync;
  try { db = new DatabaseSync(dbPath, { readOnly: true }); } catch { return []; }
  try {
    return (db.prepare('SELECT id, kind, filename, stored_path, mime, size_bytes, width, height, duration_ms, description, tags_json FROM media_assets ORDER BY created_at DESC, id').all() as Array<Record<string, unknown>>).map(rowToItem);
  } catch { return []; } finally { db.close(); }
}

export type MediaSearch = { kind?: string; tags?: string[]; query?: string; orientation?: Orientation; minDurationMs?: number; maxDurationMs?: number; limit?: number };

export function searchMedia(items: MediaItem[], input: MediaSearch): { total: number; items: MediaItem[] } {
  const wanted = (input.tags ?? []).map((tag) => tag.toLowerCase());
  const query = input.query?.trim().toLowerCase();
  const matches = items.filter((item) => {
    if (input.kind && item.kind !== input.kind) return false;
    if (input.orientation && item.orientation !== input.orientation) return false;
    if (wanted.length > 0) { const have = item.tags.map((tag) => tag.toLowerCase()); if (!wanted.every((tag) => have.includes(tag))) return false; }
    if (query && !`${item.filename} ${item.description} ${item.tags.join(' ')}`.toLowerCase().includes(query)) return false;
    // A duration bound excludes assets whose duration is unknown.
    if (input.minDurationMs !== undefined && !(item.durationMs !== null && item.durationMs >= input.minDurationMs)) return false;
    if (input.maxDurationMs !== undefined && !(item.durationMs !== null && item.durationMs <= input.maxDurationMs)) return false;
    return true;
  });
  return { total: matches.length, items: matches.slice(0, input.limit ?? 20) };
}

export const getMedia = (items: MediaItem[], id: string): MediaItem | undefined => items.find((item) => item.id === id);

// ---- queueing ---------------------------------------------------------------

const noControl = (value: string) => !/[\u0000-\u001f\u007f]/.test(value);
export const mediaTagSchema = z.string().trim().min(1).max(32).refine((value) => !value.includes(',') && noControl(value), 'must not contain commas or control characters');
export const mediaAddSchema = {
  path: z.string().min(1).max(1024).refine((value) => !value.includes('\0'), 'must not contain NUL'),
  description: z.string().max(MAX_DESCRIPTION).optional(),
  tags: z.array(mediaTagSchema).max(MAX_MEDIA_TAGS).optional(),
};

export function isValidMediaRequestId(id: string): boolean { return id.length <= 128 && /^media-[A-Za-z0-9_-]+$/.test(id); }

// Returns a problem description, or undefined when the file can be queued. Desktop checks again on accept.
export function precheckMediaAdd(path: string, maxBytes = MAX_MEDIA_BYTES): string | undefined {
  if (!isAbsolute(path)) return 'path must be an absolute path.';
  let stats: ReturnType<typeof statSync>;
  try { stats = statSync(path); } catch { return 'The file does not exist or cannot be read.'; }
  if (!stats.isFile()) return 'The path is not a regular file (directories are not accepted).';
  if (stats.size === 0) return 'The file is empty.';
  if (stats.size > maxBytes) return `The file is larger than the ${maxBytes} byte limit.`;
  return undefined;
}

export function queueMediaAdd(directory: string, input: { path: string; description?: string; tags?: string[] }): string {
  const requestId = `media-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = { version: 1, requestId, kind: 'add_media', path: input.path, description: input.description ?? '', tags: [...new Set(input.tags ?? [])], createdAt: new Date().toISOString() };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

export type MediaAddStatus =
  | { status: 'pending' | 'declined' | 'not_found'; requestId: string }
  | { status: 'added'; requestId: string; assetId: string; duplicate: boolean }
  | { status: 'failed'; requestId: string; error: { code: string } };

export function getMediaAddStatus(requestId: string, pendingDirectory: string, resultDirectory: string): MediaAddStatus {
  // Desktop writes the result before removing the pending file, so the result wins.
  const resultPath = join(resultDirectory, `${requestId}.json`);
  if (existsSync(resultPath)) {
    let result: Record<string, unknown>;
    try { result = JSON.parse(readFileSync(resultPath, 'utf8')); } catch { return { status: 'failed', requestId, error: { code: 'unreadable_result' } }; }
    if (result.status === 'declined') return { status: 'declined', requestId };
    if (result.status === 'added' && typeof result.assetId === 'string') return { status: 'added', requestId, assetId: result.assetId.slice(0, 200), duplicate: result.duplicate === true };
    const code = typeof (result.error as { code?: unknown } | undefined)?.code === 'string' ? String((result.error as { code: string }).code).slice(0, 64) : 'unknown';
    return { status: 'failed', requestId, error: { code } };
  }
  if (existsSync(join(pendingDirectory, `${requestId}.json`))) return { status: 'pending', requestId };
  return { status: 'not_found', requestId };
}
