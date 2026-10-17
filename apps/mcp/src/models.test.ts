import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { z } from 'zod';
import { CAPABILITIES, effectiveModelsPath, findModels, fitToLimit, loadModelProviders, readModelSnapshot, type KeyRow, type ModelProvider, type ModelRow } from './models.js';

const providers: ModelProvider[] = [
  { id: 'agg', kind: 'aggregator', protocol: 'openai', baseUrl: 'https://agg.example/v1', envKeys: ['AGG_API_KEY'], baseUrlEnvKey: 'AGG_BASE_URL' },
  { id: 'direct', baseUrl: 'https://direct.example/v1', envKeys: ['DIRECT_API_KEY'], baseUrlEnvKey: 'DIRECT_BASE_URL' },
  { id: 'nolist' },
];

const key = (id: string, over: Partial<KeyRow> = {}): KeyRow => ({ secretId: id, secretName: `Name ${id}`, providerId: 'direct', keyStatus: 'valid', endpointUrl: '', envKey: '', endpointEnvKey: '', hasValue: true, archived: false, valueType: 'api_key', modelsSyncedAt: 100, modelsSyncStatus: 'ok', ...over });
const model = (secretId: string, modelId: string, capabilities: string[], capabilitySource = 'provider'): ModelRow => ({ secretId, modelId, capabilities, capabilitySource });
const ids = (result: ReturnType<typeof findModels>, index = 0) => result.results[index].candidates.map((c) => `${c.secretId}/${c.modelId}`);

describe('findModels ranking', () => {
  it('ranks non-assumed before assumed chat candidates', () => {
    const result = findModels({ capabilities: ['chat'] }, { keys: [key('a')], models: [model('a', 'aaa', [], 'unknown'), model('a', 'zzz', ['chat'], 'heuristic')] }, providers);
    expect(ids(result)).toEqual(['a/zzz', 'a/aaa']);
    expect(result.results[0].candidates.map((c) => c.assumed)).toEqual([false, true]);
  });

  it('ranks valid keys before unverified ones when included', () => {
    const result = findModels({ capabilities: ['chat'], includeUnverified: true }, { keys: [key('a', { keyStatus: 'unknown' }), key('b')], models: [model('a', 'm', ['chat']), model('b', 'm', ['chat'])] }, providers);
    expect(ids(result)).toEqual(['b/m', 'a/m']);
    expect(result.results[0].candidates[0]).not.toHaveProperty('reason');
    expect(result.results[0].candidates[1].reason).toBe('key status: unknown');
  });

  it('ranks by capability source: user > ai > provider > catalog > heuristic > unknown', () => {
    const sources = ['unknown', 'heuristic', 'catalog', 'provider', 'ai', 'user'];
    const models = sources.map((source, i) => model('a', `m${i}`, ['chat'], source));
    const result = findModels({ capabilities: ['chat'] }, { keys: [key('a')], models }, providers);
    expect(result.results[0].candidates.map((c) => c.capabilitySource)).toEqual(['user', 'ai', 'provider', 'catalog', 'heuristic', 'unknown']);
  });

  it('prefers secrets that cover more of the requested capabilities', () => {
    const snapshot = { keys: [key('one'), key('both')], models: [model('one', 'a', ['chat']), model('both', 'z', ['chat']), model('both', 'y', ['vision'])] };
    const result = findModels({ capabilities: ['chat', 'vision'] }, snapshot, providers);
    expect(ids(result, 0)).toEqual(['both/z', 'one/a']);
  });

  it('prefers aggregators, then most recent sync, then model id', () => {
    const snapshot = {
      keys: [key('d1', { modelsSyncedAt: 50 }), key('d2', { modelsSyncedAt: 90 }), key('g', { providerId: 'agg', modelsSyncedAt: 10 })],
      models: [model('d1', 'a', ['chat']), model('d2', 'b', ['chat']), model('d2', 'a', ['chat']), model('g', 'c', ['chat'])],
    };
    expect(ids(findModels({ capabilities: ['chat'] }, snapshot, providers))).toEqual(['g/c', 'd2/a', 'd2/b', 'd1/a']);
  });

  it('applies limit per capability after ranking', () => {
    const snapshot = { keys: [key('a')], models: [model('a', 'm1', ['chat', 'vision']), model('a', 'm2', ['chat', 'vision']), model('a', 'm3', ['chat', 'vision'])] };
    const result = findModels({ capabilities: ['chat', 'vision'], limit: 2 }, snapshot, providers);
    expect(result.results.map((r) => r.candidates.length)).toEqual([2, 2]);
  });
});

