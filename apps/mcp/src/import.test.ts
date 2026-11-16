import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { getImportStatus, isValidImportRequestId, precheckImport, queueImport } from './import.js';
import { isValidRequestId } from './plan.js';
import { isValidCapsRequestId } from './caps.js';
import { isValidSyncRequestId } from './sync.js';
import { isValidCaptureRequestId } from './capture.js';
import { isValidMediaRequestId } from './media.js';

const abs = process.platform === 'win32' ? 'C:\\work\\servers' : '/work/servers';

describe('import request ids', () => {
  it('need the import- prefix and are not recognised by any other queue', () => {
    expect(isValidImportRequestId('import-1760000000000-abc123')).toBe(true);
    for (const bad of ['', 'import', 'import-', 'import-../x', 'import-a/b', 'plan-1', 'sync-1', `import-${'a'.repeat(130)}`]) expect(isValidImportRequestId(bad)).toBe(false);
    const id = 'import-1-aa';
    expect(isValidRequestId(id)).toBe(false);
    expect(isValidCapsRequestId(id)).toBe(false);
    expect(isValidSyncRequestId(id)).toBe(false);
    expect(isValidCaptureRequestId(id)).toBe(false);
    expect(isValidMediaRequestId(id)).toBe(false);
    for (const other of ['plan-1', 'caps-1', 'sync-1', 'capture-1', 'media-1']) expect(isValidImportRequestId(other)).toBe(false);
  });
});

describe('precheckImport', () => {
  it('accepts absolute paths and rejects empty, relative, control-character and too many paths', () => {
    expect(precheckImport([abs])).toBeUndefined();
    expect(precheckImport([])).toMatch(/at least one/);
    expect(precheckImport(['relative/dir'])).toMatch(/absolute/);
    expect(precheckImport([`${abs}\u0000x`])).toMatch(/control/);
    expect(precheckImport(Array.from({ length: 21 }, () => abs))).toMatch(/At most 20/);
  });
  it('does not read or even stat the paths', () => {
    expect(precheckImport(['/definitely/not/there'].map((path) => (process.platform === 'win32' ? `C:${path.replace(/\//g, '\\')}` : path)))).toBeUndefined();
  });
});

describe('queue and status files', () => {
  const withDirs = (check: (pending: string, results: string) => void) => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-import-'));
    try { check(join(directory, 'pending'), join(directory, 'results')); } finally { rmSync(directory, { recursive: true, force: true }); }
  };

  it('queues paths only, deduplicated, and reports pending until a result exists', () => {
    withDirs((pending, results) => {
      const requestId = queueImport(pending, [abs, abs]);
      expect(requestId).toMatch(/^import-/);
      const file = JSON.parse(readFileSync(join(pending, `${requestId}.json`), 'utf8'));
      expect(file).toMatchObject({ version: 1, requestId, kind: 'import', paths: [abs] });
      expect(Object.keys(file).sort()).toEqual(['createdAt', 'kind', 'paths', 'requestId', 'version']);
      expect(readdirSync(pending)).toEqual([`${requestId}.json`]);
      expect(getImportStatus(requestId, pending, results)).toEqual({ status: 'pending', requestId });
      expect(getImportStatus('import-nothing', pending, results)).toEqual({ status: 'not_found', requestId: 'import-nothing' });
    });
  });

  it('reports done with counts and names only, whitelisting what it copies', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, 'import-1-ok.json'), JSON.stringify({ requestId: 'import-1-ok', status: 'done', added: 2, updated: 1, skipped: 3, names: { added: ['Server A', 'Qwen'], updated: ['Server B'], skipped: ['x', 'y', 'z'], leaked: ['SECRET'] }, value: 'FAKE-SECRET', finishedAt: 't' }));
      const status = getImportStatus('import-1-ok', pending, results);
      expect(status).toEqual({ status: 'done', requestId: 'import-1-ok', added: 2, updated: 1, skipped: 3, names: { added: ['Server A', 'Qwen'], updated: ['Server B'], skipped: ['x', 'y', 'z'] } });
      expect(JSON.stringify(status)).not.toContain('FAKE-SECRET');
      expect(JSON.stringify(status)).not.toContain('leaked');
    });
  });

  it('reports declined, failed and unreadable results, and the result wins over a pending file', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      mkdirSync(pending, { recursive: true });
      writeFileSync(join(results, 'import-2-no.json'), JSON.stringify({ status: 'declined' }));
      writeFileSync(join(results, 'import-3-bad.json'), JSON.stringify({ status: 'failed', error: { code: 'import_failed', detail: 'C:\\secret\\path' } }));
      writeFileSync(join(results, 'import-4-junk.json'), 'not json');
      writeFileSync(join(pending, 'import-2-no.json'), '{}');
      expect(getImportStatus('import-2-no', pending, results)).toEqual({ status: 'declined', requestId: 'import-2-no' });
      expect(getImportStatus('import-3-bad', pending, results)).toEqual({ status: 'failed', requestId: 'import-3-bad', error: { code: 'import_failed' } });
      expect(getImportStatus('import-4-junk', pending, results)).toEqual({ status: 'failed', requestId: 'import-4-junk', error: { code: 'unreadable_result' } });
    });
  });
});
