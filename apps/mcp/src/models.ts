import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { dirname, join } from 'node:path';

// Fixed vocabulary shared with Rust and the skill (design §4).
export const CAPABILITIES = ['chat', 'code', 'reasoning', 'vision', 'image_gen', 'video_gen', 'tts', 'asr', 'embedding', 'rerank'] as const;
export type Capability = (typeof CAPABILITIES)[number];

export type CatalogModelEntry = { id: string; label?: string; capabilities?: string[]; deprecated?: boolean };
export type ModelProvider = { id: string; kind?: string; protocol?: string; baseUrl?: string; envKeys?: string[]; baseUrlEnvKey?: string; defaultModel?: string; models?: CatalogModelEntry[] };

export type KeyRow = {
  secretId: string;
  secretName: string;
  providerId: string;
  keyStatus: string;
  endpointUrl: string;
  envKey: string;
  endpointEnvKey: string;
  hasValue: boolean;
  archived: boolean;
  valueType: string;
  modelsSyncedAt: number;
  modelsSyncStatus: string;
};

// One synced provider model; capabilities/source already reflect any user/AI override.
export type ModelRow = { secretId: string; modelId: string; capabilities: string[]; capabilitySource: string };

// A model the user picked for one capability of a key (secret_capability_models); absent in old vaults.
export type CapabilityChoice = { secretId: string; capability: string; modelId: string; envKey: string };

export type ModelSnapshot = { keys: KeyRow[]; models: ModelRow[]; capabilityModels?: CapabilityChoice[] };

export type Candidate = {
  secretId: string;
  secretName: string;
  providerId: string;
  providerKind: string;
  protocol: string;
  baseUrl: string;
  apiKeyEnvKey: string;
  baseUrlEnvKey: string;
  modelId: string;
  capabilities: string[];
  capabilitySource: string;
  keyStatus: string;
  modelsSyncedAt: number;
  assumed: boolean;
  /** The user chose this model for the capability in SecretHub Desktop; capabilitySource is then `user`. */
  preferred?: true;
  /** Variable name the user entered for a preferred model; use it instead of inventing one. */
  modelEnvKey?: string;
  reason?: string;
};

export type FindModelsInput = { capabilities: Capability[]; provider?: string; includeUnverified?: boolean; limit?: number };
export type UnsyncedKey = { secretId: string; secretName: string; providerId: string; keyStatus: string; modelsSyncStatus: string };
export type FindModelsResult = {
  results: Array<{ capability: Capability; candidates: Candidate[] }>;
  uncovered: Capability[];
  unsyncedKeys: UnsyncedKey[];
  truncated?: boolean;
};

const SOURCE_RANK: Record<string, number> = { user: 0, ai: 1, provider: 2, catalog: 3, heuristic: 4, unknown: 5 };
const sourceRank = (source: string) => SOURCE_RANK[source] ?? SOURCE_RANK.unknown;
const UNSYNCED_LIMIT = 20;

