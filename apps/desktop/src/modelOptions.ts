import type { BackendModel } from './lib/backend';
import type { ModelTemplate } from './providers';

export type ModelOption = { id: string; label: string; envKey: string; capabilities: string[] };
export type ModelOptionGroups = { synced: ModelOption[]; recommended: ModelOption[] };

/**
 * Model dropdown content for the secret form.
 * synced: this key's own synced models, catalog default first, then by id.
 * recommended: catalog models that are not in the synced list (all of them when nothing is synced).
 */
export function buildModelOptions(catalogModels: ModelTemplate[], syncedModels: BackendModel[], defaultModelId?: string): ModelOptionGroups {
  const envKey = catalogModels[0]?.envKey ?? 'MODEL_NAME';
  const defaultId = defaultModelId ?? catalogModels[0]?.id;
  const syncedIds = new Set(syncedModels.map((model) => model.model_id));
  const synced = syncedModels
    .map((model) => ({ id: model.model_id, label: model.display_name || model.model_id, envKey, capabilities: model.capabilities }))
    .sort((left, right) => (left.id === defaultId ? 0 : 1) - (right.id === defaultId ? 0 : 1) || left.id.localeCompare(right.id));
  const recommended = catalogModels
    .filter((model) => !syncedIds.has(model.id))
    .map((model) => ({ id: model.id, label: model.label, envKey: model.envKey, capabilities: [] }));
  return { synced, recommended };
}
