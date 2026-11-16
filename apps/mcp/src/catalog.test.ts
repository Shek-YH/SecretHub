import { createCatalogStore, recommendForStack, type CatalogStore } from './catalog.js';

function store(): CatalogStore {
  return createCatalogStore({
    secrets: [
      { id: 'secret-1', name: 'OpenAI Main', provider: 'OpenAI', envKey: 'OPENAI_API_KEY', status: 'valid', tags: ['ai'], hasValue: true },
      { id: 'secret-2', name: 'Supabase URL', provider: 'Supabase', envKey: 'SUPABASE_URL', status: 'unknown', tags: ['db'], hasValue: true },
    ],
    profiles: [],
    projects: [],
  });
}

describe('AI-safe catalog', () => {
  it('returns paginated metadata and never exposes a value field', () => {
    const result = store().list({ limit: 1, offset: 0 });
    expect(result.total).toBe(2);
    expect(result.items).toHaveLength(1);
    expect(result.items[0]).toEqual(expect.objectContaining({ envKey: 'OPENAI_API_KEY', hasValue: true }));
    expect(result.items[0]).not.toHaveProperty('value');
  });

  it('recommends available and missing environment keys from a project stack', () => {
    const result = recommendForStack(['Next.js', 'Supabase', 'OpenAI'], store());
    expect(result.recommended).toEqual(['OPENAI_API_KEY', 'SUPABASE_URL', 'SUPABASE_ANON_KEY']);
    expect(result.available).toEqual(['OPENAI_API_KEY', 'SUPABASE_URL']);
    expect(result.missing).toEqual(['SUPABASE_ANON_KEY']);
  });

  it('recommends keys for any provider in the shared catalog, including aliases', () => {
    expect(recommendForStack(['claude-code', 'Kimi', 'grok'], store()).recommended).toEqual(['MOONSHOT_API_KEY', 'ANTHROPIC_API_KEY', 'XAI_API_KEY']);
  });
});

describe('vault-backed catalog', () => {
  it('sees secrets added after the MCP server opened the vault', async () => {
    const { mkdtempSync, rmSync } = await import('node:fs');
    const { tmpdir } = await import('node:os');
    const { join } = await import('node:path');
    const { DatabaseSync } = await import('node:sqlite');
    const { openCatalogStore } = await import('./catalog.js');
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-mcp-'));
    const path = join(directory, 'vault.sqlite');
    const db = new DatabaseSync(path);
    db.exec(`CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT, provider_id TEXT, env_key TEXT, status TEXT, tags_json TEXT, value_type TEXT, category TEXT, scope TEXT, favorite INTEGER, archived INTEGER, model_id TEXT, model_env_key TEXT, endpoint_url TEXT, endpoint_env_key TEXT, updated_at INTEGER);
      CREATE TABLE secret_payloads (secret_id TEXT PRIMARY KEY);
      CREATE TABLE profiles (id TEXT PRIMARY KEY, name TEXT, description TEXT, updated_at INTEGER);
      CREATE TABLE profile_secrets (profile_id TEXT, secret_id TEXT, position INTEGER);
      CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT, display_name TEXT, last_used_at INTEGER);`);
    try {
      const catalog = openCatalogStore(path);
      expect(catalog.list({ limit: 10, offset: 0 }).total).toBe(0);
      db.exec(`INSERT INTO secrets VALUES ('secret-1', 'OpenAI', 'openai', 'OPENAI_API_KEY', 'valid', '[]', 'api_key', 'ai', 'global', 0, 0, '', '', '', '', 1)`);
      db.exec(`INSERT INTO secret_payloads VALUES ('secret-1')`);
      expect(catalog.get('secret-1')).toEqual(expect.objectContaining({ envKey: 'OPENAI_API_KEY', status: 'valid', hasValue: true }));
    } finally {
      db.close();
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
