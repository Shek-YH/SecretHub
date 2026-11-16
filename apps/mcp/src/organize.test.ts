import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { getInspection, getOrganizeStatus, isValidInspectRequestId, isValidOrganizeRequestId, planItemSchema, precheckInspect, precheckOrganizePlan, queueInspect, queueOrganize, type PlanItem } from './organize.js';
import { isValidImportRequestId } from './import.js';
import { isValidRequestId } from './plan.js';
import { isValidCapsRequestId } from './caps.js';
import { isValidSyncRequestId } from './sync.js';
import { isValidCaptureRequestId } from './capture.js';
import { isValidMediaRequestId } from './media.js';

const abs = process.platform === 'win32' ? 'C:\\work\\servers' : '/work/servers';
const FAKE = 'fakeval-never-in-a-response';

const withDirs = (check: (pending: string, results: string) => void) => {
  const directory = mkdtempSync(join(tmpdir(), 'secrethub-organize-'));
  try { check(join(directory, 'pending'), join(directory, 'results')); } finally { rmSync(directory, { recursive: true, force: true }); }
};

describe('request ids', () => {
  it('inspect- and organize- ids are recognised by their own queue only', () => {
    expect(isValidInspectRequestId('inspect-1760000000000-abc')).toBe(true);
    expect(isValidOrganizeRequestId('organize-1760000000000-abc')).toBe(true);
    for (const bad of ['', 'inspect-', 'inspect-../x', 'inspect-a/b', `inspect-${'a'.repeat(130)}`, 'import-1', 'organize-1']) expect(isValidInspectRequestId(bad)).toBe(false);
    for (const bad of ['', 'organize-', 'organize-../x', 'inspect-1', 'import-1']) expect(isValidOrganizeRequestId(bad)).toBe(false);
    for (const id of ['inspect-1-aa', 'organize-1-aa']) {
      expect(isValidImportRequestId(id)).toBe(false);
      expect(isValidRequestId(id)).toBe(false);
      expect(isValidCapsRequestId(id)).toBe(false);
      expect(isValidSyncRequestId(id)).toBe(false);
      expect(isValidCaptureRequestId(id)).toBe(false);
      expect(isValidMediaRequestId(id)).toBe(false);
    }
  });
});

describe('plan schema', () => {
  const group = { type: 'env_group', name: '阿里宝塔服务器', category: 'server', variables: [{ name: 'ALI_BT_SSH_HOST', value: { literal: '8.0.0.1' }, secret: false, description: 'SSH 主机' }, { name: 'ALI_BT_SSH_PASSWORD', value: { token: 'S3' } }] };

  it('accepts tokens in the three spellings and literals, and nothing else as a value', () => {
    expect(planItemSchema.safeParse(group).success).toBe(true);
    for (const token of ['S3', '«S3»', '«S12:多行私钥»']) expect(planItemSchema.safeParse({ ...group, variables: [{ name: 'A', value: { token } }] }).success, token).toBe(true);
    for (const value of [{ token: 'x3' }, { token: 'S0' }, { token: 'S3', literal: 'a' }, { literal: '' }, {}, { value: 'plain' }, 'plain']) expect(planItemSchema.safeParse({ ...group, variables: [{ name: 'A', value }] }).success, JSON.stringify(value)).toBe(false);
  });

  it('rejects unknown keys, bad names and unknown types', () => {
    expect(planItemSchema.safeParse({ ...group, extra: 1 }).success).toBe(false);
    expect(planItemSchema.safeParse({ ...group, variables: [{ name: 'BAD-NAME', value: { literal: 'a' } }] }).success).toBe(false);
    expect(planItemSchema.safeParse({ ...group, variables: [{ name: 'A', value: { literal: 'a' }, plain: FAKE }] }).success).toBe(false);
    expect(planItemSchema.safeParse({ ...group, type: 'secret' }).success).toBe(false);
    expect(planItemSchema.safeParse({ ...group, category: 'personal' }).success).toBe(false);
  });
});

