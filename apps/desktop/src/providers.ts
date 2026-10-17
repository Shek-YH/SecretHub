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
  baseUrlEnvKey?: string;
  modelEnvKey?: string;
  kind?: string;
  protocol?: string;
  defaultModel?: string;
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
      return { id: provider.id, name: provider.name, keyUrl: provider.keyUrl, baseUrl: provider.baseUrl, models, ...(defaultModel ? { defaultModel } : {}) };
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

export function defaultApiKeyEnvKey(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.envKeys[0] ?? '';
}

export function defaultEndpointEnvKey(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.baseUrlEnvKey || `${providerId.toUpperCase().replaceAll('-', '_')}_BASE_URL`;
}
