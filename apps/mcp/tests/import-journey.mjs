// What an AI client sees after the import/export user journey. The Rust test
// `e2e_journey::user_journey_import_export_rekey` builds a vault from a fake credentials folder and
// copies its database to SECRETHUB_E2E_OUT; this script then talks to the real MCP server over stdio.
// Run through `npm run e2e:import`. All values are fakes.
import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';

const repo = new URL('../../../', import.meta.url);
const mcp = new URL('../', import.meta.url);
const work = mkdtempSync(join(tmpdir(), 'secrethub-import-journey-'));
const run = (command, args, options) => spawnSync(command, args, { encoding: 'utf8', shell: process.platform === 'win32', ...options });

try {
  // 1. The vault: reuse a generated one, or let the Rust journey test produce it.
  let source = process.env.SECRETHUB_E2E_OUT;
  if (!source || !existsSync(source)) {
    source = source || join(work, 'journey.sqlite');
    const cargo = run('cargo', ['test', '-p', 'secrethub-desktop', 'e2e_journey::user_journey', '--', '--nocapture'], { cwd: repo, env: { ...process.env, SECRETHUB_E2E_OUT: source }, stdio: ['ignore', 'inherit', 'inherit'] });
    assert.equal(cargo.status, 0, 'the Rust journey test failed');
  }
  assert.ok(existsSync(source), `no database at ${source}`);
  const build = run('npm', ['run', 'build'], { cwd: mcp });
  assert.equal(build.status, 0, build.stderr || build.stdout);

  // 2. Talk to the real server over stdio, once per database.
  const call = (databasePath, calls) => {
    const messages = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'journey', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized', params: {} },
      ...calls.map(([name, args], index) => ({ jsonrpc: '2.0', id: index + 2, method: 'tools/call', params: { name, arguments: { response_format: 'json', ...args } } })),
    ];
    const result = spawnSync(process.execPath, ['dist/index.js'], {
      cwd: mcp,
      input: `${messages.map((message) => JSON.stringify(message)).join('\n')}\n`,
      encoding: 'utf8',
      env: { ...process.env, SECRETHUB_DB_PATH: databasePath, SECRETHUB_PLAN_DIR: join(work, 'plans'), SECRETHUB_RESULT_DIR: join(work, 'results') },
    });
    assert.equal(result.status, 0, result.stderr);
    const responses = result.stdout.trim().split(/\r?\n/).map((line) => JSON.parse(line));
    return calls.map((_, index) => responses.find((response) => response.id === index + 2));
  };

  // Secret material of the fake folder: none of it may appear in any response.
  const secrets = [
    'fakeval-yc-panel-password', 'fakeval-yc-new-password', 'fakeval-yc-panel-token', 'fakeval-ali-bt-sk', 'fakeval$Pa55', '${HOME}', 'w$$rd',
    'sk-fakeval-dashscope', 'fakeval-runninghub-key', 'FAKEVAL-YC-LINE', 'FAKEVAL-ALI-ENV', 'FAKEVAL-ALI-FILE', 'FAKEVAL-TX-LINE', 'BEGIN OPENSSH PRIVATE KEY',
    // Notes and free text are never part of the catalog.
    'free text line', 'fakeval memo', 'fakeval-city', '腾讯这台机器', '充值记录见微信',
  ];
  const groups = ['野草云', '阿里宝塔', '腾讯宝塔', '充值'];
  const noSecrets = (label, value) => {
    const text = JSON.stringify(value);
    for (const secret of secrets) assert.equal(text.includes(secret), false, `${label} contains ${secret}`);
  };
  const variablesOf = (group) => Object.fromEntries(group.variables.map((variable) => [variable.name, variable]));

  // 3. Catalog, search, metadata with AI values on.
  const [catalog, search, searchKey] = call(source, [
    ['secrethub_list_secret_catalog', { limit: 100, offset: 0 }],
    ['secrethub_search_secrets', { query: '阿里', limit: 20, offset: 0 }],
    ['secrethub_search_secrets', { query: 'DASHSCOPE', limit: 20, offset: 0 }],
  ]);
  const items = catalog.result.structuredContent.items;
  const found = Object.fromEntries(groups.map((name) => [name, items.find((item) => item.valueType === 'env_group' && item.name === name)]));
  for (const name of groups) assert.ok(found[name], `group ${name} is in the catalog`);
  assert.equal(items.filter((item) => item.valueType === 'env_group').length, 4);
  const yc = variablesOf(found['野草云']);
  const ali = variablesOf(found['阿里宝塔']);
  assert.equal(yc.YECAOYUN_HOST.value, 'fakeval-yc-newhost.invalid', 'the merged host is what the AI sees');
  assert.equal(yc.YECAOYUN_HOST.secret, false);
  assert.equal(yc.BT_PANEL_PORT.value, '8888');
  assert.equal(ali.SSH_HOST.value, 'fakeval-ali-host.invalid');
  assert.equal(ali.ALIYUN_OSS_BUCKET.value, 'fakeval-bucket');
  assert.equal(variablesOf(found['腾讯宝塔']).model.value, 'xxx');
  for (const name of ['BT_PANEL_PASSWORD', 'BT_PANEL_API_TOKEN', 'YECAOYUN_SSH_PRIVATE_KEY']) assert.deepEqual([yc[name].secret, 'value' in yc[name]], [true, false], name);
  for (const name of ['SSH_PASSWORD', 'SSH_PRIVATE_KEY', 'SSH_PRIVATE_KEY_2', 'BT_API_SK', 'RUNNINGHUB_API_KEY']) assert.deepEqual([ali[name].secret, 'value' in ali[name]], [true, false], name);
  assert.equal('DASHSCOPE_API_KEY' in ali, false, 'the provider key was split off the group');
  const apiKey = items.find((item) => item.envKey === 'DASHSCOPE_API_KEY');
  assert.ok(apiKey && apiKey.provider === 'qwen' && apiKey.valueType !== 'env_group');
  assert.equal(search.result.structuredContent.items.some((item) => item.name === '阿里宝塔'), true);
  assert.equal(searchKey.result.structuredContent.items.some((item) => item.id === apiKey.id), true);

  const [metadata] = call(source, [['secrethub_get_secret_metadata', { id: found['阿里宝塔'].id }]]);
  assert.equal(metadata.result.structuredContent.found, true);
  assert.equal(variablesOf(metadata.result.structuredContent.item).SSH_HOST.value, 'fakeval-ali-host.invalid');
  noSecrets('catalog', [catalog, search, searchKey, metadata]);

  // 4. Turn AI values off for one group in a copy: its plain values disappear, names stay.
  const copy = join(work, 'ai-off.sqlite');
  copyFileSync(source, copy);
  const edit = new DatabaseSync(copy);
  edit.prepare("UPDATE secrets SET ai_values = 0 WHERE value_type = 'env_group' AND name = ?").run('阿里宝塔');
  edit.close();
  const [offCatalog, offMetadata, offSearch] = call(copy, [
    ['secrethub_list_secret_catalog', { limit: 100, offset: 0 }],
    ['secrethub_get_secret_metadata', { id: found['阿里宝塔'].id }],
    ['secrethub_search_secrets', { query: '阿里', limit: 20, offset: 0 }],
  ]);
  const offItems = offCatalog.result.structuredContent.items;
  const offAli = offItems.find((item) => item.name === '阿里宝塔');
  assert.equal(offAli.aiValues, false);
  assert.ok(offAli.variables.length > 0 && offAli.variables.every((variable) => !('value' in variable)), 'no value of a group with AI values off');
  assert.equal(variablesOf(offAli).SSH_HOST.secret, false);
  assert.equal(JSON.stringify([offCatalog, offMetadata, offSearch]).includes('fakeval-ali-host.invalid'), false);
  assert.equal(JSON.stringify(offMetadata.result.structuredContent.item.variables).includes('"value"'), false);
  // The other groups keep their values.
  assert.equal(variablesOf(offItems.find((item) => item.name === '野草云')).YECAOYUN_HOST.value, 'fakeval-yc-newhost.invalid');
  noSecrets('catalog with AI values off', [offCatalog, offMetadata, offSearch]);
} finally {
  rmSync(work, { recursive: true, force: true });
}

console.log('SecretHub import journey (MCP view): clean');
