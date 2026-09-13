import { DatabaseSync } from 'node:sqlite';

export type CatalogSecret = {
  id: string;
  name: string;
  provider: string;
  envKey: string;
  status: string;
  tags: string[];
  hasValue: boolean;
  valueType?: string;
  category?: string;
  scope?: string;
  favorite?: boolean;
  archived?: boolean;
};

export type CatalogProfile = { id: string; name: string; description: string; secretIds: string[] };
export type CatalogProject = { id: string; path: string; displayName: string; lastUsedAt: number };
export type CatalogSeed = { secrets: CatalogSecret[]; profiles: CatalogProfile[]; projects: CatalogProject[] };

export type CatalogStore = {
  list(input: { query?: string; limit: number; offset: number }): { total: number; count: number; offset: number; items: CatalogSecret[]; hasMore: boolean; nextOffset?: number };
  get(id: string): CatalogSecret | undefined;
  profiles(): CatalogProfile[];
  profile(id: string): CatalogProfile | undefined;
  projects(): CatalogProject[];
};

export function createCatalogStore(seed: CatalogSeed): CatalogStore {
  const secrets = seed.secrets.map((secret) => ({ ...secret, tags: [...secret.tags], valueType: secret.valueType ?? 'api_key', category: secret.category ?? 'other', scope: secret.scope ?? 'global', favorite: secret.favorite ?? false, archived: secret.archived ?? false }));
  return {
    list({ query, limit, offset }) {
      const normalized = query?.trim().toLowerCase();
      const filtered = normalized ? secrets.filter((secret) => `${secret.name} ${secret.provider} ${secret.envKey} ${secret.tags.join(' ')}`.toLowerCase().includes(normalized)) : secrets;
      const items = filtered.slice(offset, offset + limit);
      const hasMore = offset + items.length < filtered.length;
      return { total: filtered.length, count: items.length, offset, items, hasMore, ...(hasMore ? { nextOffset: offset + items.length } : {}) };
    },
    get(id) { return secrets.find((secret) => secret.id === id); },
    profiles() { return seed.profiles.map((profile) => ({ ...profile, secretIds: [...profile.secretIds] })); },
    profile(id) { const profile = seed.profiles.find((item) => item.id === id); return profile ? { ...profile, secretIds: [...profile.secretIds] } : undefined; },
    projects() { return seed.projects.map((project) => ({ ...project })); },
  };
}

export function openCatalogStore(path: string): CatalogStore {
  let db: DatabaseSync;
  try { db = new DatabaseSync(path, { readOnly: true }); } catch { return createCatalogStore({ secrets: [], profiles: [], projects: [] }); }
  try {
    const secrets = (db.prepare(`SELECT s.id, s.name, s.provider_id, s.env_key, s.status, s.tags_json, s.value_type, s.category, s.scope, s.favorite, s.archived, EXISTS(SELECT 1 FROM secret_payloads p WHERE p.secret_id = s.id) AS has_value FROM secrets s ORDER BY s.updated_at DESC`).all() as Array<Record<string, unknown>>).map((row) => ({ id: String(row.id), name: String(row.name), provider: String(row.provider_id), envKey: String(row.env_key), status: String(row.status), tags: parseTags(row.tags_json), hasValue: Number(row.has_value) === 1, valueType: String(row.value_type), category: String(row.category), scope: String(row.scope), favorite: Number(row.favorite) === 1, archived: Number(row.archived) === 1 }));
    const profiles = (db.prepare('SELECT id, name, description FROM profiles ORDER BY updated_at DESC').all() as Array<Record<string, unknown>>).map((row) => ({ id: String(row.id), name: String(row.name), description: String(row.description), secretIds: (db.prepare('SELECT secret_id FROM profile_secrets WHERE profile_id = ? ORDER BY position').all(String(row.id)) as Array<Record<string, unknown>>).map((item) => String(item.secret_id)) }));
    const projects = (db.prepare('SELECT id, path, display_name, last_used_at FROM projects ORDER BY last_used_at DESC').all() as Array<Record<string, unknown>>).map((row) => ({ id: String(row.id), path: String(row.path), displayName: String(row.display_name), lastUsedAt: Number(row.last_used_at) }));
    return createCatalogStore({ secrets, profiles, projects });
  } finally { db.close(); }
}

export function recommendForStack(stack: string[], store: CatalogStore) {
  const normalized = stack.map((value) => value.toLowerCase());
  const recommended: string[] = [];
  const add = (...keys: string[]) => keys.forEach((key) => { if (!recommended.includes(key)) recommended.push(key); });
  if (normalized.some((value) => value.includes('openai'))) add('OPENAI_API_KEY');
  if (normalized.some((value) => value.includes('anthropic') || value.includes('claude'))) add('ANTHROPIC_API_KEY');
  if (normalized.some((value) => value.includes('gemini') || value.includes('google'))) add('GEMINI_API_KEY');
  if (normalized.some((value) => value.includes('deepseek'))) add('DEEPSEEK_API_KEY');
  if (normalized.some((value) => value.includes('xai') || value.includes('grok'))) add('XAI_API_KEY');
  if (normalized.some((value) => value.includes('openrouter'))) add('OPENROUTER_API_KEY');
  if (normalized.some((value) => value.includes('github'))) add('GITHUB_TOKEN');
  if (normalized.some((value) => value.includes('supabase'))) add('SUPABASE_URL', 'SUPABASE_ANON_KEY');
  if (normalized.some((value) => value.includes('cloudflare'))) add('CLOUDFLARE_API_TOKEN');
  const available = recommended.filter((key) => store.list({ query: key, limit: 100, offset: 0 }).items.some((secret) => secret.envKey === key));
  return { recommended, available, missing: recommended.filter((key) => !available.includes(key)) };
}

function parseTags(value: unknown): string[] { try { const parsed = JSON.parse(String(value)); return Array.isArray(parsed) ? parsed.map(String) : []; } catch { return []; } }
