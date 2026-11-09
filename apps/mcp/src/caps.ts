import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';
import { CAPABILITIES, type ModelProvider } from './models.js';

// AI capability suggestions (design §7): queued as caps-*.json, never written to SQLite.
export const MAX_SUGGESTIONS = 200;

const httpsUrl = z.string().max(500).refine((value) => { try { return new URL(value).protocol === 'https:'; } catch { return false; } }, 'must be an https URL');

export const suggestionSchema = z.object({
  model_id: z.string().trim().min(1).max(200).refine((value) => !/[\u0000-\u001f\u007f]/.test(value), 'must not contain control characters'),
  capabilities: z.array(z.enum(CAPABILITIES)).min(1).max(CAPABILITIES.length).transform((items) => [...new Set(items)]),
  evidence_url: httpsUrl.optional(),
}).strict();
export type Suggestion = z.infer<typeof suggestionSchema>;

// Returns a problem description, or undefined when the suggestions can be queued.
export function precheckSuggestions(providerId: string, suggestions: Suggestion[], providers: ModelProvider[]): string | undefined {
  if (!providers.some((provider) => provider.id === providerId)) return `Unknown provider_id: ${providerId}`;
  const seen = new Set<string>();
  for (const suggestion of suggestions) {
    if (seen.has(suggestion.model_id)) return `Duplicate model_id in one call: ${suggestion.model_id}`;
    seen.add(suggestion.model_id);
  }
  return undefined;
}

// Same prefix-plus-charset rule as plan ids, with the caps- prefix.
export function isValidCapsRequestId(id: string): boolean { return id.length <= 128 && /^caps-[A-Za-z0-9_-]+$/.test(id); }

export function queueSuggestions(directory: string, providerId: string, suggestions: Suggestion[]): string {
  const requestId = `caps-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = {
    version: 1,
    requestId,
    kind: 'suggest_model_capabilities',
    providerId,
    suggestions: suggestions.map((item) => ({ modelId: item.model_id, capabilities: item.capabilities, ...(item.evidence_url ? { evidenceUrl: item.evidence_url } : {}) })),
    createdAt: new Date().toISOString(),
  };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}
