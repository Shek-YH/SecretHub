import { describe, expect, it } from 'vitest';
import { applyEffectiveCatalog, detectProviderByPrefix, groupProviders, providerTemplate, providerTemplates, uniqueCredentialName } from './providers';

describe('provider model templates', () => {
  it('contains unique providers with safe key pages and model env mappings', () => {
    expect(new Set(providerTemplates.map((provider) => provider.id)).size).toBe(providerTemplates.length);
    for (const provider of providerTemplates) {
      expect(provider.id === 'custom' || provider.keyUrl.startsWith('https://')).toBe(true);
      expect(provider.models.length).toBeGreaterThan(0);
      for (const model of provider.models) {
        expect(model.id).toBeTruthy();
        expect(model.envKey).toMatch(/^[A-Z][A-Z0-9_]*$/);
      }
    }
  });

  it('keeps a custom provider option without pretending to know its endpoint', () => {
    const custom = providerTemplates.find((provider) => provider.id === 'custom');
    expect(custom?.models[0].id).toBe('custom-model');
    expect(custom?.baseUrl).toBe('');
  });

  it('mirrors the expanded EchoBird directory and removes retired DeepSeek aliases', () => {
    expect(providerTemplates.length).toBeGreaterThanOrEqual(30);
    const deepseek = providerTemplates.find((provider) => provider.id === 'deepseek');
    expect(deepseek?.models.map((model) => model.id)).toEqual(['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-v4-flash-vision-exp']);
    expect(providerTemplates.flatMap((provider) => provider.models).map((model) => model.id)).not.toContain('deepseek-chat');
    expect(providerTemplates.flatMap((provider) => provider.models).map((model) => model.id)).not.toContain('deepseek-reasoner');
    expect(providerTemplates.find((provider) => provider.id === 'hunyuan')?.models.map((model) => model.id)).not.toContain('hunyuan-t1-latest');
  });
});

describe('effective catalog', () => {
  const none = { serial: 1, publishedAt: '2026-10-17T00:00:00Z', source: 'bundled' as const, providers: {} };
  afterEach(() => applyEffectiveCatalog(none));

  it('replaces model lists, hides deprecated models and keeps bundled URLs', () => {
    const before = providerTemplate('deepseek');
    const baseUrl = before.baseUrl;
    const keyUrl = before.keyUrl;
    applyEffectiveCatalog({ ...none, source: 'remote', providers: { deepseek: { defaultModel: 'ds-new', models: [{ id: 'ds-old', deprecated: true }, { id: 'ds-new', label: 'DS New' }] }, unknownVendor: { models: [{ id: 'x' }] } } });
    const after = providerTemplate('deepseek');
    expect(after.models.map((model) => model.id)).toEqual(['ds-new']);
    expect(after.models[0]).toEqual({ id: 'ds-new', label: 'DS New', envKey: 'DEEPSEEK_MODEL' });
    expect(after.defaultModel).toBe('ds-new');
    expect(after.baseUrl).toBe(baseUrl);
    expect(after.keyUrl).toBe(keyUrl);
    expect(providerTemplates.some((provider) => provider.id === 'unknownVendor')).toBe(false);
    expect(providerTemplate('openai').models.length).toBeGreaterThan(0);
  });

  it('restores the bundled list when the overlay is withdrawn', () => {
    applyEffectiveCatalog({ ...none, providers: { deepseek: { models: [{ id: 'only-one' }] } } });
    applyEffectiveCatalog(none);
    expect(providerTemplate('deepseek').models.map((model) => model.id)).toContain('deepseek-v4-pro');
  });
});

describe('detectProviderByPrefix', () => {
  it('detects providers by distinctive prefixes, longest prefix first', () => {
    expect(detectProviderByPrefix('sk-ant-FAKE-KEY', providerTemplates).match?.id).toBe('anthropic');
    expect(detectProviderByPrefix('sk-or-FAKE-KEY', providerTemplates).match?.id).toBe('openrouter');
    expect(detectProviderByPrefix('sk-deep-FAKE-KEY', providerTemplates).match?.id).toBe('deepseek');
    expect(detectProviderByPrefix('AIzaFAKEKEY', providerTemplates).match?.id).toBe('gemini');
    const custom = [{ id: 'short', keyPrefixes: ['sk-'] }, { id: 'long', keyPrefixes: ['sk-abcd-'] }];
    expect(detectProviderByPrefix('sk-abcd-1', custom).match?.id).toBe('long');
  });

  it('never selects a provider from the generic "sk-" prefix and says it is ambiguous', () => {
    expect(detectProviderByPrefix('sk-FAKE-KEY', providerTemplates)).toEqual({ match: null, ambiguous: true });
    expect(detectProviderByPrefix('plain-value', providerTemplates)).toEqual({ match: null, ambiguous: false });
    expect(detectProviderByPrefix('   ', providerTemplates)).toEqual({ match: null, ambiguous: false });
  });

  it('treats an identical longest prefix shared by two providers as ambiguous', () => {
    const shared = [{ id: 'a', keyPrefixes: ['key-1234'] }, { id: 'b', keyPrefixes: ['key-1234'] }];
    expect(detectProviderByPrefix('key-1234-x', shared)).toEqual({ match: null, ambiguous: true });
  });
});

describe('provider groups', () => {
  it('marks the intended providers as featured', () => {
    const featured = providerTemplates.filter((provider) => provider.featured).map((provider) => provider.id).sort();
    expect(featured).toEqual(['anthropic', 'deepseek', 'gemini', 'glm', 'kimi', 'minimax', 'openai', 'openrouter', 'qwen', 'volcengine']);
  });

  it('puts featured and already used providers in the common group with custom last', () => {
    const plain = groupProviders(providerTemplates, []);
    expect(plain.common.at(-1)?.id).toBe('custom');
    expect(plain.common.slice(0, -1).every((provider) => provider.featured)).toBe(true);
    expect(plain.more.some((provider) => provider.id === 'groq')).toBe(true);
    const used = groupProviders(providerTemplates, ['groq']);
    expect(used.common.some((provider) => provider.id === 'groq')).toBe(true);
    expect(used.common.at(-1)?.id).toBe('custom');
    expect(used.more.some((provider) => provider.id === 'groq')).toBe(false);
    expect(used.common.length + used.more.length).toBe(providerTemplates.length);
  });

  it('filters both groups by the search term', () => {
    const result = groupProviders(providerTemplates, [], 'GROQ');
    expect(result.searching).toBe(true);
    expect(result.more.map((provider) => provider.id)).toEqual(['groq']);
    expect(result.common).toEqual([]);
    expect(groupProviders(providerTemplates, [], 'deepseek').common.map((provider) => provider.id)).toEqual(['deepseek']);
  });

  it('suggests unique names with a numeric suffix', () => {
    expect(uniqueCredentialName('OpenAI', [])).toBe('OpenAI');
    expect(uniqueCredentialName('OpenAI', ['openai'])).toBe('OpenAI 2');
    expect(uniqueCredentialName('OpenAI', ['OpenAI', 'OpenAI 2'])).toBe('OpenAI 3');
  });
});
