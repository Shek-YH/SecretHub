import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';
import type { CatalogSecret } from './catalog.js';
import { CAPABILITIES, type ModelProvider } from './models.js';

// Plan v2 (design §8.2): bindings carry names and ids only, never values.
export const MAX_BINDINGS = 50;
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
const envName = z.string().regex(ENV_NAME, 'must match /^[A-Za-z_][A-Za-z0-9_]*$/').max(128);

export const bindingSchema = z.object({
  secret_id: z.string().min(1).max(200),
  capability: z.enum(CAPABILITIES),
  model_id: z.string().trim().min(1).max(200).optional(),
  env: z.object({ api_key: envName.optional(), base_url: envName.optional(), model: envName.optional() }).strict(),
}).strict().superRefine((binding, context) => {
  const { api_key, base_url, model } = binding.env;
  if (!api_key && !base_url && !model) context.addIssue({ code: 'custom', path: ['env'], message: 'at least one of api_key, base_url, model is required' });
  if (model && !binding.model_id) context.addIssue({ code: 'custom', path: ['model_id'], message: 'model_id is required when env.model is set' });
});
export type Binding = z.infer<typeof bindingSchema>;

// Config group binding: no capability, model or env names; `variables` omitted means every variable.
export const groupBindingSchema = z.object({
  secret_id: z.string().min(1).max(200),
  variables: z.array(envName).min(1).max(200).optional(),
}).strict();
export type GroupBinding = z.infer<typeof groupBindingSchema>;
export type AnyBinding = Binding | GroupBinding;
const isGroupBinding = (binding: AnyBinding): binding is GroupBinding => !('env' in binding);

export type PlanLookup = { secret(id: string): CatalogSecret | undefined; providers: ModelProvider[] };

// Read-only metadata pre-check; Desktop validates again before anything is written.
export function precheckPlan(input: { secretIds?: string[]; bindings?: AnyBinding[] }, lookup: PlanLookup): string | undefined {
  const secretIds = input.secretIds ?? [];
  const bindings = input.bindings ?? [];
  if (secretIds.length === 0 && bindings.length === 0) return 'Provide secret_ids, bindings, or both.';
  if (bindings.length > MAX_BINDINGS) return `At most ${MAX_BINDINGS} bindings are allowed.`;
  const providerById = new Map(lookup.providers.map((provider) => [provider.id, provider]));
  const usable = (id: string): string | undefined => {
    const secret = lookup.secret(id);
    if (!secret) return `Unknown secret id: ${id}`;
    if (secret.archived) return `Secret ${id} is archived.`;
    if (!secret.hasValue) return `Secret ${id} has no stored value.`;
    return undefined;
  };
  for (const id of new Set([...secretIds, ...bindings.map((binding) => binding.secret_id)])) {
    const problem = usable(id);
    if (problem) return problem;
  }
  const sources = new Map<string, string>();
  const claim = (name: string, source: string): string | undefined => {
    const previous = sources.get(name);
    if (previous !== undefined && previous !== source) return `Environment variable ${name} is bound to two different sources in this plan.`;
    sources.set(name, source);
    return undefined;
  };
  // Config group variables: a name may come from one group only (the same group twice is fine).
  const groupVariables = (id: string, names?: string[]): string | undefined => {
    const group = lookup.secret(id)!;
    const known = (group.variables ?? []).map((variable) => variable.name);
    for (const name of names ?? known) {
      if (!known.includes(name)) return `Config group ${id} has no variable ${name}.`;
      const problem = claim(name, `group:${id}:${name}`);
      if (problem) return problem;
    }
    return undefined;
  };
  for (const id of secretIds) {
    if (lookup.secret(id)!.valueType === 'env_group') { const problem = groupVariables(id); if (problem) return problem; }
  }
  for (const binding of bindings) {
    const secret = lookup.secret(binding.secret_id)!;
    const isGroup = secret.valueType === 'env_group';
    if (isGroupBinding(binding)) {
      if (!isGroup) return `Secret ${binding.secret_id} is not a config group; use capability and env instead of variables.`;
      const problem = groupVariables(binding.secret_id, binding.variables);
      if (problem) return problem;
      continue;
    }
    if (isGroup) return `Secret ${binding.secret_id} is a config group; bind it with secret_id and optional variables only.`;
    const { api_key, base_url, model } = binding.env;
    if (api_key) { const problem = claim(api_key, `key:${binding.secret_id}`); if (problem) return problem; }
    if (base_url) {
      const url = secret.endpointUrl || providerById.get(secret.provider)?.baseUrl || '';
      if (!url) return `No base URL is known for secret ${binding.secret_id}; do not bind env.base_url for it.`;
      const problem = claim(base_url, `url:${url}`);
      if (problem) return problem;
    }
    if (model) { const problem = claim(model, `model:${binding.model_id}`); if (problem) return problem; }
  }
  return undefined;
}

export type PlanStatus =
  | { status: 'pending' | 'not_found' | 'declined'; requestId: string }
  | { status: 'written'; requestId: string; writtenKeys: string[]; skippedKeys: string[]; warnings: string[] }
  | { status: 'failed'; requestId: string; error: { code: string } };

// Prefix check plus a conservative character set keeps ids from escaping the plan directories.
export function isValidRequestId(id: string): boolean { return id.length <= 128 && /^plan-[A-Za-z0-9_-]+$/.test(id); }

const strings = (value: unknown): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string').slice(0, 200).map((item) => item.slice(0, 300)) : []);

export function getPlanStatus(requestId: string, planDirectory: string, resultDirectory: string): PlanStatus {
  // Desktop writes the result before removing the pending file, so the result wins.
  const resultPath = join(resultDirectory, `${requestId}.json`);
  if (existsSync(resultPath)) {
    let result: Record<string, unknown>;
    try { result = JSON.parse(readFileSync(resultPath, 'utf8')); } catch { return { status: 'failed', requestId, error: { code: 'unreadable_result' } }; }
    // Only whitelisted fields are copied out of the file.
    if (result.status === 'written') return { status: 'written', requestId, writtenKeys: strings(result.writtenKeys), skippedKeys: strings(result.skippedKeys), warnings: strings(result.warnings) };
    if (result.status === 'declined') return { status: 'declined', requestId };
    const code = typeof (result.error as { code?: unknown } | undefined)?.code === 'string' ? String((result.error as { code: string }).code).slice(0, 64) : 'unknown';
    return { status: 'failed', requestId, error: { code } };
  }
  if (existsSync(join(planDirectory, `${requestId}.json`))) return { status: 'pending', requestId };
  return { status: 'not_found', requestId };
}
