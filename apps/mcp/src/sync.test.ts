import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { CatalogSecret } from './catalog.js';
import { getSyncStatus, isValidSyncRequestId, precheckSyncRequest, queueSyncRequest } from './sync.js';
import { isValidRequestId } from './plan.js';
import { isValidCapsRequestId } from './caps.js';

const secret = (id: string, over: Partial<CatalogSecret> = {}): CatalogSecret => ({ id, name: id, provider: 'deepseek', envKey: 'K', status: 'valid', tags: [], hasValue: true, valueType: 'api_key', archived: false, ...over });

describe('sync request ids', () => {
  it('need the sync- prefix and a safe charset, and never collide with plan/caps ids', () => {
    expect(isValidSyncRequestId('sync-1760000000000-abc123')).toBe(true);
    for (const bad of ['', 'sync', 'sync-', 'sync-../x', 'sync-a/b', 'plan-1', 'caps-1', `sync-${'a'.repeat(130)}`]) expect(isValidSyncRequestId(bad)).toBe(false);
    expect(isValidRequestId('sync-1')).toBe(false);
    expect(isValidCapsRequestId('sync-1')).toBe(false);
    expect(isValidSyncRequestId('plan-1')).toBe(false);
  });
});

describe('precheckSyncRequest', () => {
  const all = [secret('ok'), secret('arch', { archived: true }), secret('empty', { hasValue: false }), secret('tok', { valueType: 'token' })];
  it('accepts all-keys when something is eligible and explicit ids that are eligible', () => {
    expect(precheckSyncRequest(undefined, all)).toBeUndefined();
    expect(precheckSyncRequest(['ok'], all)).toBeUndefined();
  });
  it('rejects unknown, archived, empty and non-api-key ids and an empty vault', () => {
    expect(precheckSyncRequest(['nope'], all)).toMatch(/Unknown secret id/);
    expect(precheckSyncRequest(['arch'], all)).toMatch(/archived/);
    expect(precheckSyncRequest(['empty'], all)).toMatch(/no stored value/);
    expect(precheckSyncRequest(['tok'], all)).toMatch(/not an API key/);
    expect(precheckSyncRequest(undefined, [all[1], all[2]])).toMatch(/No API key/);
  });
});

describe('queue and status files', () => {
  const withDirs = (check: (pending: string, results: string) => void) => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-sync-'));
    try { check(join(directory, 'pending'), join(directory, 'results')); } finally { rmSync(directory, { recursive: true, force: true }); }
  };

  it('queues sync-*.json with ids only and reports pending, then done, then not_found', () => {
    withDirs((pending, results) => {
      const id = queueSyncRequest(pending, ['a', 'a', 'b']);
      expect(id).toMatch(/^sync-/);
      const file = JSON.parse(readFileSync(join(pending, `${id}.json`), 'utf8'));
      expect(file).toEqual(expect.objectContaining({ version: 1, requestId: id, kind: 'sync_models', secretIds: ['a', 'b'] }));
      expect(readdirSync(pending)).toEqual([`${id}.json`]);
      expect(getSyncStatus(id, pending, results)).toEqual({ status: 'pending', requestId: id });
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, `${id}.json`), JSON.stringify({ requestId: id, status: 'done', finishedAt: 'now', secrets: [{ secretId: 'a', providerId: 'deepseek', modelsSyncStatus: 'ok', count: 2, added: ['m2'], removed: ['m0'], addedCount: 1, removedCount: 1, injected: 'ignored' }] }));
      const done = getSyncStatus(id, pending, results);
      expect(done).toEqual({ status: 'done', requestId: id, secrets: [{ secretId: 'a', providerId: 'deepseek', modelsSyncStatus: 'ok', count: 2, added: ['m2'], removed: ['m0'], addedCount: 1, removedCount: 1 }] });
      expect(getSyncStatus('sync-missing', pending, results)).toEqual({ status: 'not_found', requestId: 'sync-missing' });
    });
  });

  it('omits secretIds for an all-keys request and marks garbage results as failed', () => {
    withDirs((pending, results) => {
      const id = queueSyncRequest(pending);
      expect(JSON.parse(readFileSync(join(pending, `${id}.json`), 'utf8'))).not.toHaveProperty('secretIds');
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, `${id}.json`), '{not json');
      expect(getSyncStatus(id, pending, results)).toEqual({ status: 'failed', requestId: id, error: { code: 'unreadable_result' } });
      writeFileSync(join(results, `${id}.json`), JSON.stringify({ status: 'written' }));
      expect(getSyncStatus(id, pending, results)).toEqual({ status: 'failed', requestId: id, error: { code: 'invalid_result' } });
    });
  });
});
