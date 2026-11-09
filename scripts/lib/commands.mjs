// Implementations behind npm run catalog:keygen / catalog:sign / catalog:bake.
// The CLI wrappers in scripts/ only parse arguments; tests call these directly with temp paths.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { bundledProviders, definitionsDir, repoRoot, validateCatalog } from './catalog.mjs';
import { assertOutsideRepo, generateSeed, privateKeyFromSeed, publicB64, readPrivateKeyFile, readPublicKeyFile, signBytes, verifyBytes } from './keys.mjs';

export const defaultCatalogPath = join(definitionsDir, 'model-catalog.json');
export const defaultPublicKeyPath = join(definitionsDir, 'catalog-public-key.txt');
export const defaultOutDir = join(repoRoot, 'dist', 'catalog');

/** New key pair: private seed to a path outside the repo, public key to the repo file. */
export function keygen({ privateOut, publicOut = defaultPublicKeyPath, force = false }) {
  assertOutsideRepo(privateOut, 'private key');
  if (existsSync(privateOut) && !force) throw new Error(`${privateOut} already exists; pass --force to replace it (the old key will no longer be recoverable)`);
  const seed = generateSeed();
  const publicKey = publicB64(privateKeyFromSeed(seed));
  mkdirSync(dirname(resolve(privateOut)), { recursive: true });
  writeFileSync(privateOut, `${seed}\n`, { mode: 0o600 });
  writeFileSync(publicOut, `${publicKey}\n`);
  return { publicKey, privateOut, publicOut };
}

/** Validates, signs and writes the catalog plus a detached .sig to outDir. */
export function signCatalog({ privateKeyPath, catalogPath = defaultCatalogPath, outDir = defaultOutDir, publicKeyPath = defaultPublicKeyPath }) {
  const privateKey = readPrivateKeyFile(privateKeyPath);
  const bytes = readFileSync(catalogPath);
  const errors = validateCatalog(bytes.toString('utf8'), bundledProviders().map((provider) => provider.id));
  if (errors.length > 0) throw new Error(`refusing to sign an invalid catalog:\n  - ${errors.join('\n  - ')}`);
  const signature = signBytes(privateKey, bytes);
  const expected = readPublicKeyFile(publicKeyPath);
  if (publicB64(privateKey) !== expected) throw new Error(`this private key does not match ${publicKeyPath}; apps built with that public key would reject the signature`);
  if (!verifyBytes(expected, bytes, signature)) throw new Error('self-check of the new signature failed');
  mkdirSync(outDir, { recursive: true });
  // Publish exactly the bytes that were signed.
  writeFileSync(join(outDir, 'model-catalog.json'), bytes);
  writeFileSync(join(outDir, 'model-catalog.json.sig'), `${signature}\n`);
  return { outDir, serial: JSON.parse(bytes.toString('utf8')).serial };
}

/** Merges the catalog into providers.json and records its serial as the bundled one. */
export function bake({ catalogPath = defaultCatalogPath, providersPath = join(definitionsDir, 'providers.json'), serialPath = join(definitionsDir, 'catalog-serial.json') } = {}) {
  const text = readFileSync(catalogPath, 'utf8');
  const providers = JSON.parse(readFileSync(providersPath, 'utf8'));
  const errors = validateCatalog(text, providers.map((provider) => provider.id));
  if (errors.length > 0) throw new Error(`catalog is invalid:\n  - ${errors.join('\n  - ')}`);
  const catalog = JSON.parse(text);
  const current = JSON.parse(readFileSync(serialPath, 'utf8'));
  if (catalog.serial < current.serial) throw new Error(`catalog serial ${catalog.serial} is older than the bundled serial ${current.serial}`);
  for (const provider of providers) {
    const entry = catalog.providers[provider.id];
    if (!entry) continue;
    provider.models = entry.models.map((model) => ({
      id: model.id,
      ...(model.label ? { label: model.label } : {}),
      ...(model.capabilities?.length ? { capabilities: model.capabilities } : {}),
      ...(model.deprecated ? { deprecated: true } : {}),
    }));
    if (entry.defaultModel) provider.defaultModel = entry.defaultModel;
    else delete provider.defaultModel;
  }
  writeFileSync(providersPath, `${JSON.stringify(providers, null, 2)}\n`);
  writeFileSync(serialPath, `${JSON.stringify({ serial: catalog.serial, publishedAt: catalog.publishedAt }, null, 2)}\n`);
  return { serial: catalog.serial, providers: Object.keys(catalog.providers).length };
}
