import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { bundledProviders, bundledSerial, definitionsDir, validateCatalog, validateProviderInvocations } from './lib/catalog.mjs';

const ids = bundledProviders().map((provider) => provider.id);
const wrap = (providers, extra = {}) => JSON.stringify({ schema: 1, serial: 2026101699, publishedAt: '2026-10-17T00:00:00Z', providers, ...extra });

test('the shipped catalog is valid and not older than the bundled serial', () => {
  const text = readFileSync(join(definitionsDir, 'model-catalog.json'), 'utf8');
  assert.deepEqual(validateCatalog(text, ids), []);
  assert.ok(JSON.parse(text).serial >= bundledSerial().serial);
});

test('rejects the same invalid inputs as the Rust parser', () => {
  const bad = [
    wrap({ deepseek: { models: [{ id: 'x' }], baseUrl: 'https://evil.example' } }),
    wrap({ deepseek: { models: [{ id: 'x' }], invocations: { chat: { kind: 'openai-chat', path: '/x' } } } }),
    wrap({ deepseek: { models: [{ id: 'x' }], docsUrl: 'https://evil.example' } }),
    wrap({ deepseek: { models: [{ id: 'x', url: 'https://evil.example' }] } }),
    wrap({}, { baseUrl: 'https://evil.example' }),
    wrap({ deepseek: { models: [{ id: '' }] } }),
    wrap({ deepseek: { models: [{ id: 'a\u0007b' }] } }),
    wrap({ deepseek: { models: [{ id: 'a'.repeat(201) }] } }),
    wrap({ deepseek: { models: [{ id: 'x', capabilities: ['telepathy'] }] } }),
    wrap({ deepseek: { models: [{ id: 'x' }, { id: 'x' }] } }),
    wrap({ deepseek: { models: [] } }),
    wrap({ deepseek: { defaultModel: 'missing', models: [{ id: 'x' }] } }),
    wrap({ deepseek: { defaultModel: 'x', models: [{ id: 'x', deprecated: true }] } }),
    wrap({ deepseek: { models: Array.from({ length: 301 }, (_, index) => ({ id: `m${index}` })) } }),
    wrap({ 'no-such-provider': { models: [{ id: 'x' }] } }),
    wrap({}, { schema: 2 }),
    'not json',
    ' '.repeat(1024 * 1024 + 1),
  ];
  for (const text of bad) assert.notDeepEqual(validateCatalog(text, ids), [], text.slice(0, 80));
  const good = wrap({ deepseek: { defaultModel: 'x', models: [{ id: 'x', label: 'X', capabilities: ['chat'] }, { id: 'old', deprecated: true }] } });
  assert.deepEqual(validateCatalog(good, ids), []);
  assert.deepEqual(validateCatalog(wrap({ deepseek: { models: Array.from({ length: 300 }, (_, index) => ({ id: `m${index}` })) } }), ids), []);
});

test('catalog:check exits non-zero for a bad file and zero for the shipped one', () => {
  const script = join(definitionsDir, '..', '..', 'scripts', 'catalog-check.mjs');
  assert.equal(spawnSync(process.execPath, [script], { encoding: 'utf8' }).status, 0);
  const result = spawnSync(process.execPath, [script, join(definitionsDir, 'providers.json')], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
});

test('bundled invocations and docsUrl in providers.json are valid', () => {
  const providers = bundledProviders();
  assert.deepEqual(validateProviderInvocations(providers), []);
  assert.ok(providers.some((provider) => Object.keys(provider.invocations ?? {}).length > 0));
});

test('rejects invalid invocations like the Rust validator', () => {
  const make = (extra) => [{ id: 'p', ...extra }];
  const chat = { kind: 'openai-chat', path: '/chat/completions' };
  assert.deepEqual(validateProviderInvocations(make({ invocations: { chat }, docsUrl: 'https://docs.example.com' })), []);
  assert.deepEqual(validateProviderInvocations(make({ invocations: { video_gen: { kind: 'async-task', path: '/t', pollPath: '/t/{id}' } } })), []);
  const bad = [
    { invocations: { telepathy: chat } },
    { invocations: { chat: { ...chat, kind: 'soap' } } },
    { invocations: { chat: { ...chat, path: 'chat' } } },
    { invocations: { chat: { ...chat, path: '//evil.example/x' } } },
    { invocations: { chat: { ...chat, path: '/a/../b' } } },
    { invocations: { chat: { ...chat, path: '/a:b' } } },
    { invocations: { chat: { ...chat, path: '/a b' } } },
    { invocations: { chat: { ...chat, path: `/${'a'.repeat(200)}` } } },
    { invocations: { chat: { ...chat, pollPath: '/p' } } },
    { invocations: { chat: { ...chat, extra: 1 } } },
    { invocations: { video_gen: { kind: 'async-task', path: '/t', pollPath: '/t/{id}/../x' } } },
    { docsUrl: 'http://docs.example.com' },
    { docsUrl: 'not a url' },
  ];
  for (const extra of bad) assert.notDeepEqual(validateProviderInvocations(make(extra)), [], JSON.stringify(extra));
});
