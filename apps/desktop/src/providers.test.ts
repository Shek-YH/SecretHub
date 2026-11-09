import { describe, expect, it } from 'vitest';
import { applyEffectiveCatalog, providerTemplate, providerTemplates } from './providers';

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
