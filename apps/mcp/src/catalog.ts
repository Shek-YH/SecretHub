import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { aiVisibleCondition } from './visibility.js';

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
  modelId?: string;
  modelEnvKey?: string;
  endpointUrl?: string;
  endpointEnvKey?: string;
  /** Config groups only. `value` is present for plain variables of groups that allow AI values. */
  variables?: GroupVariable[];
  aiValues?: boolean;
};

export type GroupVariable = { name: string; secret: boolean; value?: string };

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
  const secrets = seed.secrets.map((secret) => ({ ...secret, tags: [...secret.tags], valueType: secret.valueType ?? 'api_key', category: secret.category ?? 'other', scope: secret.scope ?? 'global', favorite: secret.favorite ?? false, archived: secret.archived ?? false, modelId: secret.modelId ?? '', modelEnvKey: secret.modelEnvKey ?? '', endpointUrl: secret.endpointUrl ?? '', endpointEnvKey: secret.endpointEnvKey ?? '' }));
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

// Re-reads the vault on every call so edits made in SecretHub Desktop are visible
// without restarting the MCP server.
export function openCatalogStore(path: string): CatalogStore {
  const current = () => createCatalogStore(readCatalogSeed(path));
  return {
    list: (input) => current().list(input),
    get: (id) => current().get(id),
    profiles: () => current().profiles(),
    profile: (id) => current().profile(id),
    projects: () => current().projects(),
  };
}

function readCatalogSeed(path: string): CatalogSeed {
  let db: DatabaseSync;
  try { db = new DatabaseSync(path, { readOnly: true }); } catch { return { secrets: [], profiles: [], projects: [] }; }
  try {
    const secrets = (db.prepare(`SELECT s.id, s.name, s.provider_id, s.env_key, s.status, s.tags_json, s.value_type, s.category, s.scope, s.favorite, s.archived, s.model_id, s.model_env_key, s.endpoint_url, s.endpoint_env_key, EXISTS(SELECT 1 FROM secret_payloads p WHERE p.secret_id = s.id) AS has_value FROM secrets s WHERE ${aiVisibleCondition(db)} ORDER BY s.updated_at DESC`).all() as Array<Record<string, unknown>>).map((row) => ({ id: String(row.id), name: String(row.name), provider: String(row.provider_id), envKey: String(row.env_key), status: String(row.status), tags: parseTags(row.tags_json), hasValue: Number(row.has_value) === 1, valueType: String(row.value_type), category: String(row.category), scope: String(row.scope), favorite: Number(row.favorite) === 1, archived: Number(row.archived) === 1, modelId: String(row.model_id), modelEnvKey: String(row.model_env_key), endpointUrl: String(row.endpoint_url), endpointEnvKey: String(row.endpoint_env_key) }));
    // Profiles keep their own list, but ids the AI may not see are dropped from it.
    const visible = new Set(secrets.map((secret) => secret.id));
    attachGroupVariables(db, secrets);
    const profiles = (db.prepare('SELECT id, name, description FROM profiles ORDER BY updated_at DESC').all() as Array<Record<string, unknown>>).map((row) => ({ id: String(row.id), name: String(row.name), description: String(row.description), secretIds: (db.prepare('SELECT secret_id FROM profile_secrets WHERE profile_id = ? ORDER BY position').all(String(row.id)) as Array<Record<string, unknown>>).map((item) => String(item.secret_id)).filter((id) => visible.has(id)) }));
    const projects = (db.prepare('SELECT id, path, display_name, last_used_at FROM projects ORDER BY last_used_at DESC').all() as Array<Record<string, unknown>>).map((row) => ({ id: String(row.id), path: String(row.path), displayName: String(row.display_name), lastUsedAt: Number(row.last_used_at) }));
    return { secrets, profiles, projects };
  } finally { db.close(); }
}

type ProviderDefinition = { id: string; envKeys: string[]; aliases?: string[] };

// Shared with the desktop UI and Rust crates; resolves from both src/ and dist/.
const providerDefinitions: ProviderDefinition[] = JSON.parse(readFileSync(new URL('../../../packages/provider-definitions/providers.json', import.meta.url), 'utf8'));

export function recommendForStack(stack: string[], store: CatalogStore) {
  const normalized = stack.map((value) => value.toLowerCase());
  const recommended: string[] = [];
  const add = (...keys: string[]) => keys.forEach((key) => { if (!recommended.includes(key)) recommended.push(key); });
  for (const provider of providerDefinitions) {
    const keywords = [provider.id, ...(provider.aliases ?? [])];
    if (normalized.some((value) => keywords.some((keyword) => value.includes(keyword)))) add(...provider.envKeys);
  }
  const available = recommended.filter((key) => store.list({ query: key, limit: 100, offset: 0 }).items.some((secret) => secret.envKey === key));
  return { recommended, available, missing: recommended.filter((key) => !available.includes(key)) };
}

// Config groups list their variables; secret variables never carry a value, plain ones only when
// the group allows AI to read values. A vault from an older build has neither table nor column.
function attachGroupVariables(db: DatabaseSync, secrets: CatalogSecret[]) {
  const groups = secrets.filter((secret) => secret.valueType === 'env_group');
  if (groups.length === 0) return;
  let aiValues = new Map<string, boolean>();
  try { aiValues = new Map((db.prepare("SELECT id, ai_values FROM secrets WHERE value_type = 'env_group'").all() as Array<Record<string, unknown>>).map((row) => [String(row.id), Number(row.ai_values) === 1])); } catch { /* column missing: default on */ }
  for (const group of groups) {
    const allow = aiValues.get(group.id) ?? true;
    let rows: Array<Record<string, unknown>> = [];
    try { rows = db.prepare('SELECT name, secret, plain_value FROM group_variables WHERE secret_id = ? ORDER BY position').all(group.id) as Array<Record<string, unknown>>; } catch { rows = []; }
    group.aiValues = allow;
    group.variables = rows.map((row) => {
      const secret = Number(row.secret) === 1;
      return { name: String(row.name), secret, ...(!secret && allow ? { value: String(row.plain_value) } : {}) };
    });
  }
}

function parseTags(value: unknown): string[] { try { const parsed = JSON.parse(String(value)); return Array.isArray(parsed) ? parsed.map(String) : []; } catch { return []; } }
