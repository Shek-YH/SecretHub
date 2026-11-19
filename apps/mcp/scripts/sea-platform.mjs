// Platform-specific parts of the Node SEA sidecar build (see build-sea.mjs).
// Windows: inject into a copy of the running node.exe (an official, self-contained build).
// macOS:   Homebrew's node links against Homebrew dylibs and cannot be shipped, so an official
//          nodejs.org build of NODE_VERSION is downloaded (SHA-256 checked) and cached under build/;
//          injection uses the Mach-O segment and the result is ad-hoc signed.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

// Same Node release the Windows sidecar is built with; node:sqlite needs >= 22.5.
export const NODE_VERSION = '22.23.3';

const TRIPLES = {
  'win32-x64': 'x86_64-pc-windows-msvc',
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
};

export function targetTriple(platform = process.platform, arch = process.arch) {
  const triple = TRIPLES[`${platform}-${arch}`];
  if (!triple) throw new Error(`SEA sidecar build is not set up for ${platform}-${arch}`);
  return triple;
}

export function sidecarFileName(triple) {
  return `secrethub-mcp-${triple}${triple.includes('windows') ? '.exe' : ''}`;
}

/** The node binary that is both the blob generator and the binary the blob is injected into. */
export async function baseNode(cacheDir) {
  if (process.env.SECRETHUB_SEA_NODE) return process.env.SECRETHUB_SEA_NODE;
  if (process.platform !== 'darwin') return process.execPath;
  return downloadOfficialNode(cacheDir);
}

async function downloadOfficialNode(cacheDir) {
  const name = `node-v${NODE_VERSION}-darwin-${process.arch}`;
  const binary = join(cacheDir, name, 'bin', 'node');
  if (existsSync(binary)) return binary;
  mkdirSync(cacheDir, { recursive: true });
  const base = `https://nodejs.org/dist/v${NODE_VERSION}`;
  const sums = await (await fetchOk(`${base}/SHASUMS256.txt`)).text();
  const expected = sums.split('\n').map((line) => line.trim().split(/\s+/)).find(([, file]) => file === `${name}.tar.gz`)?.[0];
  if (!expected) throw new Error(`${name}.tar.gz is not listed in SHASUMS256.txt`);
  console.log(`downloading ${base}/${name}.tar.gz`);
  const archive = Buffer.from(await (await fetchOk(`${base}/${name}.tar.gz`)).arrayBuffer());
  const actual = createHash('sha256').update(archive).digest('hex');
  if (actual !== expected) throw new Error(`checksum mismatch for ${name}.tar.gz`);
  const tarball = join(cacheDir, `${name}.tar.gz`);
  writeFileSync(tarball, archive);
  execFileSync('tar', ['-xzf', tarball, '-C', cacheDir, `${name}/bin/node`], { stdio: 'inherit' });
  return binary;
}

async function fetchOk(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`GET ${url} -> ${response.status}`);
  return response;
}

export function beforeInject(target) {
  // The copied official node carries Apple's signature; it must go before the binary is modified.
  if (process.platform === 'darwin') execFileSync('codesign', ['--remove-signature', target], { stdio: 'inherit' });
}

export function postjectArgs() {
  return process.platform === 'darwin' ? ['--macho-segment-name', 'NODE_SEA'] : [];
}

export function afterInject(target) {
  // Apple Silicon refuses to run unsigned code; ad-hoc sign here, `tauri build` re-signs inside the .app.
  if (process.platform === 'darwin') execFileSync('codesign', ['--sign', '-', '--force', target], { stdio: 'inherit' });
}
