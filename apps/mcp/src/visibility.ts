import type { DatabaseSync } from 'node:sqlite';

/**
 * Typed-items design section 6: which `secrets` rows the AI may know about at all. Personal
 * items (bank cards, notes, ordinary logins) are filtered out in SQL by every read of the
 * table, so not even their names reach a tool result.
 *
 * - developer types: `api_key`, `dev_secret`, `token`, `env_group` (config groups)
 * - pre-migration `url` / `password` / `text` rows that already export under an env name
 * - `login` rows explicitly marked `dev_use = 1`
 *
 * The Rust side applies the same rule in `SecretMetadata::ai_visible`.
 */
export function aiVisibleCondition(db: DatabaseSync, alias = 's'): string {
  const column = (name: string) => `${alias}.${name}`;
  const developer = `${column('value_type')} IN ('api_key', 'dev_secret', 'token', 'env_group')`;
  const legacy = `(${column('value_type')} IN ('url', 'password', 'text') AND ${column('env_key')} <> '')`;
  // A vault written by a build that predates `dev_use` has no such column: no login is visible then.
  const login = hasColumn(db, 'secrets', 'dev_use') ? `(${column('value_type')} = 'login' AND ${column('dev_use')} = 1)` : '0';
  return `(${developer} OR ${legacy} OR ${login})`;
}

function hasColumn(db: DatabaseSync, table: string, column: string): boolean {
  try {
    return (db.prepare(`PRAGMA table_info(${table})`).all() as Array<Record<string, unknown>>).some((row) => row.name === column);
  } catch { return false; }
}