export function findModels(input: FindModelsInput, snapshot: ModelSnapshot, providers: ModelProvider[]): FindModelsResult {
  const limit = input.limit ?? 20;
  const capabilities = [...new Set(input.capabilities)];
  const providerById = new Map(providers.map((provider) => [provider.id, provider]));
  const usable = (key: KeyRow) => key.hasValue && !key.archived && key.valueType === 'api_key' && (!input.provider || key.providerId === input.provider);
  const eligible = (key: KeyRow) => usable(key) && (input.includeUnverified || key.keyStatus === 'valid');
  const keys = new Map(snapshot.keys.filter(eligible).map((key) => [key.secretId, key]));
  const rows = snapshot.models.filter((row) => keys.has(row.secretId));
  const choices = new Map((snapshot.capabilityModels ?? []).map((choice) => [`${choice.secretId}|${choice.capability}`, choice]));
  const chosen = (row: ModelRow, capability: Capability) => choices.get(`${row.secretId}|${capability}`)?.modelId === row.modelId;

  // A row matches a capability for real, or (chat only) as an assumed id-only gateway model.
  const matches = (row: ModelRow, capability: Capability): false | 'real' | 'assumed' => {
    if (row.capabilities.includes(capability) || chosen(row, capability)) return 'real';
    if (capability === 'chat' && row.capabilities.length === 0 && row.capabilitySource === 'unknown') return 'assumed';
    return false;
  };

  // Rule 4: how many requested capabilities a secret covers across all its models.
  const coverage = new Map<string, number>();
  for (const capability of capabilities) {
    for (const id of new Set(rows.filter((row) => matches(row, capability)).map((row) => row.secretId))) coverage.set(id, (coverage.get(id) ?? 0) + 1);
  }

  const results = capabilities.map((capability) => {
    const candidates: Array<Candidate & { cover: number }> = [];
    for (const row of rows) {
      const match = matches(row, capability);
      if (!match) continue;
      const key = keys.get(row.secretId)!;
      const provider = providerById.get(key.providerId);
      const candidate: Candidate & { cover: number } = {
        secretId: key.secretId,
        secretName: key.secretName,
        providerId: key.providerId,
        providerKind: provider?.kind ?? 'direct',
        protocol: provider?.protocol ?? 'openai',
        baseUrl: key.endpointUrl || provider?.baseUrl || '',
        apiKeyEnvKey: key.envKey || provider?.envKeys?.[0] || '',
        baseUrlEnvKey: key.endpointEnvKey || provider?.baseUrlEnvKey || '',
        modelId: row.modelId,
        capabilities: [...row.capabilities],
        capabilitySource: row.capabilitySource,
        keyStatus: key.keyStatus,
        modelsSyncedAt: key.modelsSyncedAt,
        assumed: match === 'assumed',
        cover: coverage.get(key.secretId) ?? 0,
      };
      if (chosen(row, capability)) {
        candidate.preferred = true;
        candidate.capabilitySource = 'user';
        const envKey = choices.get(`${row.secretId}|${capability}`)!.envKey;
        if (envKey) candidate.modelEnvKey = envKey;
      }
      if (key.keyStatus !== 'valid') candidate.reason = `key status: ${key.keyStatus || 'unknown'}`;
      candidates.push(candidate);
    }
    candidates.sort((a, b) =>
      Number(Boolean(b.preferred)) - Number(Boolean(a.preferred))
      || Number(a.assumed) - Number(b.assumed)
      || Number(b.keyStatus === 'valid') - Number(a.keyStatus === 'valid')
      || sourceRank(a.capabilitySource) - sourceRank(b.capabilitySource)
      || b.cover - a.cover
      || Number(b.providerKind === 'aggregator') - Number(a.providerKind === 'aggregator')
      || b.modelsSyncedAt - a.modelsSyncedAt
      || compare(a.modelId, b.modelId)
      || compare(a.secretId, b.secretId));
    return { capability, candidates: candidates.slice(0, limit).map(({ cover: _cover, ...rest }) => rest) };
  });

  // Listed regardless of key status: an unchecked key is exactly what the user must validate/sync.
  const unsyncedKeys = snapshot.keys.filter(usable)
    .filter((key) => key.modelsSyncStatus !== 'unsupported' && (key.modelsSyncedAt === 0 || key.modelsSyncStatus === 'failed'))
    .filter((key) => Boolean(key.endpointUrl || providerById.get(key.providerId)?.baseUrl))
    .slice(0, UNSYNCED_LIMIT)
    .map((key) => ({ secretId: key.secretId, secretName: key.secretName, providerId: key.providerId, keyStatus: key.keyStatus, modelsSyncStatus: key.modelsSyncStatus }));

  return { results, uncovered: results.filter((entry) => entry.candidates.length === 0).map((entry) => entry.capability), unsyncedKeys };
}

// Trims candidates evenly (drops the last one of the longest list) until the JSON fits the budget.
export function fitToLimit(result: FindModelsResult, budget: number): FindModelsResult {
  const copy: FindModelsResult = { ...result, results: result.results.map((entry) => ({ ...entry, candidates: [...entry.candidates] })) };
  while (JSON.stringify(copy, null, 2).length > budget) {
    const longest = copy.results.reduce((best, entry) => (entry.candidates.length > best.candidates.length ? entry : best), copy.results[0]);
    if (!longest || longest.candidates.length === 0) break;
    longest.candidates.pop();
    copy.truncated = true;
  }
  return copy;
}