describe('assumed chat', () => {
  const snapshot = { keys: [key('a')], models: [model('a', 'gpt-x', [], 'unknown')] };

  it('includes unknown models only as assumed chat candidates', () => {
    const result = findModels({ capabilities: ['chat', 'vision', 'embedding'] }, snapshot, providers);
    expect(result.results[0].candidates).toHaveLength(1);
    expect(result.results[0].candidates[0]).toEqual(expect.objectContaining({ modelId: 'gpt-x', assumed: true, capabilitySource: 'unknown', capabilities: [] }));
    expect(result.uncovered).toEqual(['vision', 'embedding']);
  });

  it('does not assume chat for models with known non-chat capabilities', () => {
    const result = findModels({ capabilities: ['chat'] }, { keys: [key('a')], models: [model('a', 'emb', ['embedding'], 'heuristic')] }, providers);
    expect(result.uncovered).toEqual(['chat']);
  });

  it('lets a user override beat the stored source', () => {
    const result = findModels({ capabilities: ['vision'] }, { keys: [key('a')], models: [model('a', 'gpt-x', ['vision'], 'user')] }, providers);
    expect(result.results[0].candidates[0]).toEqual(expect.objectContaining({ assumed: false, capabilitySource: 'user' }));
  });
});

describe('findModels filters and fields', () => {
  it('reports uncovered capabilities', () => {
    const result = findModels({ capabilities: ['chat', 'video_gen'] }, { keys: [key('a')], models: [model('a', 'm', ['chat'])] }, providers);
    expect(result.uncovered).toEqual(['video_gen']);
  });

  it('excludes archived, valueless, non-api-key secrets and, by default, unverified keys', () => {
    const snapshot = {
      keys: [key('arch', { archived: true }), key('empty', { hasValue: false }), key('tok', { valueType: 'token' }), key('unv', { keyStatus: 'invalid' }), key('ok')],
      models: ['arch', 'empty', 'tok', 'unv', 'ok'].map((id) => model(id, 'm', ['chat'])),
    };
    expect(ids(findModels({ capabilities: ['chat'] }, snapshot, providers))).toEqual(['ok/m']);
    expect(ids(findModels({ capabilities: ['chat'], includeUnverified: true }, snapshot, providers))).toEqual(['ok/m', 'unv/m']);
  });

  it('filters by provider', () => {
    const snapshot = { keys: [key('a'), key('g', { providerId: 'agg' })], models: [model('a', 'm', ['chat']), model('g', 'm', ['chat'])] };
    expect(ids(findModels({ capabilities: ['chat'], provider: 'agg' }, snapshot, providers))).toEqual(['g/m']);
  });

  it('prefers the secret-configured env keys and base URL over catalog defaults', () => {
    const snapshot = { keys: [key('own', { providerId: 'agg', endpointUrl: 'https://proxy.example/v1', envKey: 'MY_KEY', endpointEnvKey: 'MY_BASE' }), key('def', { providerId: 'agg' })], models: [model('own', 'm', ['chat']), model('def', 'n', ['chat'])] };
    const [own, def] = findModels({ capabilities: ['chat'] }, snapshot, providers).results[0].candidates;
    expect(own).toEqual(expect.objectContaining({ baseUrl: 'https://proxy.example/v1', apiKeyEnvKey: 'MY_KEY', baseUrlEnvKey: 'MY_BASE', providerKind: 'aggregator', protocol: 'openai' }));
    expect(def).toEqual(expect.objectContaining({ baseUrl: 'https://agg.example/v1', apiKeyEnvKey: 'AGG_API_KEY', baseUrlEnvKey: 'AGG_BASE_URL' }));
  });

  it('never carries a value property', () => {
    const result = findModels({ capabilities: ['chat'] }, { keys: [key('a')], models: [model('a', 'm', ['chat'])] }, providers);
    for (const candidate of result.results[0].candidates) expect(candidate).not.toHaveProperty('value');
    expect(JSON.stringify(result)).not.toContain('"cover"');
  });

  it('lists unsynced and failed keys whose provider lists models', () => {
    const snapshot = {
      keys: [key('never', { modelsSyncedAt: 0, modelsSyncStatus: '' }), key('failed', { modelsSyncStatus: 'failed' }), key('unsup', { modelsSyncedAt: 0, modelsSyncStatus: 'unsupported' }), key('nolist', { providerId: 'nolist', modelsSyncedAt: 0 }), key('synced'), key('arch', { archived: true, modelsSyncedAt: 0 })],
      models: [],
    };
    const result = findModels({ capabilities: ['chat'] }, snapshot, providers);
    expect(result.unsyncedKeys.map((k) => k.secretId)).toEqual(['never', 'failed']);
    expect(result.unsyncedKeys[1]).toEqual({ secretId: 'failed', secretName: 'Name failed', providerId: 'direct', keyStatus: 'valid', modelsSyncStatus: 'failed' });
  });

  it('lists never-checked keys as unsynced even when unverified keys are excluded from candidates', () => {
    const snapshot = { keys: [key('fresh', { keyStatus: 'unknown', modelsSyncedAt: 0, modelsSyncStatus: '' })], models: [] };
    const result = findModels({ capabilities: ['chat'] }, snapshot, providers);
    expect(result.results[0].candidates).toEqual([]);
    expect(result.unsyncedKeys).toEqual([expect.objectContaining({ secretId: 'fresh', keyStatus: 'unknown' })]);
  });

  it('trims candidates evenly and flags truncation', () => {
    const many = Array.from({ length: 100 }, (_, i) => model('a', `model-${String(i).padStart(3, '0')}`, ['chat', 'vision']));
    const full = findModels({ capabilities: ['chat', 'vision'], limit: 100 }, { keys: [key('a')], models: many }, providers);
    const fitted = fitToLimit(full, 8_000);
    expect(fitted.truncated).toBe(true);
    expect(Math.abs(fitted.results[0].candidates.length - fitted.results[1].candidates.length)).toBeLessThanOrEqual(1);
    expect(JSON.stringify(fitted, null, 2).length).toBeLessThanOrEqual(8_000);
    expect(fitToLimit(full, 1_000_000).truncated).toBeUndefined();
  });
});

