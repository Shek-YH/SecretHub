import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { z } from 'zod';

// Import requests (design section 3.1): queued as import-*.json. The file holds absolute paths
// only. The AI never reads the files at those paths; Desktop scans them locally after the user
// reviews the result. Results hold counts and item names, never values.
export const MAX_IMPORT_PATHS = 20;

export const importPathSchema = z.string().trim().min(1).max(400);

export function isValidImportRequestId(id: string): boolean { return id.length <= 128 && /^import-[A-Za-z0-9_-]+$/.test(id); }

// Returns a problem description, or undefined when the request can be queued. Only the shape of
// the paths is checked here; nothing is opened or read.
export function precheckImport(paths: string[]): string | undefined {
  if (paths.length === 0) return 'Provide at least one file or folder path.';
  if (paths.length > MAX_IMPORT_PATHS) return `At most ${MAX_IMPORT_PATHS} paths are allowed.`;
  for (const path of paths) {
    if (/[\u0000-\u001f\u007f]/.test(path)) return 'Paths must not contain control characters.';
    if (!isAbsolute(path)) return 'Every path must be absolute.';
  }
  return undefined;
}

export function queueImport(directory: string, paths: string[]): string {
  const requestId = `import-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = { version: 1, requestId, kind: 'import', paths: [...new Set(paths)], createdAt: new Date().toISOString() };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

export type ImportNames = { added: string[]; updated: string[]; skipped: string[] };
export type ImportStatus =
  | { status: 'pending' | 'declined' | 'not_found'; requestId: string }
  | { status: 'done'; requestId: string; added: number; updated: number; skipped: number; names: ImportNames }
  | { status: 'failed'; requestId: string; error: { code: string } };

const count = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0);
const names = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string').slice(0, 200).map((item) => item.slice(0, 200)) : []);

export function getImportStatus(requestId: string, pendingDirectory: string, resultDirectory: string): ImportStatus {
  // Desktop writes the result before removing the pending file, so the result wins.
  const resultPath = join(resultDirectory, `${requestId}.json`);
  if (existsSync(resultPath)) {
    let result: Record<string, unknown>;
    try { result = JSON.parse(readFileSync(resultPath, 'utf8')); } catch { return { status: 'failed', requestId, error: { code: 'unreadable_result' } }; }
    // Only whitelisted fields are copied out of the file.
    if (result.status === 'declined') return { status: 'declined', requestId };
    if (result.status === 'done') {
      const listed = (result.names ?? {}) as Record<string, unknown>;
      return { status: 'done', requestId, added: count(result.added), updated: count(result.updated), skipped: count(result.skipped), names: { added: names(listed.added), updated: names(listed.updated), skipped: names(listed.skipped) } };
    }
    const code = typeof (result.error as { code?: unknown } | undefined)?.code === 'string' ? String((result.error as { code: string }).code).slice(0, 64) : 'unknown';
    return { status: 'failed', requestId, error: { code } };
  }
  if (existsSync(join(pendingDirectory, `${requestId}.json`))) return { status: 'pending', requestId };
  return { status: 'not_found', requestId };
}
