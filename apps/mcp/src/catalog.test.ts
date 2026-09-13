import { createCatalogStore, recommendForStack, type CatalogStore } from './catalog.js';

function store(): CatalogStore {
  return createCatalogStore({
    secrets: [
      { id: 'secret-1', name: 'OpenAI Main', provider: 'OpenAI', envKey: 'OPENAI_API_KEY', status: 'valid', tags: ['ai'], hasValue: true },
      { id: 'secret-2', name: 'Supabase URL', provider: 'Supabase', envKey: 'SUPABASE_URL', status: 'unknown', tags: ['db'], hasValue: true },
    ],
    profiles: [],
    projects: [],
  });
}

describe('AI-safe catalog', () => {
  it('returns paginated metadata and never exposes a value field', () => {
    const result = store().list({ limit: 1, offset: 0 });
    expect(result.total).toBe(2);
    expect(result.items).toHaveLength(1);
    expect(result.items[0]).toEqual(expect.objectContaining({ envKey: 'OPENAI_API_KEY', hasValue: true }));
    expect(result.items[0]).not.toHaveProperty('value');
  });

  it('recommends available and missing environment keys from a project stack', () => {
    const result = recommendForStack(['Next.js', 'Supabase', 'OpenAI'], store());
    expect(result.recommended).toEqual(['OPENAI_API_KEY', 'SUPABASE_URL', 'SUPABASE_ANON_KEY']);
    expect(result.available).toEqual(['OPENAI_API_KEY', 'SUPABASE_URL']);
    expect(result.missing).toEqual(['SUPABASE_ANON_KEY']);
  });
});
