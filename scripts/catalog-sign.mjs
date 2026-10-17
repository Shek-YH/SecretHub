// npm run catalog:sign [-- --catalog <file> --out-dir <dir> --public-key <file>]
// Reads the private key path from SECRETHUB_CATALOG_PRIVATE_KEY (must be outside the repo).
import { fail, parseArgs } from './lib/args.mjs';
import { signCatalog } from './lib/commands.mjs';

try {
  const args = parseArgs(process.argv.slice(2));
  const result = signCatalog({
    privateKeyPath: process.env.SECRETHUB_CATALOG_PRIVATE_KEY,
    ...(typeof args.catalog === 'string' ? { catalogPath: args.catalog } : {}),
    ...(typeof args.outDir === 'string' ? { outDir: args.outDir } : {}),
    ...(typeof args.publicKey === 'string' ? { publicKeyPath: args.publicKey } : {}),
  });
  console.log(`signed serial ${result.serial}; upload model-catalog.json and model-catalog.json.sig from ${result.outDir}`);
} catch (error) { fail(error); }
