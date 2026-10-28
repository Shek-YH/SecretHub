import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { CatalogSecret } from './catalog.js';

// Model-sync requests (design M2): queued as sync-*.json, executed by Desktop when the vault is unlocked.
// Files hold secret ids only; results hold model ids and counts, never Secret values.
export const MAX_SYNC_SECRETS = 100;

export function isValidSyncRequestId(id: string): boolean { return id.length <= 128 && /^sync-[A-Za-z0-9_-]+$/.test(id); }

// Returns a problem description, or undefined when the request can be queued.
export function precheckSyncRequest(secretIds: string[] | undefined, all: CatalogSecret[]): string | undefined {
  const eligible = (secret: CatalogSecret) => secret.valueType === 'api_key' && !secret.archived && secret.hasValue;
  if (!secretIds) return all.some(eligible) ? undefined : 'No API key with a stored value is available to sync.';
  const known = new Map(all.map((secret) => [secret.id, secret]));
  for (const id of new Set(secretIds)) {
    const secret = known.get(id);
    if (!secret) return `Unknown secret id: ${id}`;
    if (secret.valueType !== 'api_key') return `Secret ${id} is not an API key.`;
    if (secret.archived) return `Secret ${id} is archived.`;
    if (!secret.hasValue) return `Secret ${id} has no stored value.`;
  }
  return undefined;
}

export function queueSyncRequest(directory: string, secretIds?: string[]): string {
  const requestId = `sync-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = { version: 1, requestId, kind: 'sync_models', ...(secretIds ? { secretIds: [...new Set(secretIds)] } : {}), createdAt: new Date().toISOString() };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

export type SyncSecretResult = { secretId: string; providerId: string; modelsSyncStatus: string; validationStatus?: string; count: number; added: string[]; removed: string[]; addedCount: number; removedCount: number };
export type SyncStatus =
  | { status: 'pending' | 'not_found'; requestId: string }
  | { status: 'done'; requestId: string; secrets: SyncSecretResult[] }
  | { status: 'failed'; requestId: string; error: { code: string } };

const text = (value: unknown, max: number) => (typeof value === 'string' ? value.slice(0, max) : '');
const num = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
const models = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string').slice(0, 200).map((item) => item.slice(0, 300)) : []);

export function getSyncStatus(requestId: string, pendingDirectory: string, resultDirectory: string): SyncStatus {
  // Desktop writes the result before removing the pending file, so the result wins.
  const resultPath = join(resultDirectory, `${requestId}.json`);
  if (existsSync(resultPath)) {
    let result: Record<string, unknown>;
    try { result = JSON.parse(readFileSync(resultPath, 'utf8')); } catch { return { status: 'failed', requestId, error: { code: 'unreadable_result' } }; }
    if (result.status !== 'done' || !Array.isArray(result.secrets)) return { status: 'failed', requestId, error: { code: 'invalid_result' } };
    // Only whitelisted fields are copied out of the file.
    const secrets = result.secrets.slice(0, MAX_SYNC_SECRETS).map((entry: Record<string, unknown>) => ({
      secretId: text(entry.secretId, 200), providerId: text(entry.providerId, 100), modelsSyncStatus: text(entry.modelsSyncStatus, 32),
      ...(typeof entry.validationStatus === 'string' ? { validationStatus: entry.validationStatus.slice(0, 32) } : {}),
      count: num(entry.count), added: models(entry.added), removed: models(entry.removed), addedCount: num(entry.addedCount), removedCount: num(entry.removedCount),
    }));
    return { status: 'done', requestId, secrets };
  }
  if (existsSync(join(pendingDirectory, `${requestId}.json`))) return { status: 'pending', requestId };
  return { status: 'not_found', requestId };
}
