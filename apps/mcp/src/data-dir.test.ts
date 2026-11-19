import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { defaultDataDir, defaultDbPath } from './data-dir.js';

describe('defaultDataDir', () => {
  it('uses APPDATA on Windows', () => {
    expect(defaultDataDir('win32', { APPDATA: 'R' }, 'H')).toBe(join('R', 'com.secrethub.desktop'));
    expect(defaultDataDir('win32', {}, 'H')).toBe(join('H', 'AppData', 'Roaming', 'com.secrethub.desktop'));
  });

  it('uses Application Support on macOS and ignores APPDATA', () => {
    expect(defaultDataDir('darwin', { APPDATA: 'R' }, 'H')).toBe(join('H', 'Library', 'Application Support', 'com.secrethub.desktop'));
  });

  it('follows XDG_DATA_HOME on Linux', () => {
    expect(defaultDataDir('linux', { XDG_DATA_HOME: 'X' }, 'H')).toBe(join('X', 'com.secrethub.desktop'));
    expect(defaultDataDir('linux', {}, 'H')).toBe(join('H', '.local', 'share', 'com.secrethub.desktop'));
  });

  it('puts vault.sqlite inside the data directory', () => {
    expect(defaultDbPath('darwin', {}, 'H')).toBe(join('H', 'Library', 'Application Support', 'com.secrethub.desktop', 'vault.sqlite'));
  });
});
