import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openCatalogStore, recommendForStack } from './catalog.js';
import { buildInvocation } from './invocation.js';
import { findModels, loadModelProviders, readModelSnapshot } from './models.js';
import { precheckPlan } from './plan.js';
import { precheckSyncRequest } from './sync.js';

// The table layout of the Rust vault after the typed-items migration.
const SCHEMA = `
  CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider_id TEXT NOT NULL, env_key TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', tags_json TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL DEFAULT 'unknown', value_type TEXT NOT NULL DEFAULT 'api_key', category TEXT NOT NULL DEFAULT 'other', scope TEXT NOT NULL DEFAULT 'global', favorite INTEGER NOT NULL DEFAULT 0, archived INTEGER NOT NULL DEFAULT 0, model_id TEXT NOT NULL DEFAULT '', model_env_key TEXT NOT NULL DEFAULT '', endpoint_url TEXT NOT NULL DEFAULT '', endpoint_env_key TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0, models_synced_at INTEGER NOT NULL DEFAULT 0, models_sync_status TEXT NOT NULL DEFAULT '', validated_at INTEGER NOT NULL DEFAULT 0, subtitle TEXT NOT NULL DEFAULT '', dev_use INTEGER NOT NULL DEFAULT 0);
  CREATE TABLE secret_payloads (secret_id TEXT PRIMARY KEY, ciphertext BLOB NOT NULL);
  CREATE TABLE profiles (id TEXT PRIMARY KEY, name TEXT, description TEXT, updated_at INTEGER);
  CREATE TABLE profile_secrets (profile_id TEXT, secret_id TEXT, position INTEGER);
  CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT, display_name TEXT, last_used_at INTEGER);
  CREATE TABLE provider_models (secret_id TEXT, model_id TEXT, display_name TEXT, input_json TEXT, output_json TEXT, capabilities_json TEXT, capability_source TEXT, synced_at INTEGER);
  CREATE TABLE secret_capability_models (secret_id TEXT, capability TEXT, model_id TEXT, env_key TEXT);
  CREATE TABLE model_capability_overrides (provider_id TEXT, model_id TEXT, capabilities_json TEXT, source TEXT, updated_at INTEGER);`;

type Row = { id: string; name: string; type: string; env?: string; provider?: string; devUse?: number; subtitle?: string };

// Names are deliberately recognisable so any leak fails loudly.
const ROWS: Row[] = [
  { id: 'api', name: 'Fake OpenAI Key', type: 'api_key', env: 'OPENAI_API_KEY', provider: 'openai' },
  { id: 'dev', name: 'Fake Dev Token', type: 'dev_secret', env: 'FAKE_TOKEN', subtitle: 'FAKE_TOKEN' },
  { id: 'tok', name: 'Fake Old Token', type: 'token' },
  { id: 'url-env', name: 'Fake Legacy Url With Env', type: 'url', env: 'FAKE_URL' },
  { id: 'pw-env', name: 'Fake Legacy Password With Env', type: 'password', env: 'FAKE_PW' },
  { id: 'text-env', name: 'Fake Legacy Text With Env', type: 'text', env: 'FAKE_TEXT' },
  { id: 'url-hidden', name: 'Hidden Legacy Url', type: 'url' },
  { id: 'pw-hidden', name: 'Hidden Legacy Password', type: 'password' },
  { id: 'text-hidden', name: 'Hidden Legacy Text', type: 'text' },
  { id: 'login-dev', name: 'Fake Dev Login', type: 'login', env: 'DOUYIN_TEST', devUse: 1, subtitle: 'login.example.com' },
  { id: 'login', name: 'Hidden Personal Login', type: 'login', env: 'ANTHROPIC_API_KEY', subtitle: 'accounts.example.com' },
  { id: 'card', name: 'Hidden Salary Card', type: 'bank_card', subtitle: 'Fake Bank · 尾号 1111', provider: 'openai' },
  { id: 'note', name: 'Hidden Private Note', type: 'note' },
];
const HIDDEN = ROWS.filter((row) => row.name.startsWith('Hidden')).map((row) => row.name);

let directory: string;
let path: string;
let db: DatabaseSync;

