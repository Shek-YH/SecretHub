import type { CatalogSecret } from './catalog.js';
import type { CapabilityChoice, ModelProvider } from './models.js';

// Variable names suggested when the key has no model variable for the capability (same list as the project-env skill).
const SUGGESTED_MODEL_ENV: Record<string, string> = { chat: 'LLM_MODEL', code: 'CODE_MODEL', reasoning: 'LLM_MODEL', vision: 'VISION_MODEL', image_gen: 'IMAGE_MODEL', video_gen: 'VIDEO_MODEL', tts: 'TTS_MODEL', asr: 'ASR_MODEL', embedding: 'EMBEDDING_MODEL', rerank: 'RERANK_MODEL' };
const JSON_TYPE = 'application/json';

export type InvocationResult = Record<string, unknown>;

/** Body templates are generated here per kind; the catalog stores no free text. `model` is an env reference, never a value. */
function bodyFor(kind: string, model: string): { contentType: string; bodySkeleton: unknown; note?: string } {
  switch (kind) {
    case 'openai-chat': return { contentType: JSON_TYPE, bodySkeleton: { model, messages: [{ role: 'user', content: '<PROMPT>' }] } };
    case 'openai-images': return { contentType: JSON_TYPE, bodySkeleton: { model, prompt: '<PROMPT>', n: 1 } };
    case 'openai-audio-speech': return { contentType: JSON_TYPE, bodySkeleton: { model, input: '<TEXT>', voice: '<VOICE>' } };
    case 'openai-audio-transcriptions': return { contentType: 'multipart/form-data', bodySkeleton: { file: '@<AUDIO_FILE>', model } };
    case 'openai-embeddings': return { contentType: JSON_TYPE, bodySkeleton: { model, input: '<TEXT>' } };
    case 'anthropic-messages': return { contentType: JSON_TYPE, bodySkeleton: { model, max_tokens: 1024, messages: [{ role: 'user', content: '<PROMPT>' }] } };
    case 'gemini-generate': return { contentType: JSON_TYPE, bodySkeleton: { contents: [{ parts: [{ text: '<PROMPT>' }] }] }, note: 'The model is part of the request path for this API; see docsUrl.' };
    default: return { contentType: JSON_TYPE, bodySkeleton: { model, prompt: '<PROMPT>' }, note: 'Asynchronous task: POST to url, read the task id from the response, then poll pollUrl (replace {id}). Exact body fields vary by model; see docsUrl.' };
  }
}

function authFor(kind: string, protocol: string, apiKeyEnv: string): { authHeader: string; extraHeaders?: string[] } {
  if (kind === 'anthropic-messages' || protocol === 'anthropic') return { authHeader: `x-api-key: \${${apiKeyEnv}}`, extraHeaders: ['anthropic-version: 2023-06-01'] };
  // Gemini's OpenAI-compatible endpoints take Bearer; only the native generateContent API uses x-goog-api-key.
  if (kind === 'gemini-generate') return { authHeader: `x-goog-api-key: \${${apiKeyEnv}}` };
  return { authHeader: `Authorization: Bearer \${${apiKeyEnv}}` };
}

const join = (base: string, path: string) => `${base.replace(/\/+$/, '')}${path}`;

/** Metadata only: builds a call template from the bundled catalog plus the secret's endpoint and env names. Never touches a value. */
export function buildInvocation(secret: CatalogSecret, capability: string, choices: CapabilityChoice[], providers: ModelProvider[]): InvocationResult {
  const provider = providers.find((item) => item.id === secret.provider);
  const docsUrl = provider?.docsUrl;
  const native = (note: string): InvocationResult => ({ kind: 'native', providerId: secret.provider, capability, ...(docsUrl ? { docsUrl } : {}), note });
  if (!provider) return native('This provider is not in the bundled catalog, so SecretHub has no call template. Use the provider\'s own documentation.');
  const invocation = provider.invocations?.[capability];
  if (!invocation || invocation.kind === 'native') {
    return native(`SecretHub has no verified standard call template for ${capability} on ${provider.id}. ${docsUrl ? 'Follow the official documentation at docsUrl.' : 'Use the provider\'s official documentation.'}`);
  }
  const baseUrl = secret.endpointUrl || provider.baseUrl || '';
  if (!baseUrl) return native('No base URL is known for this key; set an endpoint URL in SecretHub Desktop.');

  const apiKeyEnv = secret.envKey || provider.envKeys?.[0] || 'API_KEY';
  const choice = choices.find((item) => item.secretId === secret.id && item.capability === capability);
  let modelEnv = choice?.envKey || (capability === 'chat' ? secret.modelEnvKey || provider.modelEnvKey : '') || '';
  const suggested = !modelEnv;
  if (suggested) modelEnv = SUGGESTED_MODEL_ENV[capability] ?? 'MODEL_NAME';
  const baseUrlEnv = secret.endpointEnvKey || provider.baseUrlEnvKey || '';

  const body = bodyFor(invocation.kind, `\${${modelEnv}}`);
  const notes = [body.note, suggested ? `No model variable is configured for ${capability} on this key; ${modelEnv} is a suggested name. Bind it with secrethub_prepare_project_env.` : undefined].filter(Boolean);
  return {
    kind: invocation.kind,
    providerId: provider.id,
    capability,
    method: 'POST',
    url: join(baseUrl, invocation.path),
    ...(invocation.pollPath ? { pollUrl: join(baseUrl, invocation.pollPath), pollMethod: 'GET' } : {}),
    ...authFor(invocation.kind, provider.protocol ?? 'openai', apiKeyEnv),
    contentType: body.contentType,
    bodySkeleton: body.bodySkeleton,
    envVars: { apiKey: apiKeyEnv, baseUrl: baseUrlEnv, model: modelEnv },
    ...(choice?.modelId ? { selectedModelId: choice.modelId } : {}),
    ...(docsUrl ? { docsUrl } : {}),
    ...(notes.length ? { note: notes.join(' ') } : {}),
  };
}
