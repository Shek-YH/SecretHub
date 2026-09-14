import { describe, expect, it } from 'vitest';
import { providerTemplates } from './providers';

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
});
