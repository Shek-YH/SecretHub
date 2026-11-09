import { describe, expect, it } from 'vitest';
import type { CatalogSecret } from './catalog.js';
import { buildInvocation } from './invocation.js';
import type { CapabilityChoice, ModelProvider } from './models.js';

const secret = (over: Partial<CatalogSecret> = {}): CatalogSecret => ({ id: 's1', name: 'Key', provider: 'acme', envKey: 'ACME_API_KEY', status: 'valid', tags: [], hasValue: true, valueType: 'api_key', modelEnvKey: '', endpointUrl: '', endpointEnvKey: '', ...over });
const providers: ModelProvider[] = [
  { id: 'acme', baseUrl: 'https://api.acme.test/v1/', envKeys: ['ACME_API_KEY'], baseUrlEnvKey: 'ACME_BASE_URL', modelEnvKey: 'ACME_MODEL', docsUrl: 'https://docs.acme.test', invocations: {
    chat: { kind: 'openai-chat', path: '/chat/completions' },
    image_gen: { kind: 'openai-images', path: '/images/generations' },
    tts: { kind: 'openai-audio-speech', path: '/audio/speech' },
    asr: { kind: 'openai-audio-transcriptions', path: '/audio/transcriptions' },
    embedding: { kind: 'openai-embeddings', path: '/embeddings' },
    video_gen: { kind: 'async-task', path: '/tasks', pollPath: '/tasks/{id}' },
    vision: { kind: 'native', path: '/x' },
  } },
  { id: 'claude', protocol: 'anthropic', baseUrl: 'https://api.claude.test', envKeys: ['CLAUDE_API_KEY'], invocations: { chat: { kind: 'anthropic-messages', path: '/v1/messages' } } },
  { id: 'goog', protocol: 'gemini', baseUrl: 'https://g.test/v1beta/openai', envKeys: ['GEMINI_API_KEY'], invocations: { chat: { kind: 'openai-chat', path: '/chat/completions' }, reasoning: { kind: 'gemini-generate', path: '/generate' } } },
  { id: 'nodocs', baseUrl: 'https://n.test', envKeys: ['N_KEY'] },
];

