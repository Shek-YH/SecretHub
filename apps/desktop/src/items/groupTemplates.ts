import definitions from '../../../../packages/item-types/group-templates.json';

export type GroupTemplate = { key: string; category: string; label: Record<string, string>; variables: Array<{ name: string; secret: boolean; default?: string }> };

type Rules = { plainContains: string[]; plainSuffixes: string[]; secretContains: string[]; secretSuffixes: string[] };
const file = definitions as { secretRules: Rules; categories: string[]; templates: GroupTemplate[] };

export const GROUP_TEMPLATES = file.templates;
export const GROUP_CATEGORIES = file.categories;

export const VARIABLE_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
export const MAX_GROUP_VARIABLES = 200;

export function templateLabel(template: GroupTemplate, locale: string): string { return template.label[locale === 'zh-CN' ? 'zh-CN' : 'en'] ?? template.key; }

/** Default for the secret switch of a variable name (same rules as the Rust side: plain markers win). */
export function isSecretName(name: string): boolean {
  const rules = file.secretRules;
  const upper = name.toUpperCase();
  if (rules.plainContains.some((part) => upper.includes(part)) || rules.plainSuffixes.some((part) => upper.endsWith(part))) return false;
  return rules.secretContains.some((part) => upper.includes(part)) || rules.secretSuffixes.some((part) => upper.endsWith(part));
}

/** `YECAOYUN` + `SSH_HOST` -> `YECAOYUN_SSH_HOST`; an empty prefix leaves the name alone. */
export function prefixedName(prefix: string, base: string): string {
  const cleaned = prefix.trim().replace(/_+$/, '');
  return cleaned ? `${cleaned}_${base}` : base;
}

/** Private keys are multi-line, so they get a text area instead of a one-line input. */
export function isKeyName(name: string): boolean { return name.toUpperCase().includes('PRIVATE_KEY'); }

/** Number of variables from the plaintext list subtitle ("3 个变量"), if it has one. */
export function variableCountOf(subtitle: string | undefined): number | undefined {
  const match = /^(\d+)\b/.exec(subtitle ?? '');
  return match ? Number(match[1]) : undefined;
}
