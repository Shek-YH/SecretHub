// Builds the standalone Windows MCP exe: esbuild -> single CJS -> Node SEA blob -> postject into a copy of node.exe.
import { build } from 'esbuild';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const out = join(root, 'build', 'sea');
const target = join(root, '..', 'desktop', 'src-tauri', 'binaries', 'secrethub-mcp-x86_64-pc-windows-msvc.exe');
mkdirSync(out, { recursive: true });
mkdirSync(dirname(target), { recursive: true });

const providers = readFileSync(join(root, '..', '..', 'packages', 'provider-definitions', 'providers.json'), 'utf8');
const bundle = join(out, 'bundle.cjs');
await build({
  entryPoints: [join(root, 'src', 'index.ts')],
  outfile: bundle,
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node22',
  define: { __PROVIDERS_JSON__: JSON.stringify(providers) },
  // node:sqlite prints an ExperimentalWarning to stderr; keep the stdio channel quiet for MCP clients.
  banner: { js: "process.removeAllListeners('warning');" },
  logLevel: 'warning',
  logOverride: { 'empty-import-meta': 'silent' },
});

const blob = join(out, 'sea-prep.blob');
const config = join(out, 'sea-config.json');
writeFileSync(config, JSON.stringify({ main: bundle, output: blob, disableExperimentalSEAWarning: true }));
execFileSync(process.execPath, ['--experimental-sea-config', config], { stdio: 'inherit' });

copyFileSync(process.execPath, target);
const postject = createRequire(import.meta.url).resolve('postject/dist/cli.js');
execFileSync(process.execPath, [postject, target, 'NODE_SEA_BLOB', blob, '--sentinel-fuse', 'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2'], { stdio: 'inherit' });
console.log(`built ${target}`);
