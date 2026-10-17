import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { after, test } from 'node:test';
import { bundledSerial, definitionsDir, repoRoot } from './lib/catalog.mjs';
import { bake, keygen, signCatalog } from './lib/commands.mjs';
import { privateKeyFromSeed, publicB64, readPublicKeyFile, verifyBytes } from './lib/keys.mjs';

const workDirs = [];
const temp = () => { const dir = mkdtempSync(join(tmpdir(), 'secrethub-catalog-')); workDirs.push(dir); return dir; };
after(() => { for (const dir of workDirs) rmSync(dir, { recursive: true, force: true }); });

const catalogSource = join(definitionsDir, 'model-catalog.json');
const newSerial = bundledSerial().serial + 98;

function editedCatalog() {
  const catalog = JSON.parse(readFileSync(catalogSource, 'utf8'));
  catalog.serial = newSerial;
  catalog.publishedAt = '2026-10-20T08:00:00Z';
  catalog.providers.deepseek = {
    defaultModel: 'deepseek-v9-flash',
    models: [
      { id: 'deepseek-v9-flash', label: 'DeepSeek V9 Flash', capabilities: ['chat'] },
      { id: 'deepseek-v4-pro', deprecated: true, capabilities: ['chat', 'reasoning'] },
    ],
  };
  return catalog;
}

function setup() {
  const dir = temp();
  const privateKey = join(dir, 'keys', 'catalog.key');
  const publicKey = join(dir, 'catalog-public-key.txt');
  keygen({ privateOut: privateKey, publicOut: publicKey });
  const catalogPath = join(dir, 'model-catalog.json');
  writeFileSync(catalogPath, `${JSON.stringify(editedCatalog(), null, 2)}\n`);
  return { dir, privateKey, publicKey, catalogPath };
}

test('keygen writes a usable key pair and refuses in-repo or existing private key paths', () => {
  const { dir, privateKey, publicKey } = setup();
  assert.equal(readPublicKeyFile(publicKey), publicB64(privateKeyFromSeed(readFileSync(privateKey, 'utf8'))));
  if (process.platform !== 'win32') assert.equal(statSync(privateKey).mode & 0o077, 0);
  const other = join(dir, 'other-public.txt');
  assert.throws(() => keygen({ privateOut: privateKey, publicOut: other }), /already exists/);
  assert.doesNotThrow(() => keygen({ privateOut: privateKey, publicOut: other, force: true }));
  for (const inside of [join(repoRoot, 'catalog.key'), join(repoRoot, 'dist', 'catalog', 'k.key'), join(definitionsDir, 'x.key')]) {
    assert.throws(() => keygen({ privateOut: inside, publicOut: other }), /outside the repository/, inside);
  }
  assert.equal(existsSync(join(repoRoot, 'catalog.key')), false);
});

test('sign refuses in-repo keys, invalid catalogs and mismatched public keys', () => {
  const { dir, privateKey, publicKey, catalogPath } = setup();
  const outDir = join(dir, 'out');
  const inRepoKey = join(repoRoot, 'dist', 'inrepo-test.key');
  mkdirSync(join(repoRoot, 'dist'), { recursive: true });
  writeFileSync(inRepoKey, readFileSync(privateKey));
  try {
    assert.throws(() => signCatalog({ privateKeyPath: inRepoKey, catalogPath, outDir, publicKeyPath: publicKey }), /outside the repository/);
  } finally { rmSync(inRepoKey, { force: true }); }
  assert.throws(() => signCatalog({ privateKeyPath: undefined, catalogPath, outDir, publicKeyPath: publicKey }), /no path/);

  const invalid = join(dir, 'invalid.json');
  writeFileSync(invalid, JSON.stringify({ ...editedCatalog(), baseUrl: 'https://evil.example' }));
  assert.throws(() => signCatalog({ privateKeyPath: privateKey, catalogPath: invalid, outDir, publicKeyPath: publicKey }), /invalid catalog/);

  const otherPublic = join(dir, 'other-public.txt');
  keygen({ privateOut: join(dir, 'keys', 'other.key'), publicOut: otherPublic });
  assert.throws(() => signCatalog({ privateKeyPath: privateKey, catalogPath, outDir, publicKeyPath: otherPublic }), /does not match/);
  assert.equal(existsSync(join(outDir, 'model-catalog.json.sig')), false);
});

