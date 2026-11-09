import definitions from '../../../../packages/item-types/item-types.json';
import type { SecretMetadata } from '../secretTypes';

export type FieldKind = 'text' | 'url' | 'email_or_text' | 'secret' | 'multiline' | 'card_number' | 'expiry' | 'digits' | 'totp_secret';
export type FieldDef = { key: string; kind: FieldKind; sensitive: boolean; required: boolean; minLength?: number; maxLength: number; label: Record<string, string> };
export type ItemTypeDef = { key: string; personal: boolean; storage: string; label: Record<string, string>; fields: FieldDef[] };

/** The four kinds of item added from the type picker, plus API keys. */
export type ItemKind = 'api_key' | 'dev_secret' | 'env_group' | 'login' | 'bank_card' | 'note';
export type PersonalKind = 'login' | 'bank_card' | 'note';

export const ITEM_TYPES = (definitions as { types: ItemTypeDef[] }).types;
export const PICKER_ORDER: ItemKind[] = ['api_key', 'dev_secret', 'env_group', 'login', 'bank_card', 'note'];

export function itemType(key: string): ItemTypeDef | undefined { return ITEM_TYPES.find((item) => item.key === key); }
export function fieldDef(type: string, key: string): FieldDef | undefined { return itemType(type)?.fields.find((field) => field.key === key); }

/** Field label in the UI language ('zh-CN' or 'en-US'; the JSON uses 'en'). */
export function fieldLabel(field: FieldDef, locale: string): string { return field.label[locale === 'zh-CN' ? 'zh-CN' : 'en'] ?? field.key; }

/** Payload is a JSON document and the item never leaves the vault through developer features. */
export function isJsonItem(valueType: string | undefined): boolean { return valueType === 'login' || valueType === 'bank_card' || valueType === 'note' || valueType === 'env_group'; }

/** Mirrors `SecretMetadata::ai_visible` in Rust: whether AI tools, exports and profiles may include it. */
export function isAiVisible(secret: Pick<SecretMetadata, 'valueType' | 'envKey' | 'devUse'>): boolean {
  switch (secret.valueType ?? 'api_key') {
    case 'api_key': case 'dev_secret': case 'token': case 'env_group': return true;
    case 'url': case 'password': case 'text': return Boolean(secret.envKey);
    case 'login': return Boolean(secret.devUse);
    default: return false;
  }
}

/** Navigation group of an item; old value types follow the migration table. */
export function itemGroup(secret: Pick<SecretMetadata, 'valueType' | 'envKey'>): ItemKind {
  const type = secret.valueType ?? 'api_key';
  switch (type) {
    case 'api_key': case 'dev_secret': case 'env_group': case 'login': case 'bank_card': case 'note': return type;
    case 'token': return 'dev_secret';
    case 'url': return secret.envKey ? 'dev_secret' : 'login';
    case 'password': return secret.envKey ? 'dev_secret' : 'login';
    case 'text': return secret.envKey ? 'dev_secret' : 'note';
    default: return 'dev_secret';
  }
}

/** Card numbers are typed and shown in groups of four. */
export function groupCardNumber(input: string): string {
  const digits = input.replace(/\D/g, '').slice(0, 19);
  return digits.replace(/(.{4})(?=.)/g, '$1 ');
}

export function luhnValid(digits: string): boolean {
  if (!/^\d+$/.test(digits)) return false;
  let sum = 0;
  for (let index = 0; index < digits.length; index += 1) {
    let digit = Number(digits[digits.length - 1 - index]);
    if (index % 2 === 1) { digit *= 2; if (digit > 9) digit -= 9; }
    sum += digit;
  }
  return sum % 10 === 0;
}

/** 12-19 digits whose checksum fails: a warning only, saving stays possible. */
export function cardChecksumWarning(grouped: string): boolean {
  const digits = grouped.replace(/\s/g, '');
  return digits.length >= 12 && digits.length <= 19 && !luhnValid(digits);
}

/** Types month/year as MM/YY: inserts the slash by itself. */
export function formatExpiry(input: string): string {
  const digits = input.replace(/\D/g, '').slice(0, 4);
  return digits.length > 2 ? `${digits.slice(0, 2)}/${digits.slice(2)}` : digits;
}

export function validExpiry(value: string): boolean { return /^(0[1-9]|1[0-2])\/\d{2}$/.test(value); }
export function validEnvPrefix(value: string): boolean { return /^[A-Z_][A-Z0-9_]*$/.test(value) && value.length <= 100; }

export type PasswordOptions = { length: number; symbols: boolean };
export const PASSWORD_MIN = 12;
export const PASSWORD_MAX = 64;
export const PASSWORD_DEFAULT = 20;
const LETTERS = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ';
const DIGITS = '23456789';
const SYMBOLS = '!@#$%^&*()-_=+[]{};:,.?';

/** Uniform random index below `max` from crypto.getRandomValues (rejection sampling, no modulo bias). */
function randomBelow(max: number): number {
  const limit = Math.floor(0x100000000 / max) * max;
  const buffer = new Uint32Array(1);
  do { crypto.getRandomValues(buffer); } while (buffer[0] >= limit);
  return buffer[0] % max;
}

/** Random password with at least one letter, one digit and (when enabled) one symbol. */
export function generatePassword({ length, symbols }: PasswordOptions): string {
  const size = Math.min(PASSWORD_MAX, Math.max(PASSWORD_MIN, Math.round(length)));
  const pools = [LETTERS, DIGITS, ...(symbols ? [SYMBOLS] : [])];
  const all = pools.join('');
  const chars = pools.map((pool) => pool[randomBelow(pool.length)]);
  while (chars.length < size) chars.push(all[randomBelow(all.length)]);
  for (let index = chars.length - 1; index > 0; index -= 1) {
    const swap = randomBelow(index + 1);
    [chars[index], chars[swap]] = [chars[swap], chars[index]];
  }
  return chars.join('');
}

/** Host name of a stored website value (a bare `example.com` works too); empty when it cannot be parsed. */
export function hostOf(website: string): string {
  const value = website.trim();
  if (!value) return '';
  try { return new URL(value.includes('://') ? value : `https://${value}`).hostname; } catch { return ''; }
}
