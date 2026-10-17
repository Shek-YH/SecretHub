// npm run catalog:check [path-to-model-catalog.json]
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { bundledProviders, bundledSerial, definitionsDir, validateCatalog } from './lib/catalog.mjs';

const file = process.argv[2] ?? join(definitionsDir, 'model-catalog.json');
const text = readFileSync(file, 'utf8');
const errors = validateCatalog(text, bundledProviders().map((provider) => provider.id));
const serial = JSON.parse(text)?.serial;
if (errors.length === 0 && Number.isSafeInteger(serial) && serial < bundledSerial().serial) {
  errors.push(`serial ${serial} is lower than the bundled serial ${bundledSerial().serial}`);
}
if (errors.length > 0) {
  console.error(`${file}: ${errors.length} problem(s)`);
  for (const error of errors) console.error(`  - ${error}`);
  process.exit(1);
}
console.log(`${file}: OK (serial ${serial})`);
