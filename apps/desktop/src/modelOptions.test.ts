import { describe, expect, it } from 'vitest';
import { buildModelOptions } from './modelOptions';

const catalog = [
  { id: 'a-flash', label: 'A Flash', envKey: 'A_MODEL' },
  { id: 'a-pro', label: 'A Pro', envKey: 'A_MODEL' },
];
const synced = (id: string, capabilities: string[] = ['chat']) => ({ model_id: id, display_name: '', input: [], output: [], capabilities, capability_source: 'provider', synced_at: 1 });

describe('buildModelOptions', () => {
  it('keeps catalog models only when nothing is synced', () => {
    const groups = buildModelOptions(catalog, []);
    expect(groups.synced).toEqual([]);
    expect(groups.recommended.map((item) => item.id)).toEqual(['a-flash', 'a-pro']);
  });

  it('puts the catalog default first, sorts the rest by id and moves leftover catalog models to recommended', () => {
    const groups = buildModelOptions(catalog, [synced('zeta'), synced('a-flash'), synced('beta', ['chat', 'vision'])]);
    expect(groups.synced.map((item) => item.id)).toEqual(['a-flash', 'beta', 'zeta']);
    expect(groups.synced[1].capabilities).toEqual(['chat', 'vision']);
    expect(groups.synced[0].envKey).toBe('A_MODEL');
    expect(groups.recommended.map((item) => item.id)).toEqual(['a-pro']);
  });

  it('honours an explicit default model id', () => {
    const groups = buildModelOptions(catalog, [synced('a-flash'), synced('a-pro')], 'a-pro');
    expect(groups.synced.map((item) => item.id)).toEqual(['a-pro', 'a-flash']);
  });
});