describe('precheckOrganizePlan', () => {
  const item = (extra: Partial<PlanItem>): PlanItem => planItemSchema.parse({ type: 'env_group', name: 'g', variables: [{ name: 'A', value: { literal: '1' } }], ...extra });

  it('needs an existing inspection result and sane items', () => {
    withDirs((_pending, results) => {
      expect(precheckOrganizePlan('plan-1', [item({})], results)).toMatch(/inspection_id/);
      expect(precheckOrganizePlan('inspect-1-a', [item({})], results)).toMatch(/Unknown inspection_id/);
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, 'inspect-1-a.json'), '{}');
      expect(precheckOrganizePlan('inspect-1-a', [item({})], results)).toBeUndefined();
      expect(precheckOrganizePlan('inspect-1-a', [], results)).toMatch(/between 1 and 50/);
      expect(precheckOrganizePlan('inspect-1-a', [item({ variables: [] })], results)).toMatch(/at least one variable/);
      expect(precheckOrganizePlan('inspect-1-a', [item({ type: 'api_key' })], results)).toMatch(/providerId/);
      expect(precheckOrganizePlan('inspect-1-a', [item({ type: 'login', variables: [{ name: 'nickname', value: { literal: 'a' } }] })], results)).toMatch(/login variables/);
      expect(precheckOrganizePlan('inspect-1-a', [item({ type: 'note', variables: [] })], results)).toMatch(/notes text/);
      expect(precheckOrganizePlan('inspect-1-a', [item({ variables: [{ name: 'A', value: { literal: '1' } }, { name: 'A', value: { literal: '2' } }] })], results)).toMatch(/duplicate/);
      expect(precheckOrganizePlan('inspect-1-a', [item({ variables: [{ name: 'A', value: { literal: '«S1»' } }] })], results)).toMatch(/placeholder/);
    });
  });
});

describe('queue files', () => {
  it('queue the inspection paths only and the plan as given', () => {
    withDirs((pending) => {
      expect(precheckInspect([abs])).toBeUndefined();
      expect(precheckInspect([])).toMatch(/at least one/);
      expect(precheckInspect(['relative'])).toMatch(/absolute/);
      expect(precheckInspect(Array.from({ length: 21 }, () => abs))).toMatch(/At most 20/);
      const inspectId = queueInspect(pending, [abs, abs]);
      expect(inspectId).toMatch(/^inspect-/);
      const inspect = JSON.parse(readFileSync(join(pending, `${inspectId}.json`), 'utf8'));
      expect(inspect).toMatchObject({ version: 1, requestId: inspectId, kind: 'import_inspect', paths: [abs] });
      expect(Object.keys(inspect).sort()).toEqual(['createdAt', 'kind', 'paths', 'requestId', 'version']);

      const items = [planItemSchema.parse({ type: 'api_key', name: 'Qwen', providerId: 'qwen', variables: [{ name: 'DASHSCOPE_API_KEY', value: { token: 'S4' } }] })];
      const organizeId = queueOrganize(pending, inspectId, items);
      expect(organizeId).toMatch(/^organize-/);
      const organize = JSON.parse(readFileSync(join(pending, `${organizeId}.json`), 'utf8'));
      expect(organize).toMatchObject({ version: 1, requestId: organizeId, kind: 'import_organize', inspectionId: inspectId, items });
      expect(Object.keys(organize).sort()).toEqual(['createdAt', 'inspectionId', 'items', 'kind', 'requestId', 'version']);
      expect(readdirSync(pending)).toHaveLength(2);
    });
  });
});

