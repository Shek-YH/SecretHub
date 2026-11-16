import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { z } from 'zod';
import type { CatalogSecret } from './catalog.js';
import type { ModelProvider } from './models.js';
import { bindingSchema, getPlanStatus, isValidRequestId, MAX_BINDINGS, precheckPlan, type Binding } from './plan.js';

const secret = (id: string, over: Partial<CatalogSecret> = {}): CatalogSecret => ({ id, name: id, provider: 'agg', envKey: '', status: 'valid', tags: [], hasValue: true, archived: false, endpointUrl: '', ...over });
const secrets = new Map<string, CatalogSecret>([['s1', secret('s1')], ['s2', secret('s2')], ['old', secret('old', { archived: true })], ['empty', secret('empty', { hasValue: false })], ['noprov', secret('noprov', { provider: 'nolist' })]]);
const providers: ModelProvider[] = [{ id: 'agg', baseUrl: 'https://agg.example/v1' }, { id: 'nolist' }];
const lookup = { secret: (id: string) => secrets.get(id), providers };
const bind = (over: Partial<Binding> & { env: Binding['env'] }): Binding => ({ secret_id: 's1', capability: 'chat', model_id: 'm', ...over });

describe('bindingSchema', () => {
  const schema = z.array(bindingSchema);
  it('accepts a full binding', () => {
    expect(schema.safeParse([{ secret_id: 's1', capability: 'chat', model_id: 'google/gemini', env: { api_key: 'OPENROUTER_API_KEY', base_url: 'OPENROUTER_BASE_URL', model: 'LLM_MODEL' } }]).success).toBe(true);
  });
  it('accepts a model-only binding and an api-key-only binding without model_id', () => {
    expect(schema.safeParse([{ secret_id: 's1', capability: 'image_gen', model_id: 'x', env: { model: 'IMAGE_MODEL' } }, { secret_id: 's1', capability: 'chat', env: { api_key: 'K' } }]).success).toBe(true);
  });
  it.each([
    ['unknown capability', { secret_id: 's1', capability: 'telepathy', env: { api_key: 'K' } }],
    ['empty env', { secret_id: 's1', capability: 'chat', env: {} }],
    ['model without model_id', { secret_id: 's1', capability: 'chat', env: { model: 'LLM_MODEL' } }],
    ['bad env name', { secret_id: 's1', capability: 'chat', env: { api_key: '1BAD' } }],
    ['env name with space', { secret_id: 's1', capability: 'chat', env: { api_key: 'A B' } }],
    ['unknown env field', { secret_id: 's1', capability: 'chat', env: { api_key: 'K', value: 'x' } }],
    ['extra binding field', { secret_id: 's1', capability: 'chat', env: { api_key: 'K' }, value: 'x' }],
    ['missing secret_id', { capability: 'chat', env: { api_key: 'K' } }],
  ])('rejects %s', (_name, binding) => {
    expect(schema.safeParse([binding]).success).toBe(false);
  });
});

