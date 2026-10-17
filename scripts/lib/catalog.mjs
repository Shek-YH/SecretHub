// Shared rules for model-catalog.json. Mirrors crates/providers/src/catalog.rs;
// keep both in sync (the field whitelist and limits are part of the security design).
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const definitionsDir = join(repoRoot, 'packages', 'provider-definitions');
export const CAPABILITY_VOCABULARY = ['chat', 'code', 'reasoning', 'vision', 'image_gen', 'video_gen', 'tts', 'asr', 'embedding', 'rerank'];
export const MAX_CATALOG_BYTES = 1024 * 1024;
export const MAX_MODELS_PER_PROVIDER = 300;
const MAX_ID_CHARS = 200;
const MAX_LABEL_CHARS = 200;

const TOP_KEYS = ['schema', 'serial', 'publishedAt', 'providers'];
const PROVIDER_KEYS = ['defaultModel', 'models'];
const MODEL_KEYS = ['id', 'label', 'capabilities', 'deprecated'];
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/u;

const isObject = (value) => typeof value === 'object' && value !== null && !Array.isArray(value);
const unknownKeys = (value, allowed) => Object.keys(value).filter((key) => !allowed.includes(key));
const chars = (value) => [...value].length;

/** Returns a list of problems; empty means valid. `providerIds` are the bundled provider ids. */
export function validateCatalog(text, providerIds) {
  const errors = [];
  if (Buffer.byteLength(text, 'utf8') > MAX_CATALOG_BYTES) return ['file exceeds 1 MB'];
  let data;
  try { data = JSON.parse(text); } catch (error) { return [`invalid JSON: ${error.message}`]; }
  if (!isObject(data)) return ['top level must be an object'];
  for (const key of unknownKeys(data, TOP_KEYS)) errors.push(`unknown field \`${key}\``);
  if (data.schema !== 1) errors.push('schema must be 1');
  if (!Number.isSafeInteger(data.serial) || data.serial <= 0) errors.push('serial must be a positive integer');
  if (typeof data.publishedAt !== 'string' || !/^\d{4}-\d{2}-\d{2}T[\d:.]+(Z|[+-]\d{2}:\d{2})$/.test(data.publishedAt) || Number.isNaN(Date.parse(data.publishedAt))) errors.push('publishedAt must be an ISO-8601 timestamp');
  if (!isObject(data.providers)) { errors.push('providers must be an object'); return errors; }
  for (const [providerId, provider] of Object.entries(data.providers)) {
    const at = `${providerId}`;
    if (!providerIds.includes(providerId)) errors.push(`${at}: provider is not in providers.json`);
    if (!isObject(provider)) { errors.push(`${at}: must be an object`); continue; }
    for (const key of unknownKeys(provider, PROVIDER_KEYS)) errors.push(`${at}: unknown field \`${key}\``);
    if (!Array.isArray(provider.models) || provider.models.length === 0) { errors.push(`${at}: models must be a non-empty array`); continue; }
    if (provider.models.length > MAX_MODELS_PER_PROVIDER) errors.push(`${at}: more than ${MAX_MODELS_PER_PROVIDER} models`);
    const seen = new Set();
    for (const model of provider.models) {
      if (!isObject(model)) { errors.push(`${at}: model must be an object`); continue; }
      for (const key of unknownKeys(model, MODEL_KEYS)) errors.push(`${at}: unknown model field \`${key}\``);
      if (typeof model.id !== 'string' || chars(model.id) < 1 || chars(model.id) > MAX_ID_CHARS || CONTROL.test(model.id)) { errors.push(`${at}: invalid model id`); continue; }
      if (seen.has(model.id)) errors.push(`${at}: duplicate model id \`${model.id}\``);
      seen.add(model.id);
      if (model.label !== undefined && (typeof model.label !== 'string' || chars(model.label) > MAX_LABEL_CHARS || CONTROL.test(model.label))) errors.push(`${at}: invalid label for \`${model.id}\``);
      if (model.capabilities !== undefined) {
        if (!Array.isArray(model.capabilities)) errors.push(`${at}: capabilities of \`${model.id}\` must be an array`);
        else for (const cap of model.capabilities) if (!CAPABILITY_VOCABULARY.includes(cap)) errors.push(`${at}: unknown capability \`${cap}\` on \`${model.id}\``);
      }
      if (model.deprecated !== undefined && typeof model.deprecated !== 'boolean') errors.push(`${at}: deprecated of \`${model.id}\` must be a boolean`);
    }
    if (provider.defaultModel !== undefined) {
      const usable = provider.models.some((model) => isObject(model) && model.id === provider.defaultModel && model.deprecated !== true);
      if (typeof provider.defaultModel !== 'string' || !usable) errors.push(`${at}: defaultModel must be a listed, non-deprecated model`);
    }
  }
  return errors;
}

export function bundledProviders() {
  return JSON.parse(readFileSync(join(definitionsDir, 'providers.json'), 'utf8'));
}

export function bundledSerial() {
  return JSON.parse(readFileSync(join(definitionsDir, 'catalog-serial.json'), 'utf8'));
}
