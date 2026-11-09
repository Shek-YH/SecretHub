import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openCatalogStore } from './catalog.js';
import { bindingSchema, groupBindingSchema, precheckPlan, profilePlanFields } from './plan.js';

const SCHEMA = `
  CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT NOT NULL, provider_id TEXT NOT NULL, env_key TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', tags_json TEXT NOT NULL DEFAULT '[]', status TEXT NOT NULL DEFAULT 'unknown', value_type TEXT NOT NULL DEFAULT 'api_key', category TEXT NOT NULL DEFAULT 'other', scope TEXT NOT NULL DEFAULT 'global', favorite INTEGER NOT NULL DEFAULT 0, archived INTEGER NOT NULL DEFAULT 0, model_id TEXT NOT NULL DEFAULT '', model_env_key TEXT NOT NULL DEFAULT '', endpoint_url TEXT NOT NULL DEFAULT '', endpoint_env_key TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0, subtitle TEXT NOT NULL DEFAULT '', dev_use INTEGER NOT NULL DEFAULT 0, ai_values INTEGER NOT NULL DEFAULT 1);
  CREATE TABLE secret_payloads (secret_id TEXT PRIMARY KEY, ciphertext BLOB NOT NULL);
  CREATE TABLE group_variables (secret_id TEXT NOT NULL, position INTEGER NOT NULL, name TEXT NOT NULL, secret INTEGER NOT NULL, plain_value TEXT NOT NULL DEFAULT '', PRIMARY KEY (secret_id, name));
  CREATE TABLE profiles (id TEXT PRIMARY KEY, name TEXT, description TEXT, updated_at INTEGER);
  CREATE TABLE profile_secrets (profile_id TEXT, secret_id TEXT, position INTEGER, variables_json TEXT NOT NULL DEFAULT '');
  CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT, display_name TEXT, last_used_at INTEGER);`;

const SECRET_MARKER = 'FAKE-SECRET-MUST-NOT-APPEAR';

let directory: string;
let path: string;

function seed(aiValuesOff = false) {
  const db = new DatabaseSync(path);
  db.exec(SCHEMA);
  const add = (id: string, type: string, aiValues = 1) => {
    db.prepare("INSERT INTO secrets (id, name, provider_id, env_key, value_type, ai_values) VALUES (?, ?, 'group', '', ?, ?)").run(id, `Group ${id}`, type, aiValues);
    db.prepare("INSERT INTO secret_payloads VALUES (?, X'0A0B')").run(id);
  };
  add('g1', 'env_group', aiValuesOff ? 0 : 1);
  add('g2', 'env_group');
  const variable = (id: string, position: number, name: string, secret: number, plain: string) => db.prepare('INSERT INTO group_variables VALUES (?, ?, ?, ?, ?)').run(id, position, name, secret, plain);
  variable('g1', 0, 'SSH_HOST', 0, '203.0.113.7');
  variable('g1', 1, 'SSH_PASSWORD', 1, SECRET_MARKER); // secret rows never hold a value in the real app; even if one did, it must not leak
  variable('g1', 2, 'SSH_PORT', 0, '22');
  variable('g2', 0, 'SSH_HOST', 0, '198.51.100.1');
  db.exec("INSERT INTO secrets (id, name, provider_id, env_key, value_type) VALUES ('k1', 'Fake key', 'openai', 'OPENAI_API_KEY', 'api_key'); INSERT INTO secret_payloads VALUES ('k1', X'0A');");
  db.close();
}

beforeEach(() => { directory = mkdtempSync(join(tmpdir(), 'secrethub-groups-')); path = join(directory, 'vault.sqlite'); });
afterEach(() => rmSync(directory, { recursive: true, force: true }));

const lookup = (store = openCatalogStore(path)) => ({ secret: (id: string) => store.get(id), providers: [] });

describe('config groups in the catalog', () => {
  it('lists groups with plain values and never a secret value', () => {
    seed();
    const store = openCatalogStore(path);
    const group = store.get('g1')!;
    expect(group.valueType).toBe('env_group');
    expect(group.aiValues).toBe(true);
    expect(group.variables).toEqual([
      { name: 'SSH_HOST', secret: false, value: '203.0.113.7' },
      { name: 'SSH_PASSWORD', secret: true },
      { name: 'SSH_PORT', secret: false, value: '22' },
    ]);
    expect(JSON.stringify(store.list({ limit: 100, offset: 0 }))).not.toContain(SECRET_MARKER);
  });

  it('returns no plain values when the group turns AI values off, but keeps the names', () => {
    seed(true);
    const group = openCatalogStore(path).get('g1')!;
    expect(group.aiValues).toBe(false);
    expect(group.variables!.map((variable) => variable.name)).toEqual(['SSH_HOST', 'SSH_PASSWORD', 'SSH_PORT']);
    expect(JSON.stringify(group)).not.toContain('203.0.113.7');
    expect(JSON.stringify(group)).not.toContain(SECRET_MARKER);
    // The other group is unaffected.
    expect(openCatalogStore(path).get('g2')!.variables![0].value).toBe('198.51.100.1');
  });

  it('keeps working on a vault from an older build without ai_values or group tables', () => {
    const db = new DatabaseSync(path);
    db.exec(SCHEMA.replace(', ai_values INTEGER NOT NULL DEFAULT 1', '').replace(/CREATE TABLE group_variables[^;]*;/, ''));
    db.exec("INSERT INTO secrets (id, name, provider_id, env_key, value_type) VALUES ('g', 'Old', 'group', '', 'env_group'); INSERT INTO secret_payloads VALUES ('g', X'0A');");
    db.close();
    const group = openCatalogStore(path).get('g')!;
    expect(group.variables).toEqual([]);
  });
});

