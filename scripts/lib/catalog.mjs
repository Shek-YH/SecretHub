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

export const INVOCATION_KINDS = ['openai-chat', 'openai-images', 'openai-audio-speech', 'openai-audio-transcriptions', 'openai-embeddings', 'anthropic-messages', 'gemini-generate', 'async-task', 'native'];
const MAX_PATH_CHARS = 200;
const safePath = (path) => typeof path === 'string' && path.startsWith('/') && chars(path) <= MAX_PATH_CHARS && !path.includes('//') && !path.includes('..') && !path.includes(':') && !/\s/u.test(path);

/** Validates the bundled `invocations` / `docsUrl` of providers.json (never accepted from the remote overlay). Mirrors crates/providers/src/invocations.rs. */
export function validateProviderInvocations(providers) {
  const errors = [];
  for (const provider of providers) {
    const at = provider.id;
    if (provider.invocations !== undefined) {
      if (!isObject(provider.invocations)) { errors.push(`${at}: invocations must be an object`); continue; }
      for (const [capability, invocation] of Object.entries(provider.invocations)) {
        const where = `${at}.${capability}`;
        if (!CAPABILITY_VOCABULARY.includes(capability)) { errors.push(`${at}: unknown capability \`${capability}\``); continue; }
        if (!isObject(invocation)) { errors.push(`${where}: must be an object`); continue; }
        for (const key of unknownKeys(invocation, ['kind', 'path', 'pollPath'])) errors.push(`${where}: unknown field \`${key}\``);
        if (!INVOCATION_KINDS.includes(invocation.kind)) errors.push(`${where}: unknown kind \`${invocation.kind}\``);
        if (!safePath(invocation.path)) errors.push(`${where}: path is not a safe relative path`);
        if (invocation.pollPath !== undefined) {
          if (invocation.kind !== 'async-task') errors.push(`${where}: pollPath is only for async-task`);
          else if (typeof invocation.pollPath !== 'string' || !safePath(invocation.pollPath.replaceAll('{id}', 'ID'))) errors.push(`${where}: pollPath is not a safe relative path`);
        }
      }
    }
    if (provider.docsUrl !== undefined) {
      let ok = false;
      try { const url = new URL(provider.docsUrl); ok = url.protocol === 'https:' && url.hostname !== ''; } catch { /* invalid */ }
      if (!ok) errors.push(`${at}: docsUrl must be an https URL`);
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
