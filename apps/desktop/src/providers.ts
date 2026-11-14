import providerDefinitions from '../../../packages/provider-definitions/providers.json';

export type ModelTemplate = {
  id: string;
  label: string;
  envKey: string;
};

export type ProviderTemplate = {
  id: string;
  name: string;
  keyUrl: string;
  baseUrl: string;
  models: ModelTemplate[];
  defaultModel?: string;
  /** Shown in the "common" group of provider pickers. */
  featured: boolean;
  keyPrefixes: string[];
};

/** Models-only view returned by the `catalog_effective` command (no URLs, no env names). */
export type EffectiveModels = {
  serial: number;
  publishedAt: string;
  source: 'bundled' | 'remote';
  providers: Record<string, { defaultModel?: string; models: Array<{ id: string; label?: string; capabilities?: string[]; deprecated?: boolean }> }>;
};

type ProviderDefinition = {
  id: string;
  name: string;
  keyUrl: string;
  baseUrl: string;
  envKeys: string[];
  featured?: boolean;
  keyPrefixes?: string[];
  baseUrlEnvKey?: string;
  modelEnvKey?: string;
  kind?: string;
  protocol?: string;
  defaultModel?: string;
  /** Bundled call templates for MCP clients; not used by the UI. */
  invocations?: Record<string, { kind: string; path: string; pollPath?: string } | undefined>;
  docsUrl?: string;
  models: Array<{ id: string; label?: string; capabilities?: string[]; deprecated?: boolean }>;
};

const definitions: ProviderDefinition[] = providerDefinitions;

// The shared catalog lives in packages/provider-definitions/providers.json and is
// also read by the Rust crates and the MCP server. Its model baseline mirrors
// EchoBird's modelDirectory.json; dynamic gateways still retain the custom option.
// Only providers with model templates are offered as AI presets. Model lists may be replaced
// by the effective catalog (see applyEffectiveCatalog); URLs and env names always stay bundled.
function buildTemplates(effective?: EffectiveModels): ProviderTemplate[] {
  return definitions
    .map((provider) => {
      const override = effective?.providers[provider.id];
      const defaultModel = override ? override.defaultModel : provider.defaultModel;
      const models = (override ? override.models : provider.models).filter((model) => !model.deprecated)
        .map((model) => ({ id: model.id, label: model.label ?? model.id, envKey: provider.modelEnvKey ?? 'MODEL_NAME' }));
      return { id: provider.id, name: provider.name, keyUrl: provider.keyUrl, baseUrl: provider.baseUrl, models, featured: provider.featured === true, keyPrefixes: provider.keyPrefixes ?? [], ...(defaultModel ? { defaultModel } : {}) };
    })
    .filter((provider) => provider.models.length > 0);
}

// Mutated in place so existing imports keep seeing the latest catalog after a re-render.
export const providerTemplates: ProviderTemplate[] = buildTemplates();

export function applyEffectiveCatalog(effective: EffectiveModels) {
  providerTemplates.splice(0, providerTemplates.length, ...buildTemplates(effective));
}

export function providerTemplate(id: string): ProviderTemplate {
  return providerTemplates.find((provider) => provider.id === id) ?? providerTemplates.find((provider) => provider.id === 'custom')!;
}

export function providerKind(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.kind ?? 'direct';
}

export function defaultApiKeyEnvKey(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.envKeys[0] ?? '';
}

export function defaultEndpointEnvKey(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.baseUrlEnvKey || `${providerId.toUpperCase().replaceAll('-', '_')}_BASE_URL`;
}

/** Key prefixes shorter than this are too generic to pick a provider automatically (for example "sk-"). */
export const MIN_AUTO_PREFIX_LENGTH = 4;

export type PrefixDetection<T> = {
  /** Provider selected automatically; null when nothing matched uniquely. */
  match: T | null;
  /** A prefix matched but was too short or shared, so the user has to choose. */
  ambiguous: boolean;
};

/** Longest matching key prefix wins; prefixes shorter than MIN_AUTO_PREFIX_LENGTH never select a provider. */
export function detectProviderByPrefix<T extends { keyPrefixes?: string[] }>(value: string, providers: T[]): PrefixDetection<T> {
  const key = value.trim();
  if (!key) return { match: null, ambiguous: false };
  let bestLength = 0;
  let best: T[] = [];
  for (const provider of providers) {
    for (const prefix of provider.keyPrefixes ?? []) {
      if (!prefix || !key.startsWith(prefix)) continue;
      if (prefix.length > bestLength) { bestLength = prefix.length; best = [provider]; }
      else if (prefix.length === bestLength && !best.includes(provider)) best.push(provider);
    }
  }
  if (best.length === 0) return { match: null, ambiguous: false };
  if (bestLength < MIN_AUTO_PREFIX_LENGTH || best.length > 1) return { match: null, ambiguous: true };
  return { match: best[0], ambiguous: false };
}

/** Resolves a stored provider reference (catalog id or display name) to a catalog id. */
export function resolveProviderId(reference: string): string | undefined {
  const lowered = reference.trim().toLowerCase();
  return providerTemplates.find((item) => item.id.toLowerCase() === lowered || item.name.toLowerCase() === lowered)?.id;
}

export type ProviderGroups<T extends { id: string; name: string; featured: boolean }> = { common: T[]; more: T[]; searching: boolean };

/** Common = featured providers plus providers already used by active credentials, with "custom" always last. */
export function groupProviders<T extends { id: string; name: string; featured: boolean }>(providers: T[], usedProviderIds: Iterable<string>, query = ''): ProviderGroups<T> {
  const used = new Set(usedProviderIds);
  const needle = query.trim().toLowerCase();
  const matches = (provider: T) => !needle || `${provider.name} ${provider.id}`.toLowerCase().includes(needle);
  const isCommon = (provider: T) => provider.id !== 'custom' && (provider.featured || used.has(provider.id));
  const custom = providers.filter((provider) => provider.id === 'custom');
  const common = [...providers.filter(isCommon), ...custom].filter(matches);
  const more = providers.filter((provider) => provider.id !== 'custom' && !isCommon(provider)).filter(matches);
  return { common, more, searching: needle.length > 0 };
}

/** "Name", then "Name 2", "Name 3" ... avoiding collisions with existing credential names (case-insensitive). */
export function uniqueCredentialName(base: string, existing: Iterable<string>): string {
  const taken = new Set(Array.from(existing, (name) => name.trim().toLowerCase()));
  if (!taken.has(base.trim().toLowerCase())) return base;
  for (let index = 2; ; index += 1) {
    const candidate = `${base} ${index}`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
}