describe('input schema', () => {
  it('rejects unknown capabilities and empty lists', () => {
    const schema = z.array(z.enum(CAPABILITIES)).min(1);
    expect(schema.safeParse(['chat', 'telepathy']).success).toBe(false);
    expect(schema.safeParse([]).success).toBe(false);
    expect(schema.safeParse(['rerank']).success).toBe(true);
  });

  it('loads the shared provider catalog with aggregator kinds', () => {
    expect(loadModelProviders().some((p) => p.kind === 'aggregator')).toBe(true);
  });

  it('reads the effective models file but keeps bundled URLs, and falls back when it is missing or broken', () => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-effective-'));
    try {
      const bundled = loadModelProviders();
      const bundledDeepseek = bundled.find((p) => p.id === 'deepseek')!;
      const file = join(directory, 'effective-models.json');
      writeFileSync(file, JSON.stringify({ serial: 5, providers: { deepseek: { defaultModel: 'ds-new', models: [{ id: 'ds-new' }, { notId: 1 }] }, bogus: { models: [{ id: 'x' }] } } }));
      const merged = loadModelProviders(file);
      const deepseek = merged.find((p) => p.id === 'deepseek')!;
      expect(deepseek.models).toEqual([{ id: 'ds-new' }]);
      expect(deepseek.defaultModel).toBe('ds-new');
      expect(deepseek.baseUrl).toBe(bundledDeepseek.baseUrl);
      expect(merged.some((p) => p.id === 'bogus')).toBe(false);
      expect(merged.length).toBe(bundled.length);
      expect(loadModelProviders(join(directory, 'missing.json'))).toEqual(bundled);
      writeFileSync(file, '{not json');
      expect(loadModelProviders(file)).toEqual(bundled);
      writeFileSync(file, JSON.stringify({ providers: 'nope' }));
      expect(loadModelProviders(file)).toEqual(bundled);
    } finally { rmSync(directory, { recursive: true, force: true }); }
  });

  it('derives the effective models path next to the vault unless overridden', () => {
    const saved = process.env.SECRETHUB_EFFECTIVE_MODELS_PATH;
    delete process.env.SECRETHUB_EFFECTIVE_MODELS_PATH;
    try {
      expect(effectiveModelsPath(join('x', 'data', 'vault.sqlite'))).toBe(join('x', 'data', 'catalog', 'effective-models.json'));
      process.env.SECRETHUB_EFFECTIVE_MODELS_PATH = 'custom.json';
      expect(effectiveModelsPath('anything')).toBe('custom.json');
    } finally { if (saved === undefined) delete process.env.SECRETHUB_EFFECTIVE_MODELS_PATH; else process.env.SECRETHUB_EFFECTIVE_MODELS_PATH = saved; }
  });
});

