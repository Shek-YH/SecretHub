// npm run catalog:bake -- merges model-catalog.json into providers.json and updates catalog-serial.json.
import { fail } from './lib/args.mjs';
import { bake } from './lib/commands.mjs';

try {
  const result = bake();
  console.log(`baked serial ${result.serial} into providers.json (${result.providers} providers); review the diff before releasing`);
} catch (error) { fail(error); }
