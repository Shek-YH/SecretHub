import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';

const directory = mkdtempSync(join(tmpdir(), 'secrethub-mcp-'));
const databasePath = join(directory, 'vault.sqlite');
const planDirectory = join(directory, 'plans');
const database = new DatabaseSync(databasePath);
database.exec(`CREATE TABLE secrets (id TEXT PRIMARY KEY, name TEXT, provider_id TEXT, env_key TEXT, status TEXT, tags_json TEXT, value_type TEXT, category TEXT, scope TEXT, favorite INTEGER, archived INTEGER, model_id TEXT, model_env_key TEXT, endpoint_url TEXT, endpoint_env_key TEXT, updated_at INTEGER); CREATE TABLE secret_payloads (secret_id TEXT PRIMARY KEY, ciphertext BLOB); CREATE TABLE profiles (id TEXT PRIMARY KEY, name TEXT, description TEXT, updated_at INTEGER); CREATE TABLE profile_secrets (profile_id TEXT, secret_id TEXT, position INTEGER); CREATE TABLE projects (id TEXT PRIMARY KEY, path TEXT, display_name TEXT, last_used_at INTEGER); INSERT INTO secrets VALUES ('secret-fixture', 'Fixture Metadata', 'generic', 'FIXTURE_KEY', 'unknown', '["test"]', 'token', 'other', 'global', 0, 0, '', '', 'https://example.invalid', 'GENERIC_BASE_URL', 1); INSERT INTO secret_payloads VALUES ('secret-fixture', X'010203'); CREATE TABLE provider_models (secret_id TEXT, model_id TEXT, display_name TEXT, input_json TEXT, output_json TEXT, capabilities_json TEXT, capability_source TEXT, synced_at INTEGER); INSERT INTO secrets VALUES ('secret-model', 'Model Key', 'deepseek', 'DEEPSEEK_API_KEY', 'valid', '[]', 'api_key', 'ai', 'global', 0, 0, '', '', '', '', 0); INSERT INTO secret_payloads VALUES ('secret-model', X'0A0B0C'); INSERT INTO provider_models VALUES ('secret-model', 'plain-model', '', '[]', '[]', '[]', 'unknown', 5), ('secret-model', 'vision-model', '', '[]', '[]', '["chat","vision"]', 'provider', 5);`);
database.close();
const messages = [
  { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke', version: '1' } } },
  { jsonrpc: '2.0', method: 'notifications/initialized', params: {} },
  { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} },
  { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'secrethub_list_secret_catalog', arguments: { response_format: 'json', limit: 20, offset: 0 } } },
  { jsonrpc: '2.0', id: 5, method: 'tools/call', params: { name: 'secrethub_find_models', arguments: { capabilities: ['chat', 'vision', 'video_gen'], response_format: 'json' } } },
  { jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'secrethub_prepare_project_env', arguments: { project_path: 'C:\\Projects\\fixture', secret_ids: ['secret-fixture'], response_format: 'json' } } },
];
const result = spawnSync(process.execPath, ['dist/index.js'], { cwd: new URL('..', import.meta.url), input: `${messages.map((message) => JSON.stringify(message)).join('\n')}\n`, encoding: 'utf8', env: { ...process.env, SECRETHUB_DB_PATH: databasePath, SECRETHUB_PLAN_DIR: planDirectory } });
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
} finally {
  rmSync(directory, { recursive: true, force: true });
}

console.log('SecretHub MCP protocol smoke: clean');
