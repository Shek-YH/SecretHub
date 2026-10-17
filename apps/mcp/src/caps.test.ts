import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { z } from 'zod';
import type { ModelProvider } from './models.js';
import { isValidCapsRequestId, precheckSuggestions, queueSuggestions, suggestionSchema } from './caps.js';

const providers: ModelProvider[] = [{ id: 'openrouter' }, { id: 'deepseek' }];
const schema = z.array(suggestionSchema).min(1).max(200);

describe('suggestionSchema', () => {
  it('accepts a suggestion and dedupes capabilities', () => {
    const parsed = schema.parse([{ model_id: 'vendor/model-x', capabilities: ['chat', 'vision', 'chat'], evidence_url: 'https://docs.example/models' }]);
    expect(parsed[0].capabilities).toEqual(['chat', 'vision']);
  });
  it('accepts a suggestion without evidence', () => {
    expect(schema.safeParse([{ model_id: 'm', capabilities: ['tts'] }]).success).toBe(true);
  });
  it.each([
    ['empty capabilities', { model_id: 'm', capabilities: [] }],
    ['unknown capability', { model_id: 'm', capabilities: ['telepathy'] }],
    ['empty model id', { model_id: '  ', capabilities: ['chat'] }],
    ['long model id', { model_id: 'a'.repeat(201), capabilities: ['chat'] }],
    ['control chars in model id', { model_id: 'a\nb', capabilities: ['chat'] }],
    ['http evidence', { model_id: 'm', capabilities: ['chat'], evidence_url: 'http://docs.example' }],
    ['javascript evidence', { model_id: 'm', capabilities: ['chat'], evidence_url: 'javascript:alert(1)' }],
    ['not a url', { model_id: 'm', capabilities: ['chat'], evidence_url: 'docs' }],
    ['long evidence', { model_id: 'm', capabilities: ['chat'], evidence_url: 'https://e.example/' + 'a'.repeat(500) }],
    ['extra field', { model_id: 'm', capabilities: ['chat'], value: 'x' }],
  ])('rejects %s', (_name, item) => {
    expect(schema.safeParse([item]).success).toBe(false);
  });
  it('enforces 1 to 200 suggestions', () => {
    const one = { model_id: 'm', capabilities: ['chat'] };
    expect(schema.safeParse([]).success).toBe(false);
    expect(schema.safeParse(Array.from({ length: 200 }, (_, i) => ({ ...one, model_id: `m${i}` }))).success).toBe(true);
    expect(schema.safeParse(Array.from({ length: 201 }, (_, i) => ({ ...one, model_id: `m${i}` }))).success).toBe(false);
  });
});

describe('precheckSuggestions', () => {
  const item = (model_id: string) => schema.parse([{ model_id, capabilities: ['chat'] }])[0];
  it('requires a known provider', () => {
    expect(precheckSuggestions('nope', [item('a')], providers)).toMatch(/Unknown provider_id/);
    expect(precheckSuggestions('openrouter', [item('a')], providers)).toBeUndefined();
  });
  it('rejects duplicate model ids within one call', () => {
    expect(precheckSuggestions('openrouter', [item('a'), item('a')], providers)).toMatch(/Duplicate model_id/);
  });
});

describe('request ids and queueing', () => {
  it('validates caps ids', () => {
    expect(isValidCapsRequestId('caps-1760000000000-abc')).toBe(true);
    for (const bad of ['', 'caps', 'plan-1', 'caps-../x', 'caps-a/b', 'caps-a\\b', 'caps-a.json', `caps-${'a'.repeat(200)}`]) expect(isValidCapsRequestId(bad)).toBe(false);
  });
  it('writes only a caps file and returns the request id', () => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-caps-'));
    try {
      const requestId = queueSuggestions(directory, 'openrouter', schema.parse([{ model_id: 'm', capabilities: ['chat', 'vision'], evidence_url: 'https://docs.example/m' }]));
      expect(requestId).toMatch(/^caps-/);
      expect(readdirSync(directory)).toEqual([`${requestId}.json`]);
      const stored = JSON.parse(readFileSync(join(directory, `${requestId}.json`), 'utf8'));
      expect(stored).toMatchObject({ version: 1, requestId, kind: 'suggest_model_capabilities', providerId: 'openrouter', suggestions: [{ modelId: 'm', capabilities: ['chat', 'vision'], evidenceUrl: 'https://docs.example/m' }] });
      expect(typeof stored.createdAt).toBe('string');
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
