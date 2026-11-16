import { vi } from 'vitest';
import {
  ITEM_TYPES,
  cardChecksumWarning,
  formatExpiry,
  generatePassword,
  groupCardNumber,
  hostOf,
  isAiVisible,
  isJsonItem,
  itemGroup,
  luhnValid,
  PASSWORD_MAX,
  PASSWORD_MIN,
  validEnvPrefix,
  validExpiry,
} from './itemTypes';

describe('shared field definitions', () => {
  it('lists the four typed kinds with the fields from the design', () => {
    expect(ITEM_TYPES.map((item) => item.key)).toEqual(['login', 'bank_card', 'note', 'dev_secret']);
    const keys = (type: string) => ITEM_TYPES.find((item) => item.key === type)!.fields.map((field) => field.key);
    expect(keys('login')).toEqual(['website', 'username', 'password', 'totp', 'notes']);
    expect(keys('bank_card')).toEqual(['bank', 'card_number', 'holder', 'expiry', 'cvv', 'pin', 'branch', 'notes']);
    expect(keys('note')).toEqual(['body']);
    const secrets = ITEM_TYPES.flatMap((item) => item.fields.filter((field) => field.sensitive).map((field) => `${item.key}.${field.key}`));
    expect(secrets).toEqual(['login.password', 'login.totp', 'bank_card.card_number', 'bank_card.cvv', 'bank_card.pin', 'note.body', 'dev_secret.value']);
  });
});

describe('visibility and grouping', () => {
  it('follows the same AI-visibility rule as the backend', () => {
    expect(isAiVisible({ valueType: 'api_key', envKey: '' })).toBe(true);
    expect(isAiVisible({ valueType: 'dev_secret', envKey: '' })).toBe(true);
    expect(isAiVisible({ valueType: 'token', envKey: '' })).toBe(true);
    expect(isAiVisible({ valueType: 'url', envKey: 'FAKE_URL' })).toBe(true);
    expect(isAiVisible({ valueType: 'password', envKey: '' })).toBe(false);
    expect(isAiVisible({ valueType: 'login', envKey: '', devUse: false })).toBe(false);
    expect(isAiVisible({ valueType: 'login', envKey: 'FAKE_TEST', devUse: true })).toBe(true);
    expect(isAiVisible({ valueType: 'bank_card', envKey: 'FAKE', devUse: true })).toBe(false);
    expect(isAiVisible({ valueType: 'note', envKey: 'FAKE' })).toBe(false);
  });

  it('counts old value types into the groups of the migration table', () => {
    expect(itemGroup({ valueType: 'token', envKey: '' })).toBe('dev_secret');
    expect(itemGroup({ valueType: 'url', envKey: 'FAKE_URL' })).toBe('dev_secret');
    expect(itemGroup({ valueType: 'url', envKey: '' })).toBe('login');
    expect(itemGroup({ valueType: 'password', envKey: '' })).toBe('login');
    expect(itemGroup({ valueType: 'password', envKey: 'FAKE_PW' })).toBe('dev_secret');
    expect(itemGroup({ valueType: 'text', envKey: '' })).toBe('note');
    expect(itemGroup({ valueType: 'text', envKey: 'FAKE_TEXT' })).toBe('dev_secret');
    expect(itemGroup({ valueType: undefined, envKey: '' })).toBe('api_key');
    expect(isJsonItem('login') && isJsonItem('bank_card') && isJsonItem('note')).toBe(true);
    expect(isJsonItem('dev_secret') || isJsonItem('api_key')).toBe(false);
  });
});

describe('card helpers', () => {
  it('groups digits by four and ignores everything else', () => {
    expect(groupCardNumber('4111111111111111')).toBe('4111 1111 1111 1111');
    expect(groupCardNumber('4111-1111 11a')).toBe('4111 1111 11');
    expect(groupCardNumber('41111')).toBe('4111 1');
    expect(groupCardNumber('4111 1111 1111 1111 1234 5678')).toBe('4111 1111 1111 1111 123');
  });

  it('checks the Luhn sum and only warns for complete-looking numbers', () => {
    expect(luhnValid('4111111111111111')).toBe(true);
    expect(luhnValid('4111111111111112')).toBe(false);
    expect(luhnValid('')).toBe(false);
    expect(cardChecksumWarning('4111 1111 1111 1112')).toBe(true);
    expect(cardChecksumWarning('4111 1111 1111 1111')).toBe(false);
    expect(cardChecksumWarning('4111 1111')).toBe(false);
  });

  it('formats the expiry as MM/YY', () => {
    expect(formatExpiry('0930')).toBe('09/30');
    expect(formatExpiry('09')).toBe('09');
    expect(formatExpiry('9/3')).toBe('93');
    expect(validExpiry('09/30') && validExpiry('12/99')).toBe(true);
    expect(validExpiry('13/30') || validExpiry('9/30') || validExpiry('09-30')).toBe(false);
  });
});

describe('small validators', () => {
  it('reads host names and env prefixes', () => {
    expect(hostOf('https://accounts.example.com/login')).toBe('accounts.example.com');
    expect(hostOf('example.com')).toBe('example.com');
    expect(hostOf('')).toBe('');
    expect(validEnvPrefix('DOUYIN_TEST')).toBe(true);
    expect(validEnvPrefix('douyin')).toBe(false);
    expect(validEnvPrefix('1ABC')).toBe(false);
  });
});

describe('password generator', () => {
  it('uses crypto.getRandomValues', () => {
    const spy = vi.spyOn(crypto, 'getRandomValues');
    generatePassword({ length: 20, symbols: true });
    expect(spy).toHaveBeenCalled();
    spy.mockRestore();
  });

  it('honours the length range and the symbol switch', () => {
    expect(generatePassword({ length: 20, symbols: true })).toHaveLength(20);
    expect(generatePassword({ length: 3, symbols: true })).toHaveLength(PASSWORD_MIN);
    expect(generatePassword({ length: 500, symbols: true })).toHaveLength(PASSWORD_MAX);
    for (let round = 0; round < 40; round += 1) {
      const plain = generatePassword({ length: 24, symbols: false });
      expect(plain).toMatch(/^[A-Za-z0-9]+$/);
      expect(plain).toMatch(/[a-z]/);
      expect(plain).toMatch(/[A-Z]/);
      expect(plain).toMatch(/[0-9]/);
      const rich = generatePassword({ length: 24, symbols: true });
      expect(rich).toMatch(/[^A-Za-z0-9]/);
    }
  });

  it('does not repeat itself', () => {
    const seen = new Set(Array.from({ length: 50 }, () => generatePassword({ length: 20, symbols: true })));
    expect(seen.size).toBe(50);
  });
});
