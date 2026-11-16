import { readFileSync } from 'node:fs';

// Injected by esbuild `define` in scripts/build-sea.mjs so the standalone exe needs no repo files.
declare const __PROVIDERS_JSON__: string | undefined;

// Shared with the desktop UI and Rust crates; resolves from both src/ and dist/ when not bundled.
export function bundledProvidersJson(): string {
  if (typeof __PROVIDERS_JSON__ !== 'undefined') return __PROVIDERS_JSON__;
  return readFileSync(new URL('../../../packages/provider-definitions/providers.json', import.meta.url), 'utf8');
}