describe('readModelSnapshot (real SQLite)', () => {
  const secretsDdl = `CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT, provider_id TEXT, env_key TEXT, status TEXT, value_type TEXT, archived INTEGER, endpoint_url TEXT, endpoint_env_key TEXT, updated_at INTEGER%EXTRA%);
    CREATE TABLE secret_payloads (secret_id TEXT PRIMARY KEY, ciphertext BLOB);`;
  const withDb = (setup: (db: DatabaseSync) => void, check: (path: string) => void) => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-models-'));
    const path = join(directory, 'vault.sqlite');
    const db = new DatabaseSync(path);
    try { setup(db); db.close(); check(path); } finally { rmSync(directory, { recursive: true, force: true }); }
  };

  it('applies overrides by (provider, model) and exposes only metadata', () => {
    withDb((db) => {
      db.exec(secretsDdl.replace('%EXTRA%', ', models_synced_at INTEGER, models_sync_status TEXT') + `
        CREATE TABLE provider_models (secret_id TEXT, model_id TEXT, display_name TEXT, input_json TEXT, output_json TEXT, capabilities_json TEXT, capability_source TEXT, synced_at INTEGER);
        CREATE TABLE model_capability_overrides (provider_id TEXT, model_id TEXT, capabilities_json TEXT, source TEXT, updated_at INTEGER);
        INSERT INTO secrets VALUES ('s1', 'Main', 'direct', 'DIRECT_API_KEY', 'valid', 'api_key', 0, '', '', 1, 123, 'ok');
        INSERT INTO secret_payloads VALUES ('s1', X'DEADBEEF');
        INSERT INTO provider_models VALUES ('s1', 'plain', '', '[]', '[]', '[]', 'unknown', 123), ('s1', 'seen', '', '[]', '[]', '["chat"]', 'heuristic', 123);
        INSERT INTO model_capability_overrides VALUES ('direct', 'plain', '["vision","chat"]', 'user', 1);`);
    }, (path) => {
      const snapshot = readModelSnapshot(path);
      expect(snapshot.keys[0]).toEqual(expect.objectContaining({ secretId: 's1', hasValue: true, modelsSyncedAt: 123, modelsSyncStatus: 'ok' }));
      expect(snapshot.models.find((m) => m.modelId === 'plain')).toEqual({ secretId: 's1', modelId: 'plain', capabilities: ['vision', 'chat'], capabilitySource: 'user' });
      expect(JSON.stringify(snapshot)).not.toMatch(/deadbeef/i);
      const result = findModels({ capabilities: ['vision'] }, snapshot, providers);
      expect(ids(result)).toEqual(['s1/plain']);
      expect(result.results[0].candidates[0]).toEqual(expect.objectContaining({ capabilitySource: 'user', baseUrl: 'https://direct.example/v1', apiKeyEnvKey: 'DIRECT_API_KEY', assumed: false }));
    });
  });

  it('tolerates an older vault without model tables or sync columns', () => {
    withDb((db) => {
      db.exec(secretsDdl.replace('%EXTRA%', '') + `
        INSERT INTO secrets VALUES ('old', 'Old', 'direct', 'DIRECT_API_KEY', 'valid', 'api_key', 0, '', '', 1);
        INSERT INTO secret_payloads VALUES ('old', X'01');`);
    }, (path) => {
      const result = findModels({ capabilities: ['chat'] }, readModelSnapshot(path), providers);
      expect(result.uncovered).toEqual(['chat']);
      expect(result.unsyncedKeys.map((k) => k.secretId)).toEqual(['old']);
    });
  });

  it('returns an empty snapshot for a missing vault file', () => {
    expect(readModelSnapshot(join(tmpdir(), 'secrethub-does-not-exist', 'vault.sqlite'))).toEqual({ keys: [], models: [] });
  });
});
