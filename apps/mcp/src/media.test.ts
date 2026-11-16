import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { getMedia, getMediaAddStatus, isValidMediaRequestId, orientationOf, precheckMediaAdd, queueMediaAdd, readMedia, searchMedia, type MediaItem } from './media.js';
import { isValidSyncRequestId } from './sync.js';
import { isValidCaptureRequestId } from './capture.js';
import { isValidRequestId } from './plan.js';

const item = (id: string, over: Partial<MediaItem> = {}): MediaItem => ({ id, kind: 'image', filename: `${id}.png`, path: `C:\\media\\${id}.png`, mime: 'image/png', sizeBytes: 1, width: 100, height: 100, durationMs: null, orientation: 'square', description: '', tags: [], ...over });
const items = [
  item('a', { kind: 'image', width: 1920, height: 1080, orientation: 'landscape', tags: ['Cat', 'pets'], description: 'sleeping cat' }),
  item('b', { kind: 'video', filename: 'clip.mp4', width: 1080, height: 1920, orientation: 'portrait', durationMs: 5000, tags: ['pets'] }),
  item('c', { kind: 'video', filename: 'long.mp4', durationMs: 60000, orientation: 'landscape', tags: [] }),
  item('d', { kind: 'audio', filename: 'voice.mp3', width: null, height: null, orientation: null, durationMs: 1500, description: 'greeting' }),
  item('e', { kind: 'text', filename: 'prompt.txt', width: null, height: null, orientation: null }),
];
const ids = (input: Parameters<typeof searchMedia>[1]) => searchMedia(items, input).items.map((entry) => entry.id);

describe('media search', () => {
  it('derives orientation from width and height', () => {
    expect(orientationOf(2, 1)).toBe('landscape');
    expect(orientationOf(1, 2)).toBe('portrait');
    expect(orientationOf(3, 3)).toBe('square');
    expect(orientationOf(null, 5)).toBeNull();
    expect(orientationOf(0, 0)).toBeNull();
  });

  it('filters by kind, orientation, tags (all, case-insensitive) and text', () => {
    expect(ids({ kind: 'video' })).toEqual(['b', 'c']);
    expect(ids({ orientation: 'landscape' })).toEqual(['a', 'c']);
    expect(ids({ tags: ['PETS'] })).toEqual(['a', 'b']);
    expect(ids({ tags: ['pets', 'cat'] })).toEqual(['a']);
    expect(ids({ query: 'SLEEPING' })).toEqual(['a']);
    expect(ids({ query: 'voice' })).toEqual(['d']);
    expect(ids({ query: 'cat' })).toEqual(['a']);
    expect(ids({ kind: 'video', tags: ['pets'], orientation: 'portrait' })).toEqual(['b']);
  });

  it('filters by duration bounds and drops assets with unknown duration when a bound is set', () => {
    expect(ids({ minDurationMs: 2000 })).toEqual(['b', 'c']);
    expect(ids({ maxDurationMs: 5000 })).toEqual(['b', 'd']);
    expect(ids({ minDurationMs: 1000, maxDurationMs: 6000 })).toEqual(['b', 'd']);
    expect(ids({})).toHaveLength(5);
  });

  it('keeps the library order and applies limit after filtering, reporting the total', () => {
    const found = searchMedia(items, { limit: 2 });
    expect(found.items.map((entry) => entry.id)).toEqual(['a', 'b']);
    expect(found.total).toBe(5);
    expect(searchMedia(items, { kind: 'video', limit: 1 })).toEqual({ total: 2, items: [items[1]] });
    expect(getMedia(items, 'c')?.filename).toBe('long.mp4');
    expect(getMedia(items, 'zz')).toBeUndefined();
  });
});

