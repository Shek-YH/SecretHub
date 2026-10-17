// Ed25519 helpers for the model-catalog signature. Key formats:
//   public key  file: base64 of the raw 32-byte key (lines starting with # are comments)
//   private key file: base64 of the raw 32-byte seed, one line, kept OUTSIDE the repository
import { createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from 'node:crypto';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import { repoRoot } from './catalog.mjs';

const PKCS8_ED25519_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex');
const SPKI_ED25519_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');

function realOrSelf(path) {
  // Resolve symlinks of the deepest existing ancestor so a link into the repo is still caught.
  let current = resolve(path);
  const tail = [];
  while (!existsSync(current)) {
    const parent = dirname(current);
    if (parent === current) break;
    tail.unshift(current.slice(parent.length).replace(/^[\\/]/, ''));
    current = parent;
  }
  return resolve(realpathSync(current), ...tail);
}

const norm = (value) => (process.platform === 'win32' ? value.toLowerCase() : value);

export function isInsideRepo(path) {
  const rel = relative(norm(realOrSelf(repoRoot)), norm(realOrSelf(path)));
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

export function assertOutsideRepo(path, what) {
  if (!path) throw new Error(`${what}: no path given`);
  if (isInsideRepo(path)) throw new Error(`${what} must live outside the repository (got ${path})`);
}

export function generateSeed() {
  const { privateKey } = generateKeyPairSync('ed25519');
  const der = privateKey.export({ format: 'der', type: 'pkcs8' });
  return der.subarray(der.length - 32).toString('base64');
}

export function privateKeyFromSeed(seedB64) {
  const seed = Buffer.from(String(seedB64).trim(), 'base64');
  if (seed.length !== 32) throw new Error('private key file must contain a base64 32-byte seed');
  return createPrivateKey({ key: Buffer.concat([PKCS8_ED25519_PREFIX, seed]), format: 'der', type: 'pkcs8' });
}

export function publicB64(privateKey) {
  const der = createPublicKey(privateKey).export({ format: 'der', type: 'spki' });
  return der.subarray(SPKI_ED25519_PREFIX.length).toString('base64');
}

export function signBytes(privateKey, bytes) {
  return sign(null, bytes, privateKey).toString('base64');
}

export function verifyBytes(publicKeyB64, bytes, signatureB64) {
  const raw = Buffer.from(String(publicKeyB64).trim(), 'base64');
  const signature = Buffer.from(String(signatureB64).trim(), 'base64');
  if (raw.length !== 32 || signature.length !== 64) return false;
  const key = createPublicKey({ key: Buffer.concat([SPKI_ED25519_PREFIX, raw]), format: 'der', type: 'spki' });
  return verify(null, bytes, key, signature);
}

export function readPublicKeyFile(path) {
  const line = readFileSync(path, 'utf8').split(/\r?\n/).map((item) => item.trim()).find((item) => item && !item.startsWith('#'));
  if (!line) throw new Error(`${path}: no public key found`);
  return line;
}

export function readPrivateKeyFile(path) {
  assertOutsideRepo(path, 'private key');
  return privateKeyFromSeed(readFileSync(path, 'utf8'));
}