describe('precheckPlan', () => {
  it('requires secret_ids or bindings', () => {
    expect(precheckPlan({}, lookup)).toMatch(/secret_ids, bindings/);
    expect(precheckPlan({ secretIds: [], bindings: [] }, lookup)).toMatch(/secret_ids, bindings/);
  });
  it('accepts shared api key across bindings with one model variable each', () => {
    const bindings = [bind({ env: { api_key: 'API_KEY', base_url: 'BASE_URL', model: 'LLM_MODEL' } }), bind({ capability: 'image_gen', model_id: 'img', env: { api_key: 'API_KEY', base_url: 'BASE_URL', model: 'IMAGE_MODEL' } })];
    expect(precheckPlan({ bindings }, lookup)).toBeUndefined();
  });
  it('rejects unknown, archived and value-less secrets in secret_ids and bindings', () => {
    expect(precheckPlan({ secretIds: ['nope'] }, lookup)).toMatch(/Unknown secret id: nope/);
    expect(precheckPlan({ secretIds: ['old'] }, lookup)).toMatch(/archived/);
    expect(precheckPlan({ bindings: [bind({ secret_id: 'empty', env: { api_key: 'K' } })] }, lookup)).toMatch(/no stored value/);
    expect(precheckPlan({ bindings: [bind({ secret_id: 'nope', env: { api_key: 'K' } })] }, lookup)).toMatch(/Unknown secret id/);
  });
  it('rejects one env name bound to two different sources', () => {
    expect(precheckPlan({ bindings: [bind({ env: { model: 'LLM_MODEL' } }), bind({ capability: 'code', model_id: 'other', env: { model: 'LLM_MODEL' } })] }, lookup)).toMatch(/LLM_MODEL/);
    expect(precheckPlan({ bindings: [bind({ env: { api_key: 'KEY' } }), bind({ secret_id: 's2', env: { api_key: 'KEY' } })] }, lookup)).toMatch(/KEY/);
    expect(precheckPlan({ bindings: [bind({ env: { api_key: 'X' } }), bind({ env: { model: 'X' } })] }, lookup)).toMatch(/X/);
  });
  it('allows identical duplicates', () => {
    expect(precheckPlan({ bindings: [bind({ env: { model: 'LLM_MODEL' } }), bind({ env: { model: 'LLM_MODEL' } })] }, lookup)).toBeUndefined();
  });
  it('rejects base_url when no URL is known', () => {
    expect(precheckPlan({ bindings: [bind({ secret_id: 'noprov', env: { base_url: 'U' } })] }, lookup)).toMatch(/No base URL/);
  });
  it('caps bindings', () => {
    const many = Array.from({ length: MAX_BINDINGS + 1 }, (_, i) => bind({ env: { model: `M${i}` }, model_id: `m${i}` }));
    expect(precheckPlan({ bindings: many }, lookup)).toMatch(/At most/);
  });
});

describe('request ids and plan status', () => {
  it('validates request ids', () => {
    expect(isValidRequestId('plan-1760000000000-abc123')).toBe(true);
    for (const bad of ['', 'plan', 'x-plan-1', 'plan-../x', 'plan-a/b', 'plan-a\\b', 'plan-a.json', 'plan-' + 'a'.repeat(200)]) expect(isValidRequestId(bad)).toBe(false);
  });

  const withDirs = (check: (planDir: string, resultDir: string) => void) => {
    const root = mkdtempSync(join(tmpdir(), 'secrethub-plan-'));
    const planDir = join(root, 'mcp-pending');
    const resultDir = join(root, 'mcp-results');
    mkdirSync(planDir); mkdirSync(resultDir);
    try { check(planDir, resultDir); } finally { rmSync(root, { recursive: true, force: true }); }
  };

  it('reports pending, written, declined, failed and not_found', () => {
    withDirs((planDir, resultDir) => {
      writeFileSync(join(planDir, 'plan-pending.json'), '{}');
      writeFileSync(join(resultDir, 'plan-ok.json'), JSON.stringify({ requestId: 'plan-ok', status: 'written', writtenKeys: ['A', 'B'], skippedKeys: ['C'], warnings: ['B: model_not_in_synced_list'], finishedAt: 'x', extra: 'ignored' }));
      writeFileSync(join(resultDir, 'plan-no.json'), JSON.stringify({ status: 'declined' }));
      writeFileSync(join(resultDir, 'plan-bad.json'), JSON.stringify({ status: 'failed', error: { code: 'env_tracked', message: 'detail that must not be forwarded' } }));
      writeFileSync(join(resultDir, 'plan-garbled.json'), '{not json');
      // Result is written before the pending file is removed, so the result wins.
      writeFileSync(join(planDir, 'plan-ok.json'), '{}');
      expect(getPlanStatus('plan-pending', planDir, resultDir)).toEqual({ status: 'pending', requestId: 'plan-pending' });
      expect(getPlanStatus('plan-ok', planDir, resultDir)).toEqual({ status: 'written', requestId: 'plan-ok', writtenKeys: ['A', 'B'], skippedKeys: ['C'], warnings: ['B: model_not_in_synced_list'] });
      expect(getPlanStatus('plan-no', planDir, resultDir)).toEqual({ status: 'declined', requestId: 'plan-no' });
      expect(getPlanStatus('plan-bad', planDir, resultDir)).toEqual({ status: 'failed', requestId: 'plan-bad', error: { code: 'env_tracked' } });
      expect(getPlanStatus('plan-garbled', planDir, resultDir)).toEqual({ status: 'failed', requestId: 'plan-garbled', error: { code: 'unreadable_result' } });
      expect(getPlanStatus('plan-missing', planDir, resultDir)).toEqual({ status: 'not_found', requestId: 'plan-missing' });
    });
  });
});
