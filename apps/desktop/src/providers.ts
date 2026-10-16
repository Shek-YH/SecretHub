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
};

type ProviderDefinition = {
  id: string;
  name: string;
  keyUrl: string;
  baseUrl: string;
  envKeys: string[];
  baseUrlEnvKey?: string;
  modelEnvKey?: string;
  models: Array<{ id: string; label?: string }>;
};

const definitions: ProviderDefinition[] = providerDefinitions;

// The shared catalog lives in packages/provider-definitions/providers.json and is
// also read by the Rust crates and the MCP server. Its model baseline mirrors
// EchoBird's modelDirectory.json; dynamic gateways still retain the custom option.
// Only providers with model templates are offered as AI presets.
export const providerTemplates: ProviderTemplate[] = definitions
  .filter((provider) => provider.models.length > 0)
  .map((provider) => ({
    id: provider.id,
    name: provider.name,
    keyUrl: provider.keyUrl,
    baseUrl: provider.baseUrl,
    models: provider.models.map((model) => ({ id: model.id, label: model.label ?? model.id, envKey: provider.modelEnvKey ?? 'MODEL_NAME' })),
  }));

export function providerTemplate(id: string): ProviderTemplate {
  return providerTemplates.find((provider) => provider.id === id) ?? providerTemplates.find((provider) => provider.id === 'custom')!;
}

export function defaultApiKeyEnvKey(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.envKeys[0] ?? '';
}

export function defaultEndpointEnvKey(providerId: string): string {
  return definitions.find((provider) => provider.id === providerId)?.baseUrlEnvKey || `${providerId.toUpperCase().replaceAll('-', '_')}_BASE_URL`;
}