describe('getInspection', () => {
  const view = (lines: string[][], extra: Record<string, unknown> = {}) => ({ requestId: 'inspect-1-a', status: 'done', inspectionId: 'inspect-1-a', expiresInSeconds: 1800, tokenCount: 2, truncated: false, files: lines.map((entries, index) => ({ name: `f${index}/.env`, lines: entries })), skipped: [{ name: 'x.bin', reason: 'binary' }], finishedAt: new Date().toISOString(), ...extra });

  it('pages by line and reports pending, not_found, failed and expired', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      mkdirSync(pending, { recursive: true });
      expect(getInspection('inspect-1-a', pending, results)).toEqual({ status: 'not_found', requestId: 'inspect-1-a' });
      writeFileSync(join(pending, 'inspect-1-a.json'), '{}');
      expect(getInspection('inspect-1-a', pending, results)).toEqual({ status: 'pending', requestId: 'inspect-1-a' });

      writeFileSync(join(results, 'inspect-1-a.json'), JSON.stringify(view([['A=1', 'B=«S1:12 位»'], ['C=3']])));
      const all = getInspection('inspect-1-a', pending, results);
      expect(all).toMatchObject({ status: 'done', totalLines: 3, nextOffset: null, tokenCount: 2, skipped: [{ name: 'x.bin', reason: 'binary' }] });
      expect((all as { files: unknown[] }).files).toEqual([{ name: 'f0/.env', startLine: 0, lines: ['A=1', 'B=«S1:12 位»'] }, { name: 'f1/.env', startLine: 2, lines: ['C=3'] }]);

      const first = getInspection('inspect-1-a', pending, results, 0, 10) as { files: Array<{ lines: string[] }>; nextOffset: number };
      expect(first.files[0].lines).toEqual(['A=1']);
      expect(first.nextOffset).toBe(1);
      const second = getInspection('inspect-1-a', pending, results, first.nextOffset, 10) as { files: Array<{ name: string; startLine: number; lines: string[] }>; nextOffset: number | null };
      expect(second.files[0]).toEqual({ name: 'f0/.env', startLine: 1, lines: ['B=«S1:12 位»'] });
      expect(second.nextOffset).toBe(2);

      writeFileSync(join(results, 'inspect-2-b.json'), JSON.stringify({ status: 'failed', error: { code: 'nothing_readable', detail: 'C:\\secret\\path' } }));
      expect(getInspection('inspect-2-b', pending, results)).toEqual({ status: 'failed', requestId: 'inspect-2-b', error: { code: 'nothing_readable' } });
      writeFileSync(join(results, 'inspect-3-c.json'), JSON.stringify(view([['A=1']], { finishedAt: '2026-01-01T00:00:00Z' })));
      expect(getInspection('inspect-3-c', pending, results, 0, 100, Date.parse('2026-01-01T01:00:00Z'))).toEqual({ status: 'expired', requestId: 'inspect-3-c' });
    });
  });

  it('copies whitelisted fields only', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, 'inspect-1-a.json'), JSON.stringify(view([['A=1']], { password: FAKE, files: [{ name: 'f/.env', lines: ['A=1'], raw: FAKE }] })));
      expect(JSON.stringify(getInspection('inspect-1-a', pending, results))).not.toContain(FAKE);
    });
  });
});

describe('getOrganizeStatus', () => {
  it('reports pending, not_found, failed and a whitelisted done result', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      mkdirSync(pending, { recursive: true });
      expect(getOrganizeStatus('organize-1-a', pending, results)).toEqual({ status: 'not_found', requestId: 'organize-1-a' });
      writeFileSync(join(pending, 'organize-1-a.json'), '{}');
      expect(getOrganizeStatus('organize-1-a', pending, results)).toEqual({ status: 'pending', requestId: 'organize-1-a' });
      writeFileSync(join(results, 'organize-2-b.json'), JSON.stringify({ status: 'failed', error: { code: 'inspection_expired', detail: FAKE } }));
      expect(getOrganizeStatus('organize-2-b', pending, results)).toEqual({ status: 'failed', requestId: 'organize-2-b', error: { code: 'inspection_expired' } });

      writeFileSync(join(results, 'organize-3-c.json'), JSON.stringify({ status: 'done', batchId: 'batch-1', counts: { createdGroups: 1, addedVariables: 2, savedSeparately: 0, otherItems: 1, skippedVariables: 3, value: FAKE }, items: [{ index: 0, name: 'Server', type: 'env_group', result: 'created', addedVariables: 4, savedSeparately: 0, skippedVariables: 0, value: FAKE }], rejected: [{ item: 0, variable: 'ALI_X', reason: 'literal_not_in_view', value: FAKE }, { item: 1, variable: `has space ${FAKE}`, reason: 'invalid_name' }], password: FAKE }));
      const done = getOrganizeStatus('organize-3-c', pending, results);
      expect(done).toMatchObject({ status: 'done', batchId: 'batch-1', counts: { createdGroups: 1, addedVariables: 2, savedSeparately: 0, otherItems: 1, skippedVariables: 3 } });
      expect((done as { rejected: unknown[] }).rejected).toEqual([{ item: 0, variable: 'ALI_X', reason: 'literal_not_in_view' }, { item: 1, reason: 'invalid_name' }]);
      expect(JSON.stringify(done)).not.toContain(FAKE);
    });
  });
});