function seed(database: DatabaseSync, schema: string, withDevUse = true) {
  database.exec(schema);
  const insert = database.prepare(`INSERT INTO secrets (id, name, provider_id, env_key, value_type, subtitle${withDevUse ? ', dev_use' : ''}, status, updated_at) VALUES (?, ?, ?, ?, ?, ?${withDevUse ? ', ?' : ''}, 'valid', ?)`);
  ROWS.forEach((row, index) => {
    const values: Array<string | number> = [row.id, row.name, row.provider ?? 'generic', row.env ?? '', row.type, row.subtitle ?? ''];
    if (withDevUse) values.push(row.devUse ?? 0);
    insert.run(...values, 100 - index);
    database.prepare('INSERT INTO secret_payloads VALUES (?, X\'0A0B0C\')').run(row.id);
  });
  // Every row has a synced chat model and a chosen capability model, hidden ones included.
  for (const row of ROWS) {
    database.prepare(`INSERT INTO provider_models VALUES (?, 'm-${row.id}', '', '[]', '[]', '["chat","vision"]', 'provider', 5)`).run(row.id);
    database.prepare(`INSERT INTO secret_capability_models VALUES (?, 'vision', 'm-${row.id}', 'VISION_${row.id.toUpperCase().replace(/-/g, '_')}')`).run(row.id);
  }
  database.exec(`UPDATE secrets SET models_synced_at = 5, models_sync_status = 'ok'`);
  database.exec(`INSERT INTO profiles VALUES ('p1', 'Mixed profile', '', 1); INSERT INTO profile_secrets VALUES ('p1', 'api', 0), ('p1', 'card', 1), ('p1', 'login-dev', 2), ('p1', 'note', 3), ('p1', 'login', 4);`);
}

beforeEach(() => {
  directory = mkdtempSync(join(tmpdir(), 'secrethub-typed-'));
  path = join(directory, 'vault.sqlite');
  db = new DatabaseSync(path);
  seed(db, SCHEMA);
});
afterEach(() => { db.close(); rmSync(directory, { recursive: true, force: true }); });

const everything = (value: unknown) => JSON.stringify(value);
const VISIBLE_IDS = ['api', 'dev', 'tok', 'url-env', 'pw-env', 'text-env', 'login-dev'];