// Re-reads the vault on every call; tolerates vaults created before model sync existed.
export function readModelSnapshot(path: string): ModelSnapshot {
  let db: DatabaseSync;
  try { db = new DatabaseSync(path, { readOnly: true }); } catch { return { keys: [], models: [] }; }
  try {
    const base = 's.id, s.name, s.provider_id, s.status, s.env_key, s.endpoint_url, s.endpoint_env_key, s.value_type, s.archived, EXISTS(SELECT 1 FROM secret_payloads p WHERE p.secret_id = s.id) AS has_value';
    let secretRows: Array<Record<string, unknown>>;
    try { secretRows = db.prepare(`SELECT ${base}, s.models_synced_at, s.models_sync_status FROM secrets s`).all() as Array<Record<string, unknown>>; }
    catch {
      try { secretRows = db.prepare(`SELECT ${base}, 0 AS models_synced_at, '' AS models_sync_status FROM secrets s`).all() as Array<Record<string, unknown>>; }
      catch { return { keys: [], models: [] }; }
    }
    const keys: KeyRow[] = secretRows.map((row) => ({
      secretId: String(row.id), secretName: String(row.name), providerId: String(row.provider_id), keyStatus: String(row.status),
      endpointUrl: String(row.endpoint_url ?? ''), envKey: String(row.env_key ?? ''), endpointEnvKey: String(row.endpoint_env_key ?? ''),
      hasValue: Number(row.has_value) === 1, archived: Number(row.archived) === 1, valueType: String(row.value_type),
      modelsSyncedAt: Number(row.models_synced_at ?? 0), modelsSyncStatus: String(row.models_sync_status ?? ''),
    }));
    // Same semantics as Rust list_provider_models: an override keyed by (provider, model) wins for capabilities and source.
    const select = 'm.secret_id, m.model_id, COALESCE(o.capabilities_json, m.capabilities_json) AS caps, COALESCE(o.source, m.capability_source) AS source';
    let modelRows: Array<Record<string, unknown>> = [];
    try { modelRows = db.prepare(`SELECT ${select} FROM provider_models m JOIN secrets s ON s.id = m.secret_id LEFT JOIN model_capability_overrides o ON o.provider_id = s.provider_id AND o.model_id = m.model_id`).all() as Array<Record<string, unknown>>; }
    catch {
      try { modelRows = db.prepare('SELECT m.secret_id, m.model_id, m.capabilities_json AS caps, m.capability_source AS source FROM provider_models m').all() as Array<Record<string, unknown>>; }
      catch { modelRows = []; }
    }
    let capabilityModels: CapabilityChoice[] = [];
    try { capabilityModels = (db.prepare('SELECT secret_id, capability, model_id, env_key FROM secret_capability_models').all() as Array<Record<string, unknown>>).map((row) => ({ secretId: String(row.secret_id), capability: String(row.capability), modelId: String(row.model_id), envKey: String(row.env_key ?? '') })); } catch { capabilityModels = []; }
    const models = modelRows.map((row) => ({ secretId: String(row.secret_id), modelId: String(row.model_id), capabilities: parseList(row.caps), capabilitySource: String(row.source) }));
    return { keys, models, capabilityModels };
  } finally { db.close(); }
}

// Shared with the desktop UI and Rust crates; resolves from both src/ and dist/.
/** Desktop writes the effective model lists (bundled + verified remote overlay) next to the vault. */
export function effectiveModelsPath(dbPath: string): string {
  return process.env.SECRETHUB_EFFECTIVE_MODELS_PATH ?? join(dirname(dbPath), 'catalog', 'effective-models.json');
}

// Only model lists and the default come from the effective file; URLs, protocols and env names stay bundled.
// The file lives in the same local directory as the vault, so it is trusted like the vault and not re-verified.
export function loadModelProviders(effectivePath?: string): ModelProvider[] {
  const bundled: ModelProvider[] = JSON.parse(readFileSync(new URL('../../../packages/provider-definitions/providers.json', import.meta.url), 'utf8'));
  if (!effectivePath) return bundled;
  let effective: Record<string, { defaultModel?: unknown; models?: unknown }>;
  try {
    const parsed = JSON.parse(readFileSync(effectivePath, 'utf8'));
    if (!parsed || typeof parsed.providers !== 'object' || parsed.providers === null) return bundled;
    effective = parsed.providers;
  } catch { return bundled; }
  return bundled.map((provider) => {
    const entry = effective[provider.id];
    if (!entry || !Array.isArray(entry.models)) return provider;
    const models = entry.models.filter((model): model is CatalogModelEntry => typeof model === 'object' && model !== null && typeof (model as CatalogModelEntry).id === 'string');
    return { ...provider, models, ...(typeof entry.defaultModel === 'string' ? { defaultModel: entry.defaultModel } : {}) };
  });
}

function compare(a: string, b: string) { return a < b ? -1 : a > b ? 1 : 0; }
function parseList(value: unknown): string[] { try { const parsed = JSON.parse(String(value)); return Array.isArray(parsed) ? parsed.map(String) : []; } catch { return []; } }