describe('readMedia (real SQLite)', () => {
  it('reads newest first, maps columns, tolerates null dimensions and a missing table', () => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-media-'));
    const path = join(directory, 'vault.sqlite');
    const db = new DatabaseSync(path);
    try {
      expect(readMedia(path)).toEqual([]);
      db.exec(`CREATE TABLE media_assets (id TEXT PRIMARY KEY, kind TEXT, filename TEXT, stored_path TEXT, sha256 TEXT, size_bytes INTEGER, mime TEXT, width INTEGER, height INTEGER, duration_ms INTEGER, description TEXT, tags_json TEXT, source TEXT, created_at INTEGER);
        INSERT INTO media_assets VALUES ('old', 'audio', 'v.mp3', 'C:/m/old.mp3', 'aa', 10, 'audio/mpeg', NULL, NULL, 1500, '', '[]', 'user', 1);
        INSERT INTO media_assets VALUES ('new', 'image', 'p.png', 'C:/m/new.png', 'bb', 20, 'image/png', 640, 480, NULL, 'pic', '["x","y"]', 'ai', 2);
        INSERT INTO media_assets VALUES ('broken', 'text', 't.txt', 'C:/m/t.txt', 'cc', 5, 'text/plain', NULL, NULL, NULL, '', 'not json', 'user', 0);`);
      db.close();
      const media = readMedia(path);
      expect(media.map((entry) => entry.id)).toEqual(['new', 'old', 'broken']);
      expect(media[0]).toEqual({ id: 'new', kind: 'image', filename: 'p.png', path: 'C:/m/new.png', mime: 'image/png', sizeBytes: 20, width: 640, height: 480, durationMs: null, orientation: 'landscape', description: 'pic', tags: ['x', 'y'] });
      expect(media[1]).toEqual(expect.objectContaining({ width: null, durationMs: 1500, orientation: null }));
      expect(media[2].tags).toEqual([]);
      expect(JSON.stringify(media)).not.toContain('sha');
    } finally { try { db.close(); } catch { /* already closed */ } rmSync(directory, { recursive: true, force: true }); }
    expect(readMedia(join(tmpdir(), 'secrethub-no-such-dir', 'vault.sqlite'))).toEqual([]);
  });
});

describe('media add queue', () => {
  const withDirs = (check: (root: string, pending: string, results: string) => void) => {
    const root = mkdtempSync(join(tmpdir(), 'secrethub-mediaq-'));
    try { check(root, join(root, 'pending'), join(root, 'results')); } finally { rmSync(root, { recursive: true, force: true }); }
  };

  it('accepts only absolute paths of existing, non-empty regular files within the size limit', () => {
    withDirs((root) => {
      const file = join(root, 'shot.png');
      writeFileSync(file, 'x'.repeat(100));
      expect(precheckMediaAdd(file)).toBeUndefined();
      expect(precheckMediaAdd('relative/shot.png')).toMatch(/absolute/);
      expect(precheckMediaAdd(join(root, 'missing.png'))).toMatch(/does not exist/);
      expect(precheckMediaAdd(root)).toMatch(/not a regular file/);
      const empty = join(root, 'empty.txt');
      writeFileSync(empty, '');
      expect(precheckMediaAdd(empty)).toMatch(/empty/);
      expect(precheckMediaAdd(file, 50)).toMatch(/larger than/);
    });
  });

  it('queues media-*.json with the path and metadata only, and reports pending then the result', () => {
    withDirs((root, pending, results) => {
      const id = queueMediaAdd(pending, { path: join(root, 'shot.png'), description: 'a shot', tags: ['t', 't', 'u'] });
      expect(id).toMatch(/^media-/);
      expect(isValidMediaRequestId(id)).toBe(true);
      const file = JSON.parse(readFileSync(join(pending, `${id}.json`), 'utf8'));
      expect(file).toEqual(expect.objectContaining({ version: 1, requestId: id, kind: 'add_media', description: 'a shot', tags: ['t', 'u'] }));
      expect(getMediaAddStatus(id, pending, results)).toEqual({ status: 'pending', requestId: id });
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, `${id}.json`), JSON.stringify({ requestId: id, status: 'added', assetId: 'asset-1', duplicate: true, stored_path: 'ignored' }));
      expect(getMediaAddStatus(id, pending, results)).toEqual({ status: 'added', requestId: id, assetId: 'asset-1', duplicate: true });
      writeFileSync(join(results, `${id}.json`), JSON.stringify({ requestId: id, status: 'declined' }));
      expect(getMediaAddStatus(id, pending, results)).toEqual({ status: 'declined', requestId: id });
      writeFileSync(join(results, `${id}.json`), JSON.stringify({ requestId: id, status: 'failed', error: { code: 'too_large', message: 'x' } }));
      expect(getMediaAddStatus(id, pending, results)).toEqual({ status: 'failed', requestId: id, error: { code: 'too_large' } });
      expect(getMediaAddStatus('media-none', pending, results)).toEqual({ status: 'not_found', requestId: 'media-none' });
    });
  });

  it('uses its own id prefix', () => {
    for (const bad of ['', 'media-', 'media-../x', 'sync-1', 'plan-1', 'capture-1', `media-${'a'.repeat(130)}`]) expect(isValidMediaRequestId(bad)).toBe(false);
    for (const other of [isValidSyncRequestId, isValidCaptureRequestId, isValidRequestId]) expect(other('media-1')).toBe(false);
  });
});
