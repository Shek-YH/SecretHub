import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';

const directory = mkdtempSync(join(tmpdir(), 'secrethub-mcp-'));
const databasePath = join(directory, 'vault.sqlite');
const planDirectory = join(directory, 'plans');
const resultDirectory = join(directory, 'results');
const database = new DatabaseSync(databasePath);
database.exec(`CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT, provider_id TEXT, env_key TEXT, status TEXT, tags_json TEXT, value_type TEXT, category TEXT, scope TEXT, favorite INTEGER, archived INTEGER, model_id TEXT, model_env_key TEXT, endpoint_url TEXT, endpoint_env_key TEXT, updated_at INTEGER); CREATE TABLE secret_payloads (secret_id TEXT PRIMARY KEY, ciphertext BLOB); CREATE TABLE profiles (id TEXT PRIMARY KEY, name TEXT, description TEXT, updated_at INTEGER); CREATE TABLE profile_secrets (profile_id TEXT, secret_id TEXT, position INTEGER); CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT, display_name TEXT, last_used_at INTEGER); INSERT INTO secrets VALUES ('secret-fixture', 'Fixture Metadata', 'generic', 'FIXTURE_KEY', 'unknown', '["test"]', 'token', 'other', 'global', 0, 0, '', '', 'https://example.invalid', 'GENERIC_BASE_URL', 1); INSERT INTO secret_payloads VALUES ('secret-fixture', X'010203'); CREATE TABLE media_assets (id TEXT PRIMARY KEY, kind TEXT, filename TEXT, stored_path TEXT, sha256 TEXT, size_bytes INTEGER, mime TEXT, width INTEGER, height INTEGER, duration_ms INTEGER, description TEXT, tags_json TEXT, source TEXT, created_at INTEGER); INSERT INTO media_assets VALUES ('asset-1', 'image', 'cat.png', 'C:\\media\\asset-1.png', 'aa', 10, 'image/png', 1920, 1080, NULL, 'a cat', '["pets"]', 'user', 2), ('asset-2', 'video', 'clip.mp4', 'C:\\media\\asset-2.mp4', 'bb', 20, 'video/mp4', 1080, 1920, 5000, 'portrait clip', '[]', 'ai', 1); CREATE TABLE provider_models (secret_id TEXT, model_id TEXT, display_name TEXT, input_json TEXT, output_json TEXT, capabilities_json TEXT, capability_source TEXT, synced_at INTEGER); INSERT INTO secrets VALUES ('secret-model', 'Model Key', 'deepseek', 'DEEPSEEK_API_KEY', 'valid', '[]', 'api_key', 'ai', 'global', 0, 0, '', '', '', '', 0); INSERT INTO secret_payloads VALUES ('secret-model', X'0A0B0C'); INSERT INTO provider_models VALUES ('secret-model', 'plain-model', '', '[]', '[]', '[]', 'unknown', 5), ('secret-model', 'vision-model', '', '[]', '[]', '["chat","vision"]', 'provider', 5);`);
// Personal items (typed-items design section 6): no tool may reveal them, not even by name.
database.exec(`INSERT INTO secrets VALUES ('secret-card', 'Hidden Salary Card', 'personal', '', 'unknown', '[]', 'bank_card', 'personal', 'global', 0, 0, '', '', '', '', 0), ('secret-note', 'Hidden Private Note', 'personal', '', 'unknown', '[]', 'note', 'personal', 'global', 0, 0, '', '', '', '', 0), ('secret-login', 'Hidden Personal Login', 'personal', 'HIDDEN_LOGIN_KEY', 'unknown', '[]', 'login', 'personal', 'global', 0, 0, '', '', '', '', 0), ('secret-legacy-url', 'Hidden Legacy Url', 'generic', '', 'unknown', '[]', 'url', 'other', 'global', 0, 0, '', '', '', '', 0);
  INSERT INTO secret_payloads VALUES ('secret-card', X'0D0E0F'), ('secret-note', X'0D0E0F'), ('secret-login', X'0D0E0F'), ('secret-legacy-url', X'0D0E0F');
  INSERT INTO provider_models VALUES ('secret-card', 'hidden-vision-model', '', '[]', '[]', '["chat","vision","video_gen"]', 'provider', 5);
  INSERT INTO profiles VALUES ('profile-mixed', 'Mixed Profile', '', 1); INSERT INTO profile_secrets VALUES ('profile-mixed', 'secret-model', 0), ('profile-mixed', 'secret-card', 1);`);