test('edited catalog is signed, verifies, and tampering or a different key is detected', () => {
  const { dir, privateKey, publicKey, catalogPath } = setup();
  const outDir = join(dir, 'out');
  const result = signCatalog({ privateKeyPath: privateKey, catalogPath, outDir, publicKeyPath: publicKey });
  assert.equal(result.serial, newSerial);
  const bytes = readFileSync(join(outDir, 'model-catalog.json'));
  assert.deepEqual(bytes, readFileSync(catalogPath));
  const signature = readFileSync(join(outDir, 'model-catalog.json.sig'), 'utf8');
  const pub = readPublicKeyFile(publicKey);
  assert.equal(verifyBytes(pub, bytes, signature), true);
  assert.equal(verifyBytes(pub, Buffer.concat([bytes, Buffer.from(' ')]), signature), false);
  const otherPublic = join(dir, 'other-public.txt');
  keygen({ privateOut: join(dir, 'keys', 'other.key'), publicOut: otherPublic });
  assert.equal(verifyBytes(readPublicKeyFile(otherPublic), bytes, signature), false);
});

test('bake merges the catalog into providers.json and the bundled serial, idempotently', () => {
  const { dir, catalogPath } = setup();
  const providersPath = join(dir, 'providers.json');
  const serialPath = join(dir, 'catalog-serial.json');
  copyFileSync(join(definitionsDir, 'providers.json'), providersPath);
  copyFileSync(join(definitionsDir, 'catalog-serial.json'), serialPath);
  const before = JSON.parse(readFileSync(providersPath, 'utf8'));
  const result = bake({ catalogPath, providersPath, serialPath });
  assert.equal(result.serial, newSerial);
  const after = JSON.parse(readFileSync(providersPath, 'utf8'));
  const deepseek = after.find((provider) => provider.id === 'deepseek');
  assert.equal(deepseek.defaultModel, 'deepseek-v9-flash');
  assert.deepEqual(deepseek.models, [
    { id: 'deepseek-v9-flash', label: 'DeepSeek V9 Flash', capabilities: ['chat'] },
    { id: 'deepseek-v4-pro', capabilities: ['chat', 'reasoning'], deprecated: true },
  ]);
  const beforeDeepseek = before.find((provider) => provider.id === 'deepseek');
  assert.equal(deepseek.baseUrl, beforeDeepseek.baseUrl);
  assert.deepEqual(after.filter((provider) => provider.id !== 'deepseek'), before.filter((provider) => provider.id !== 'deepseek'));
  assert.deepEqual(JSON.parse(readFileSync(serialPath, 'utf8')), { serial: newSerial, publishedAt: '2026-10-20T08:00:00Z' });
  const snapshot = readFileSync(providersPath);
  bake({ catalogPath, providersPath, serialPath });
  assert.deepEqual(readFileSync(providersPath), snapshot);

  const stale = JSON.parse(readFileSync(catalogPath, 'utf8'));
  stale.serial = newSerial - 1;
  writeFileSync(catalogPath, JSON.stringify(stale));
  assert.throws(() => bake({ catalogPath, providersPath, serialPath }), /older than the bundled serial/);
  writeFileSync(catalogPath, JSON.stringify({ ...stale, serial: newSerial, extra: 1 }));
  assert.throws(() => bake({ catalogPath, providersPath, serialPath }), /invalid/);
});

const cargoAvailable = spawnSync('cargo', ['--version'], { encoding: 'utf8' }).status === 0;

test('end to end: keygen, edit, sign, then the Rust verifier accepts it and rejects a tampered copy', { skip: !cargoAvailable && 'cargo not available', timeout: 600_000 }, () => {
  const { dir, privateKey, publicKey, catalogPath } = setup();
  const outDir = join(dir, 'out');
  signCatalog({ privateKeyPath: privateKey, catalogPath, outDir, publicKeyPath: publicKey });
  copyFileSync(publicKey, join(outDir, 'catalog-public-key.txt'));
  const runRust = (directory) => spawnSync('cargo', ['test', '-p', 'secrethub-providers', '--test', 'catalog_verify', 'externally_signed'], { cwd: repoRoot, encoding: 'utf8', env: { ...process.env, SECRETHUB_E2E_DIR: directory } });
  const accepted = runRust(outDir);
  assert.equal(accepted.status, 0, accepted.stdout + accepted.stderr);
  assert.match(accepted.stdout, /1 passed/);

  const tampered = join(dir, 'tampered');
  mkdirSync(tampered);
  for (const file of ['model-catalog.json.sig', 'catalog-public-key.txt']) copyFileSync(join(outDir, file), join(tampered, file));
  const stolen = JSON.parse(readFileSync(catalogPath, 'utf8'));
  stolen.providers.deepseek.models[0].id = 'attacker-model';
  writeFileSync(join(tampered, 'model-catalog.json'), JSON.stringify(stolen, null, 2));
  const rejected = runRust(tampered);
  assert.notEqual(rejected.status, 0, 'a tampered catalog must fail the Rust verifier');
});
