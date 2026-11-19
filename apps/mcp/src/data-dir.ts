// Where SecretHub Desktop keeps vault.sqlite: Tauri's app_data_dir() for identifier com.secrethub.desktop.
// Must match the desktop app on every platform, otherwise the MCP server opens an empty catalog.
import { homedir } from 'node:os';
import { join } from 'node:path';

export const APP_IDENTIFIER = 'com.secrethub.desktop';

export function defaultDataDir(platform: NodeJS.Platform = process.platform, env: NodeJS.ProcessEnv = process.env, home: string = homedir()): string {
  switch (platform) {
    case 'win32':
      return join(env.APPDATA ?? join(home, 'AppData', 'Roaming'), APP_IDENTIFIER);
    case 'darwin':
      return join(home, 'Library', 'Application Support', APP_IDENTIFIER);
    default:
      return join(env.XDG_DATA_HOME || join(home, '.local', 'share'), APP_IDENTIFIER);
  }
}

export function defaultDbPath(platform?: NodeJS.Platform, env?: NodeJS.ProcessEnv, home?: string): string {
  return join(defaultDataDir(platform, env, home), 'vault.sqlite');
}