database.close();
const messages = [
  { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke', version: '1' } } },
  { jsonrpc: '2.0', method: 'notifications/initialized', params: {} },
  { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} },
  { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'secrethub_list_secret_catalog', arguments: { response_format: 'json', limit: 20, offset: 0 } } },
  { jsonrpc: '2.0', id: 5, method: 'tools/call', params: { name: 'secrethub_find_models', arguments: { capabilities: ['chat', 'vision', 'video_gen'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 6, method: 'tools/call', params: { name: 'secrethub_prepare_project_env', arguments: { project_path: 'C:\Projects\fixture', bindings: [{ secret_id: 'secret-model', capability: 'chat', model_id: 'vision-model', env: { api_key: 'DEEPSEEK_API_KEY', base_url: 'DEEPSEEK_BASE_URL', model: 'LLM_MODEL' } }, { secret_id: 'secret-model', capability: 'vision', model_id: 'vision-model', env: { api_key: 'DEEPSEEK_API_KEY', model: 'VISION_MODEL' } }], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 7, method: 'tools/call', params: { name: 'secrethub_prepare_project_env', arguments: { project_path: 'C:\Projects\fixture', bindings: [{ secret_id: 'missing', capability: 'chat', env: { api_key: 'K' } }], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'secrethub_prepare_project_env', arguments: { project_path: 'C:\\Projects\\fixture', secret_ids: ['secret-fixture'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 8, method: 'tools/call', params: { name: 'secrethub_suggest_model_capabilities', arguments: { provider_id: 'deepseek', suggestions: [{ model_id: 'plain-model', capabilities: ['chat', 'code'], evidence_url: 'https://docs.example/plain-model' }], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 9, method: 'tools/call', params: { name: 'secrethub_suggest_model_capabilities', arguments: { provider_id: 'no-such-provider', suggestions: [{ model_id: 'plain-model', capabilities: ['chat'] }], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 10, method: 'tools/call', params: { name: 'secrethub_suggest_model_capabilities', arguments: { provider_id: 'deepseek', suggestions: [{ model_id: 'a', capabilities: ['chat'] }, { model_id: 'a', capabilities: ['code'] }], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 11, method: 'tools/call', params: { name: 'secrethub_request_model_sync', arguments: { secret_ids: ['secret-model'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 12, method: 'tools/call', params: { name: 'secrethub_request_model_sync', arguments: { secret_ids: ['secret-fixture'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 13, method: 'tools/call', params: { name: 'secrethub_get_sync_status', arguments: { request_id: 'sync-../x', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 14, method: 'tools/call', params: { name: 'secrethub_start_key_capture', arguments: { provider_id: 'deepseek', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 15, method: 'tools/call', params: { name: 'secrethub_start_key_capture', arguments: { provider_id: 'custom', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 16, method: 'tools/call', params: { name: 'secrethub_get_capture_status', arguments: { request_id: 'capture-../x', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 17, method: 'tools/call', params: { name: 'secrethub_media_search', arguments: { orientation: 'portrait', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 18, method: 'tools/call', params: { name: 'secrethub_media_get', arguments: { id: 'asset-1', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 19, method: 'tools/call', params: { name: 'secrethub_media_add', arguments: { path: databasePath, description: 'fixture file', tags: ['t1'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 20, method: 'tools/call', params: { name: 'secrethub_media_add', arguments: { path: join(directory, 'does-not-exist.png'), response_format: 'json' } } },
  { jsonrpc: '2.0', id: 21, method: 'tools/call', params: { name: 'secrethub_media_add', arguments: { path: directory, response_format: 'json' } } },
  { jsonrpc: '2.0', id: 22, method: 'tools/call', params: { name: 'secrethub_get_media_add_status', arguments: { request_id: 'media-../x', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 23, method: 'tools/call', params: { name: 'secrethub_get_invocation', arguments: { secret_id: 'secret-model', capability: 'chat', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 24, method: 'tools/call', params: { name: 'secrethub_get_invocation', arguments: { secret_id: 'secret-model', capability: 'video_gen', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 25, method: 'tools/call', params: { name: 'secrethub_get_invocation', arguments: { secret_id: 'missing', capability: 'chat', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 26, method: 'tools/call', params: { name: 'secrethub_search_secrets', arguments: { query: 'Hidden', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 27, method: 'tools/call', params: { name: 'secrethub_get_secret_metadata', arguments: { id: 'secret-card', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 28, method: 'tools/call', params: { name: 'secrethub_list_profiles', arguments: { response_format: 'json' } } },
  { jsonrpc: '2.0', id: 29, method: 'tools/call', params: { name: 'secrethub_prepare_project_env', arguments: { project_path: 'C:\\Projects\\fixture', secret_ids: ['secret-card'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 30, method: 'tools/call', params: { name: 'secrethub_get_invocation', arguments: { secret_id: 'secret-card', capability: 'chat', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 31, method: 'tools/call', params: { name: 'secrethub_validate_secret', arguments: { id: 'secret-note', response_format: 'json' } } },
  { jsonrpc: '2.0', id: 32, method: 'tools/call', params: { name: 'secrethub_recommend_secrets_for_project', arguments: { stack: ['hidden-login'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 33, method: 'tools/call', params: { name: 'secrethub_request_model_sync', arguments: { secret_ids: ['secret-login'], response_format: 'json' } } },
];
const result = spawnSync(process.execPath, ['dist/index.js'], { cwd: new URL('..', import.meta.url), input: `${messages.map((message) => JSON.stringify(message)).join('\n')}\n`, encoding: 'utf8', env: { ...process.env, SECRETHUB_DB_PATH: databasePath, SECRETHUB_PLAN_DIR: planDirectory, SECRETHUB_RESULT_DIR: resultDirectory } });
try {
  assert.equal(result.status, 0, result.stderr);
  const responses = result.stdout.trim().split(/\r?\n/).map((line) => JSON.parse(line));
  const tools = responses.find((response) => response.id === 2).result.tools;
  assert.ok(tools.some((tool) => tool.name === 'secrethub_list_secret_catalog'));
  assert.ok(tools.some((tool) => tool.name === 'secrethub_prepare_project_env'));
  const catalogResponse = responses.find((response) => response.id === 3);
  const item = catalogResponse.result.structuredContent.items[0];
  assert.deepEqual(item, { id: 'secret-fixture', name: 'Fixture Metadata', provider: 'generic', envKey: 'FIXTURE_KEY', status: 'unknown', tags: ['test'], hasValue: true, valueType: 'token', category: 'other', scope: 'global', favorite: false, archived: false, modelId: '', modelEnvKey: '', endpointUrl: 'https://example.invalid', endpointEnvKey: 'GENERIC_BASE_URL' });
  assert.equal(Object.hasOwn(item, 'value'), false);
  assert.equal(JSON.stringify(catalogResponse).includes('010203'), false);
  assert.ok(tools.some((tool) => tool.name === 'secrethub_find_models'));
  const found = responses.find((response) => response.id === 5).result.structuredContent;
  assert.deepEqual(found.results[0].candidates.map((c) => [c.modelId, c.assumed]), [['vision-model', false], ['plain-model', true]]);
  assert.equal(found.results[0].candidates[0].apiKeyEnvKey, 'DEEPSEEK_API_KEY');
  assert.deepEqual(found.results[1].candidates.map((c) => c.modelId), ['vision-model']);
  assert.deepEqual(found.uncovered, ['video_gen']);
  assert.equal(JSON.stringify(found).includes('0A0B0C'), false);
  const planResponse = responses.find((response) => response.id === 4);
  const requestId = planResponse.result.structuredContent.requestId;
  assert.equal(planResponse.result.structuredContent.requiresUserConfirmation, true);
  const plan = JSON.parse(readFileSync(`${planDirectory}/${requestId}.json`, 'utf8'));
  assert.deepEqual(plan.secretIds, ['secret-fixture']);
  assert.equal(JSON.stringify(plan).includes('010203'), false);
  assert.ok(tools.some((tool) => tool.name === 'secrethub_get_plan_status'));
  const v2Response = responses.find((response) => response.id === 6).result.structuredContent;
  const v2 = JSON.parse(readFileSync(`${planDirectory}/${v2Response.requestId}.json`, 'utf8'));
  assert.equal(v2.version, 2);
  assert.equal(v2.bindings.length, 2);
  assert.equal(JSON.stringify(v2).includes('0A0B0C'), false);
  assert.equal(Object.hasOwn(v2.bindings[0], 'value'), false);
  const rejected = responses.find((response) => response.id === 7).result;
  assert.equal(rejected.isError, true);
  assert.match(rejected.structuredContent.error, /Unknown secret id/);
  assert.ok(tools.some((tool) => tool.name === 'secrethub_suggest_model_capabilities'));
  const suggested = responses.find((response) => response.id === 8).result.structuredContent;
  assert.match(suggested.requestId, /^caps-/);
  assert.equal(suggested.requiresUserConfirmation, true);
  const caps = JSON.parse(readFileSync(`${planDirectory}/${suggested.requestId}.json`, 'utf8'));
  assert.equal(caps.kind, 'suggest_model_capabilities');
  assert.deepEqual(caps.suggestions, [{ modelId: 'plain-model', capabilities: ['chat', 'code'], evidenceUrl: 'https://docs.example/plain-model' }]);
  assert.equal(responses.find((response) => response.id === 9).result.isError, true);
  assert.equal(responses.find((response) => response.id === 10).result.isError, true);
  assert.ok(tools.some((tool) => tool.name === 'secrethub_request_model_sync'));
  assert.ok(tools.some((tool) => tool.name === 'secrethub_get_sync_status'));
  const syncQueued = responses.find((response) => response.id === 11).result.structuredContent;
  assert.match(syncQueued.requestId, /^sync-/);
  const syncFile = JSON.parse(readFileSync(`${planDirectory}/${syncQueued.requestId}.json`, 'utf8'));
  assert.deepEqual(syncFile.secretIds, ['secret-model']);
  assert.equal(JSON.stringify(syncFile).includes('0A0B0C'), false);
  assert.equal(responses.find((response) => response.id === 12).result.isError, true);
  assert.equal(responses.find((response) => response.id === 13).result.isError, true);
  assert.ok(tools.some((tool) => tool.name === 'secrethub_start_key_capture'));
  assert.ok(tools.some((tool) => tool.name === 'secrethub_get_capture_status'));
  const captureQueued = responses.find((response) => response.id === 14).result.structuredContent;
  assert.match(captureQueued.requestId, /^capture-/);
  const captureFile = JSON.parse(readFileSync(`${planDirectory}/${captureQueued.requestId}.json`, 'utf8'));
  assert.deepEqual([captureFile.kind, captureFile.providerId], ['start_key_capture', 'deepseek']);
  assert.equal(responses.find((response) => response.id === 15).result.isError, true);
  assert.equal(responses.find((response) => response.id === 16).result.isError, true);
  for (const name of ['secrethub_media_search', 'secrethub_media_get', 'secrethub_media_add', 'secrethub_get_media_add_status']) assert.ok(tools.some((tool) => tool.name === name), name);
  const mediaFound = responses.find((response) => response.id === 17).result.structuredContent;
  assert.deepEqual(mediaFound.items.map((item) => [item.id, item.orientation, item.durationMs]), [['asset-2', 'portrait', 5000]]);
  assert.equal(mediaFound.items[0].path, 'C:\\media\\asset-2.mp4');
  const mediaGot = responses.find((response) => response.id === 18).result.structuredContent;
  assert.deepEqual([mediaGot.found, mediaGot.item.orientation, mediaGot.item.tags], [true, 'landscape', ['pets']]);
  const mediaQueued = responses.find((response) => response.id === 19).result.structuredContent;
  assert.match(mediaQueued.requestId, /^media-/);
  const mediaFile = JSON.parse(readFileSync(`${planDirectory}/${mediaQueued.requestId}.json`, 'utf8'));
  assert.deepEqual([mediaFile.kind, mediaFile.path, mediaFile.tags], ['add_media', databasePath, ['t1']]);
  assert.equal(responses.find((response) => response.id === 20).result.isError, true);
  assert.equal(responses.find((response) => response.id === 21).result.isError, true);
  assert.equal(responses.find((response) => response.id === 22).result.isError, true);
  assert.ok(tools.some((tool) => tool.name === 'secrethub_get_invocation'));
  const invocation = responses.find((response) => response.id === 23).result.structuredContent;
  assert.deepEqual([invocation.kind, invocation.url, invocation.authHeader], ['openai-chat', 'https://api.deepseek.com/chat/completions', 'Authorization: Bearer ${DEEPSEEK_API_KEY}']);
  assert.equal(invocation.envVars.apiKey, 'DEEPSEEK_API_KEY');
  assert.match(invocation.docsUrl, /^https:/);
  assert.equal(JSON.stringify(invocation).includes('0A0B0C'), false);
  assert.equal(responses.find((response) => response.id === 24).result.structuredContent.kind, 'native');
  assert.equal(responses.find((response) => response.id === 25).result.isError, true);
  // Personal rows are invisible to every tool, including by name and through profiles.
  assert.equal(responses.find((response) => response.id === 26).result.structuredContent.total, 0);
  const personalMetadata = responses.find((response) => response.id === 27).result;
  assert.equal(personalMetadata.isError === true || personalMetadata.structuredContent?.found === false, true);
  assert.deepEqual(responses.find((response) => response.id === 28).result.structuredContent.profiles.find((profile) => profile.id === 'profile-mixed').secretIds, ['secret-model']);
  for (const id of [29, 30, 33]) assert.equal(responses.find((response) => response.id === id).result.isError, true, `call ${id}`);
  // Names, payload bytes and env names never appear anywhere; ids may only be echoed back for calls that named them.
  assert.equal(JSON.stringify(responses).match(/Hidden|0D0E0F|hidden-vision-model|HIDDEN_LOGIN_KEY/g)?.length ?? 0, 0);
  for (const id of [3, 5, 26, 28, 32]) assert.equal(/secret-(card|note|login|legacy-url)/.test(JSON.stringify(responses.find((response) => response.id === id))), false, `call ${id}`);
  mkdirSync(resultDirectory, { recursive: true });
  writeFileSync(join(resultDirectory, `${v2Response.requestId}.json`), JSON.stringify({ requestId: v2Response.requestId, status: 'written', writtenKeys: ['DEEPSEEK_API_KEY'], skippedKeys: [], warnings: [], finishedAt: 'now' }));
  const statusCalls = [
    { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke', version: '1' } } },
    { jsonrpc: '2.0', method: 'notifications/initialized', params: {} },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'secrethub_get_plan_status', arguments: { request_id: v2Response.requestId, response_format: 'json' } } },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'secrethub_get_plan_status', arguments: { request_id: requestId, response_format: 'json' } } },
    { jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'secrethub_get_plan_status', arguments: { request_id: 'plan-../x', response_format: 'json' } } },
  ];
  const statusRun = spawnSync(process.execPath, ['dist/index.js'], { cwd: new URL('..', import.meta.url), input: `${statusCalls.map((message) => JSON.stringify(message)).join('\n')}\n`, encoding: 'utf8', env: { ...process.env, SECRETHUB_DB_PATH: databasePath, SECRETHUB_PLAN_DIR: planDirectory, SECRETHUB_RESULT_DIR: resultDirectory } });
  assert.equal(statusRun.status, 0, statusRun.stderr);
  const statuses = statusRun.stdout.trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.deepEqual(statuses.find((r) => r.id === 2).result.structuredContent, { status: 'written', requestId: v2Response.requestId, writtenKeys: ['DEEPSEEK_API_KEY'], skippedKeys: [], warnings: [] });
  assert.equal(statuses.find((r) => r.id === 3).result.structuredContent.status, 'pending');
  assert.equal(statuses.find((r) => r.id === 4).result.isError, true);
} finally {
  rmSync(directory, { recursive: true, force: true });
}

console.log('SecretHub MCP protocol smoke: clean');