describe('buildInvocation', () => {
  it('builds a chat template with env-name auth and model reference', () => {
    const result = buildInvocation(secret(), 'chat', [], providers);
    expect(result).toMatchObject({ kind: 'openai-chat', method: 'POST', url: 'https://api.acme.test/v1/chat/completions', authHeader: 'Authorization: Bearer ${ACME_API_KEY}', envVars: { apiKey: 'ACME_API_KEY', baseUrl: 'ACME_BASE_URL', model: 'ACME_MODEL' }, docsUrl: 'https://docs.acme.test' });
    expect(result.bodySkeleton).toEqual({ model: '${ACME_MODEL}', messages: [{ role: 'user', content: '<PROMPT>' }] });
    expect(result).not.toHaveProperty('pollUrl');
    expect(result).not.toHaveProperty('note');
  });

  it('generates a body per kind', () => {
    const body = (capability: string) => buildInvocation(secret(), capability, [], providers).bodySkeleton;
    expect(body('image_gen')).toEqual({ model: '${IMAGE_MODEL}', prompt: '<PROMPT>', n: 1 });
    expect(body('tts')).toEqual({ model: '${TTS_MODEL}', input: '<TEXT>', voice: '<VOICE>' });
    expect(body('asr')).toEqual({ file: '@<AUDIO_FILE>', model: '${ASR_MODEL}' });
    expect(buildInvocation(secret(), 'asr', [], providers).contentType).toBe('multipart/form-data');
    expect(body('embedding')).toEqual({ model: '${EMBEDDING_MODEL}', input: '<TEXT>' });
    expect(body('video_gen')).toEqual({ model: '${VIDEO_MODEL}', prompt: '<PROMPT>' });
  });

  it('suggests a model variable name when none is configured and says so', () => {
    const result = buildInvocation(secret(), 'image_gen', [], providers);
    expect(result.envVars).toMatchObject({ model: 'IMAGE_MODEL' });
    expect(String(result.note)).toMatch(/suggested name/);
  });

  it('uses the per-capability model variable the user chose', () => {
    const choices: CapabilityChoice[] = [{ secretId: 's1', capability: 'image_gen', modelId: 'img-1', envKey: 'MY_IMAGE_MODEL' }, { secretId: 'other', capability: 'image_gen', modelId: 'x', envKey: 'OTHER' }];
    const result = buildInvocation(secret(), 'image_gen', choices, providers);
    expect(result.envVars).toMatchObject({ model: 'MY_IMAGE_MODEL' });
    expect(result.bodySkeleton).toMatchObject({ model: '${MY_IMAGE_MODEL}' });
    expect(result.selectedModelId).toBe('img-1');
    expect(result).not.toHaveProperty('note');
  });

  it('uses the secret overrides for key, endpoint and chat model variables', () => {
    const result = buildInvocation(secret({ envKey: 'MY_KEY', endpointUrl: 'https://proxy.test/api', endpointEnvKey: 'MY_URL', modelEnvKey: 'MY_MODEL' }), 'chat', [], providers);
    expect(result).toMatchObject({ url: 'https://proxy.test/api/chat/completions', authHeader: 'Authorization: Bearer ${MY_KEY}', envVars: { apiKey: 'MY_KEY', baseUrl: 'MY_URL', model: 'MY_MODEL' } });
  });

  it('builds poll urls for async tasks and keeps the {id} placeholder', () => {
    const result = buildInvocation(secret(), 'video_gen', [], providers);
    expect(result).toMatchObject({ kind: 'async-task', url: 'https://api.acme.test/v1/tasks', pollUrl: 'https://api.acme.test/v1/tasks/{id}', pollMethod: 'GET' });
  });

  it('uses x-api-key and the version header for the anthropic protocol', () => {
    const result = buildInvocation(secret({ provider: 'claude', envKey: 'CLAUDE_API_KEY' }), 'chat', [], providers);
    expect(result).toMatchObject({ kind: 'anthropic-messages', url: 'https://api.claude.test/v1/messages', authHeader: 'x-api-key: ${CLAUDE_API_KEY}', extraHeaders: ['anthropic-version: 2023-06-01'] });
    expect(result.bodySkeleton).toMatchObject({ max_tokens: 1024 });
  });

  it('uses Bearer for gemini OpenAI-compatible and x-goog-api-key for native generate', () => {
    expect(buildInvocation(secret({ provider: 'goog', envKey: 'GEMINI_API_KEY' }), 'chat', [], providers).authHeader).toBe('Authorization: Bearer ${GEMINI_API_KEY}');
    const native = buildInvocation(secret({ provider: 'goog', envKey: 'GEMINI_API_KEY' }), 'reasoning', [], providers);
    expect(native).toMatchObject({ kind: 'gemini-generate', authHeader: 'x-goog-api-key: ${GEMINI_API_KEY}' });
    expect(native.bodySkeleton).toEqual({ contents: [{ parts: [{ text: '<PROMPT>' }] }] });
  });

  it('falls back to native with docsUrl for unlisted, native, and unknown providers', () => {
    expect(buildInvocation(secret(), 'rerank', [], providers)).toMatchObject({ kind: 'native', docsUrl: 'https://docs.acme.test' });
    expect(buildInvocation(secret(), 'vision', [], providers)).toMatchObject({ kind: 'native', docsUrl: 'https://docs.acme.test' });
    const nodocs = buildInvocation(secret({ provider: 'nodocs' }), 'chat', [], providers);
    expect(nodocs).toMatchObject({ kind: 'native' });
    expect(nodocs).not.toHaveProperty('docsUrl');
    expect(buildInvocation(secret({ provider: 'custom' }), 'chat', [], providers)).toMatchObject({ kind: 'native' });
  });

  it('never carries a secret value field', () => {
    const text = JSON.stringify(buildInvocation(secret({ tags: ['sk-live-123'] }), 'chat', [], providers));
    expect(text).not.toContain('sk-live-123');
  });
});
