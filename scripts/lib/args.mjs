// Minimal `--name value` / `--flag` parser shared by the catalog CLIs.
export function parseArgs(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!arg.startsWith('--')) throw new Error(`unexpected argument ${arg}`);
    const name = arg.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const next = argv[index + 1];
    if (next === undefined || next.startsWith('--')) values[name] = true;
    else { values[name] = next; index += 1; }
  }
  return values;
}

export function fail(error) {
  console.error(`error: ${error.message ?? error}`);
  process.exit(1);
}
