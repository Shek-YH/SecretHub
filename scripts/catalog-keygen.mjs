// npm run catalog:keygen -- --private-out <path outside the repo> [--force]
// The private path may also come from SECRETHUB_CATALOG_PRIVATE_KEY.
import { fail, parseArgs } from './lib/args.mjs';
import { keygen } from './lib/commands.mjs';

try {
  const args = parseArgs(process.argv.slice(2));
  const privateOut = typeof args.privateOut === 'string' ? args.privateOut : process.env.SECRETHUB_CATALOG_PRIVATE_KEY;
  const result = keygen({ privateOut, ...(typeof args.publicOut === 'string' ? { publicOut: args.publicOut } : {}), force: args.force === true });
  console.log(`public key written to ${result.publicOut}`);
  console.log(`private key written to ${result.privateOut}`);
  console.log('Keep the private key offline (for example in a password manager) and never commit it.');
  console.log('Commit the new public key and ship a new app version: only apps built with it accept catalogs signed by this key.');
} catch (error) { fail(error); }