describe('plan pre-check for config groups', () => {
  const group = (variables?: string[]) => groupBindingSchema.parse({ secret_id: 'g1', ...(variables ? { variables } : {}) });

  it('accepts the whole group, a subset, and secret_ids that are groups', () => {
    seed();
    expect(precheckPlan({ bindings: [group()] }, lookup())).toBeUndefined();
    expect(precheckPlan({ bindings: [group(['SSH_HOST', 'SSH_PASSWORD'])] }, lookup())).toBeUndefined();
    expect(precheckPlan({ secretIds: ['g1'] }, lookup())).toBeUndefined();
    expect(precheckPlan({ bindings: [group(['SSH_HOST']), group()] }, lookup())).toBeUndefined();
  });

  it('rejects unknown variables and names produced by two groups', () => {
    seed();
    expect(precheckPlan({ bindings: [group(['NOPE'])] }, lookup())).toMatch(/no variable NOPE/);
    const clash = precheckPlan({ bindings: [group(['SSH_HOST']), groupBindingSchema.parse({ secret_id: 'g2' })] }, lookup());
    expect(clash).toMatch(/SSH_HOST is bound to two different sources/);
    expect(precheckPlan({ secretIds: ['g1', 'g2'] }, lookup())).toMatch(/SSH_HOST/);
    expect(precheckPlan({ secretIds: ['g1'], bindings: [groupBindingSchema.parse({ secret_id: 'g2' })] }, lookup())).toMatch(/SSH_HOST/);
  });

  it('keeps group bindings and key bindings apart', () => {
    seed();
    const keyBinding = bindingSchema.parse({ secret_id: 'k1', capability: 'chat', env: { api_key: 'OPENAI_API_KEY' } });
    expect(precheckPlan({ bindings: [keyBinding, group()] }, lookup())).toBeUndefined();
    expect(precheckPlan({ bindings: [groupBindingSchema.parse({ secret_id: 'k1' })] }, lookup())).toMatch(/not a config group/);
    const asKey = bindingSchema.parse({ secret_id: 'g1', capability: 'chat', env: { api_key: 'X' } });
    expect(precheckPlan({ bindings: [asKey] }, lookup())).toMatch(/is a config group/);
  });

  it('validates the binding shape', () => {
    expect(groupBindingSchema.safeParse({ secret_id: 'g1', variables: [] }).success).toBe(false);
    expect(groupBindingSchema.safeParse({ secret_id: 'g1', variables: ['1BAD'] }).success).toBe(false);
    expect(groupBindingSchema.safeParse({ secret_id: 'g1', capability: 'chat' }).success).toBe(false);
  });
});

describe('profiles with variable subsets', () => {
  it('lists the subset of a group and leaves whole groups alone', () => {
    seed();
    const db = new DatabaseSync(path);
    db.exec("INSERT INTO profiles VALUES ('p1', 'Deploy', '', 1); INSERT INTO profile_secrets VALUES ('p1', 'g1', 0, '[\"SSH_HOST\",\"SSH_PORT\"]'), ('p1', 'g2', 1, ''), ('p1', 'k1', 2, '');");
    db.close();
    const profile = openCatalogStore(path).profile('p1')!;
    expect(profile.secretIds).toEqual(['g1', 'g2', 'k1']);
    expect(profile.variables).toEqual({ g1: ['SSH_HOST', 'SSH_PORT'] });
    expect(JSON.stringify(profile)).not.toContain(SECRET_MARKER);
  });

  it('works on a vault whose profile_secrets has no variables column', () => {
    const db = new DatabaseSync(path);
    db.exec(SCHEMA.replace(", variables_json TEXT NOT NULL DEFAULT ''", ''));
    db.exec("INSERT INTO secrets (id, name, provider_id, env_key, value_type) VALUES ('k1', 'Key', 'openai', 'OPENAI_API_KEY', 'api_key'); INSERT INTO secret_payloads VALUES ('k1', X'0A'); INSERT INTO profiles VALUES ('p1', 'Old', '', 1); INSERT INTO profile_secrets VALUES ('p1', 'k1', 0);");
    db.close();
    const profile = openCatalogStore(path).profile('p1')!;
    expect(profile.secretIds).toEqual(['k1']);
    expect(profile.variables).toBeUndefined();
  });

  it('turns subsets into a v2 plan and plain profiles into v1', () => {
    expect(profilePlanFields({ secretIds: ['k1', 'g1'] })).toEqual({ secretIds: ['k1', 'g1'] });
    expect(profilePlanFields({ secretIds: ['k1', 'g1', 'g2'], variables: { g1: ['SSH_HOST'] } })).toEqual({
      version: 2,
      bindings: [{ secret_id: 'g1', variables: ['SSH_HOST'] }],
      secretIds: ['k1', 'g2'],
    });
    expect(profilePlanFields({ secretIds: ['g1'], variables: { g1: ['A'] } })).toEqual({ version: 2, bindings: [{ secret_id: 'g1', variables: ['A'] }] });
    // The generated plan passes the same pre-check as a hand-written one.
    seed();
    const fields = profilePlanFields({ secretIds: ['k1', 'g1'], variables: { g1: ['SSH_HOST'] } }) as { bindings: Array<{ secret_id: string; variables: string[] }>; secretIds: string[] };
    const bindings = fields.bindings.map((binding) => groupBindingSchema.parse(binding));
    expect(precheckPlan({ secretIds: fields.secretIds, bindings }, lookup())).toBeUndefined();
  });
});