describe('AI isolation of personal items', () => {
  it('lists only developer types, env-named legacy rows and dev-use logins', () => {
    const listed = openCatalogStore(path).list({ limit: 100, offset: 0 });
    expect(listed.items.map((item) => item.id).sort()).toEqual([...VISIBLE_IDS].sort());
    expect(listed.total).toBe(VISIBLE_IDS.length);
    const text = everything(listed);
    for (const name of HIDDEN) expect(text).not.toContain(name);
    expect(text).not.toContain('Salary');
    expect(text).not.toContain('accounts.example.com');
    expect(text).not.toContain('尾号');
  });

  it('search never matches a hidden item, not even by its name', () => {
    const catalog = openCatalogStore(path);
    for (const query of ['Hidden', 'Salary', 'Private Note', 'Personal Login', 'accounts.example.com', 'bank']) {
      expect(catalog.list({ query, limit: 100, offset: 0 }).total, query).toBe(0);
    }
    expect(catalog.list({ query: 'Fake Dev Login', limit: 100, offset: 0 }).total).toBe(1);
  });

  it('get_metadata is not found for hidden ids and works for visible ones', () => {
    const catalog = openCatalogStore(path);
    for (const id of ['card', 'note', 'login', 'url-hidden', 'pw-hidden', 'text-hidden']) expect(catalog.get(id), id).toBeUndefined();
    expect(catalog.get('login-dev')).toEqual(expect.objectContaining({ name: 'Fake Dev Login', envKey: 'DOUYIN_TEST', valueType: 'login', hasValue: true }));
    expect(catalog.get('url-env')?.envKey).toBe('FAKE_URL');
  });

  it('a login becomes visible only while dev use is on', () => {
    const catalog = openCatalogStore(path);
    expect(catalog.get('login')).toBeUndefined();
    db.exec(`UPDATE secrets SET dev_use = 1, env_key = 'MY_SITE' WHERE id = 'login'`);
    expect(catalog.get('login')).toEqual(expect.objectContaining({ envKey: 'MY_SITE' }));
    db.exec(`UPDATE secrets SET dev_use = 0 WHERE id = 'login'`);
    expect(catalog.get('login')).toBeUndefined();
    // dev_use never makes a card or note visible.
    db.exec(`UPDATE secrets SET dev_use = 1 WHERE id IN ('card', 'note')`);
    expect(catalog.get('card')).toBeUndefined();
    expect(catalog.get('note')).toBeUndefined();
  });

  it('profiles keep only visible secret ids', () => {
    const profile = openCatalogStore(path).profiles()[0];
    expect(profile.secretIds).toEqual(['api', 'login-dev']);
    expect(everything(openCatalogStore(path).profiles())).not.toContain('card');
  });

  it('recommend ignores hidden entries that merely carry a matching env name', () => {
    const result = recommendForStack(['claude'], openCatalogStore(path));
    expect(result.recommended).toContain('ANTHROPIC_API_KEY');
    expect(result.available).not.toContain('ANTHROPIC_API_KEY');
    expect(result.missing).toContain('ANTHROPIC_API_KEY');
  });

  it('find_models and the model snapshot omit hidden keys, models and capability choices', () => {
    const snapshot = readModelSnapshot(path);
    expect(snapshot.keys.map((key) => key.secretId).sort()).toEqual([...VISIBLE_IDS].sort());
    expect(snapshot.models.every((model) => VISIBLE_IDS.includes(model.secretId))).toBe(true);
    expect((snapshot.capabilityModels ?? []).every((choice) => VISIBLE_IDS.includes(choice.secretId))).toBe(true);
    expect(everything(snapshot)).not.toMatch(/Hidden|Salary|m-card|m-note|VISION_CARD|VISION_NOTE/);
    const providers = loadModelProviders();
    const found = findModels({ capabilities: ['vision'], includeUnverified: true }, snapshot, providers);
    expect(everything(found)).not.toMatch(/Hidden|Salary|m-card|m-note|m-login\b/);
  });

  it('plan and sync pre-checks treat hidden ids as unknown', () => {
    const catalog = openCatalogStore(path);
    const known = new Map(catalog.list({ limit: 100, offset: 0 }).items.map((item) => [item.id, item]));
    const lookup = { secret: (id: string) => known.get(id), providers: loadModelProviders() };
    for (const id of ['card', 'note', 'login']) {
      expect(precheckPlan({ secretIds: [id] }, lookup), id).toMatch(/Unknown secret id/);
      expect(precheckPlan({ bindings: [{ secret_id: id, capability: 'chat', env: { api_key: 'K' } }] }, lookup), id).toMatch(/Unknown secret id/);
    }
    expect(precheckPlan({ secretIds: ['api', 'login-dev'] }, lookup)).toBeUndefined();
    const items = catalog.list({ limit: 100, offset: 0 }).items;
    expect(precheckSyncRequest(['card'], items)).toBeTruthy();
  });

  it('get_invocation cannot be asked about a hidden key', () => {
    const catalog = openCatalogStore(path);
    const providers = loadModelProviders();
    expect(catalog.get('card')).toBeUndefined();
    const visible = catalog.get('api')!;
    expect(everything(buildInvocation(visible, 'chat', readModelSnapshot(path).capabilityModels ?? [], providers))).not.toContain('Hidden');
  });

  it('legacy vaults without the dev_use column hide every login and keep developer types', () => {
    const legacyPath = join(directory, 'legacy.sqlite');
    const legacy = new DatabaseSync(legacyPath);
    try {
      const schema = SCHEMA.replace(", subtitle TEXT NOT NULL DEFAULT '', dev_use INTEGER NOT NULL DEFAULT 0", ", subtitle TEXT NOT NULL DEFAULT ''");
      seed(legacy, schema, false);
      const ids = openCatalogStore(legacyPath).list({ limit: 100, offset: 0 }).items.map((item) => item.id).sort();
      expect(ids).toEqual(VISIBLE_IDS.filter((id) => id !== 'login-dev').sort());
      expect(readModelSnapshot(legacyPath).keys.map((key) => key.secretId).sort()).toEqual(ids);
    } finally { legacy.close(); }
  });
});
