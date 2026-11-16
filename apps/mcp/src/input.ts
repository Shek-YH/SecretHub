import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';
import type { ModelProvider } from './models.js';

// Secure input forms: the AI queues a form template (field names, labels, how to verify), the user
// fills it in SecretHub Desktop, and Desktop verifies and stores the values. The AI may only prefill
// non-secret fields; no queue file, result file or status here ever contains a secret value.
export function isValidInputRequestId(id: string): boolean { return id.length <= 128 && /^input-[A-Za-z0-9_-]+$/.test(id); }

export const MAX_INPUT_FIELDS = 20;
export const INPUT_ITEM_TYPES = ['login', 'api_key', 'env_group'] as const;
const FIELD_TYPES = ['text', 'password', 'select'] as const;
const FIELD_ROLES = ['api_key', 'base_url', 'model'] as const;
const LOGIN_KEYS = ['website', 'username', 'password', 'totp', 'notes'];
const BODY_NAME = /^[A-Za-z0-9_.-]{1,64}$/;
const VARIABLE_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

// Input schemas (snake_case, as received from the AI).
export const inputFieldSchema = z.object({
  key: z.string().min(1).max(64),
  label: z.string().min(1).max(100),
  type: z.enum(FIELD_TYPES),
  value: z.string().max(2000).optional(),
  options: z.array(z.string().max(200)).max(200).optional(),
  required: z.boolean().optional(),
  role: z.enum(FIELD_ROLES).optional(),
}).strict();

export const inputVerifySchema = z.union([
  z.object({ type: z.literal('none') }).strict(),
  z.object({ type: z.literal('login'), url: z.string().min(1).max(2000), body_user: z.string().optional(), body_pass: z.string().optional(), ok_field: z.string().optional() }).strict(),
  z.object({ type: z.literal('llm'), auth: z.enum(['bearer', 'x-api-key']) }).strict(),
]);

export const inputRequestSchema = {
  title: z.string().trim().min(1).max(100),
  hint: z.string().max(500).optional(),
  item_type: z.enum(INPUT_ITEM_TYPES),
  name: z.string().trim().min(1).max(100),
  provider_id: z.string().trim().min(1).max(100).optional(),
  category: z.string().trim().min(1).max(100).optional(),
  fields: z.array(inputFieldSchema).min(1).max(MAX_INPUT_FIELDS),
  verify: inputVerifySchema.optional(),
};

// Template schema (camelCase, as written to the queue file).
const templateFieldSchema = z.object({
  key: z.string().min(1).max(64),
  label: z.string().min(1).max(100),
  type: z.enum(FIELD_TYPES),
  value: z.string().max(2000).optional(),
  options: z.array(z.string().max(200)).max(200).optional(),
  required: z.boolean().optional(),
  role: z.enum(FIELD_ROLES).optional(),
});
const templateVerifySchema = z.union([
  z.object({ type: z.literal('none') }),
  z.object({ type: z.literal('login'), url: z.string(), bodyUser: z.string().regex(BODY_NAME).optional(), bodyPass: z.string().regex(BODY_NAME).optional(), okField: z.string().regex(BODY_NAME).optional() }),
  z.object({ type: z.literal('llm'), auth: z.enum(['bearer', 'x-api-key']) }),
]);
export const inputTemplateSchema = z.object({
  title: z.string().min(1).max(100),
  hint: z.string().max(500).optional(),
  itemType: z.enum(INPUT_ITEM_TYPES),
  name: z.string().min(1).max(100),
  providerId: z.string().min(1).max(100).optional(),
  category: z.string().max(100).optional(),
  fields: z.array(templateFieldSchema).min(1).max(MAX_INPUT_FIELDS),
  verify: templateVerifySchema.optional(),
});
export type InputTemplate = z.infer<typeof inputTemplateSchema>;
export type InputField = z.infer<typeof templateFieldSchema>;

export type InputRequestArgs = { title: string; hint?: string; item_type: InputTemplate['itemType']; name: string; provider_id?: string; category?: string; fields: z.infer<typeof inputFieldSchema>[]; verify?: z.infer<typeof inputVerifySchema> };

// snake_case tool arguments -> camelCase template.
export function toTemplate(args: InputRequestArgs): InputTemplate {
  const verify = args.verify;
  return {
    title: args.title,
    ...(args.hint !== undefined ? { hint: args.hint } : {}),
    itemType: args.item_type,
    name: args.name,
    ...(args.provider_id ? { providerId: args.provider_id } : {}),
    ...(args.category ? { category: args.category } : {}),
    fields: args.fields,
    ...(verify ? { verify: verify.type === 'login' ? { type: 'login' as const, url: verify.url, ...(verify.body_user !== undefined ? { bodyUser: verify.body_user } : {}), ...(verify.body_pass !== undefined ? { bodyPass: verify.body_pass } : {}), ...(verify.ok_field !== undefined ? { okField: verify.ok_field } : {}) } : verify } : {}),
  };
}

export type InputProblem = { code: string; message: string };
const problem = (code: string, message: string): InputProblem => ({ code, message });

function badUrl(raw: string): boolean {
  let url: URL;
  try { url = new URL(raw); } catch { return true; }
  if (url.username || url.password) return true;
  if (url.protocol === 'https:') return false;
  return !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname));
}

// Returns a reason code and explanation, or undefined when the template can be queued.
export function precheckInput(template: unknown, providers: ModelProvider[]): InputProblem | undefined {
  const parsed = inputTemplateSchema.safeParse(template);
  if (!parsed.success) return problem('bad_shape', `Invalid form template: ${parsed.error.issues.map((issue) => `${issue.path.join('.') || 'template'}: ${issue.message}`).slice(0, 3).join('; ')}`);
  const { itemType, fields, providerId } = parsed.data;
  const verify = parsed.data.verify ?? { type: 'none' as const };

  const seen = new Set<string>();
  for (const field of fields) {
    if (seen.has(field.key)) return problem('duplicate_key', `Field key "${field.key}" appears more than once.`);
    seen.add(field.key);
  }
  for (const field of fields) {
    if (field.type === 'password' && field.value) return problem('prefilled_secret', `Field "${field.key}" is a password field and must not carry a value; the user types it in SecretHub Desktop.`);
    if (field.options && field.type !== 'select') return problem('options_not_select', `Field "${field.key}" has options but is not a select field.`);
    if (itemType === 'login' ? !LOGIN_KEYS.includes(field.key) : !VARIABLE_NAME.test(field.key)) {
      return problem('bad_key', itemType === 'login' ? `Login field key "${field.key}" must be one of ${LOGIN_KEYS.join(', ')}.` : `Field key "${field.key}" must be a valid variable name (letters, digits, underscore; not starting with a digit).`);
    }
  }
  if (itemType === 'login' && !fields.some((field) => field.key === 'username')) return problem('login_needs_username', 'A login form needs a "username" field.');
  const roles = new Set<string>();
  for (const field of fields) {
    if (!field.role) continue;
    if (itemType === 'login') return problem('role_not_allowed', `Field "${field.key}": login fields cannot have a role.`);
    if (roles.has(field.role)) return problem('duplicate_role', `Role "${field.role}" is used by more than one field.`);
    roles.add(field.role);
  }
  if (itemType === 'api_key') {
    const keyFields = fields.filter((field) => field.role === 'api_key');
    if (keyFields.length !== 1 || keyFields[0].type !== 'password') return problem('api_key_fields', 'An api_key form needs exactly one field with role "api_key" and type "password".');
    for (const field of fields) {
      if (!field.role) return problem('api_key_fields', `Field "${field.key}" needs a role (api_key, base_url or model) in an api_key form.`);
      if (field.role === 'base_url' && field.type !== 'text') return problem('api_key_fields', `Field "${field.key}" (base_url) must be a text field.`);
      if (field.role === 'model' && field.type === 'password') return problem('api_key_fields', `Field "${field.key}" (model) must be a text or select field.`);
    }
  }
  if (providerId !== undefined && !providers.some((item) => item.id === providerId)) return problem('unknown_provider', `Unknown provider_id: ${providerId}`);
  if (itemType === 'api_key' && !providerId) return problem('unknown_provider', 'An api_key form requires provider_id.');
  if (verify.type === 'login') {
    const hasPassword = fields.some((field) => field.key === 'password');
    if (itemType !== 'login' || !hasPassword) return problem('verify_mismatch', 'Login verification needs item_type "login" with both username and password fields.');
    if (badUrl(verify.url)) return problem('bad_url', 'verify.url must be https (http is allowed only for localhost, 127.0.0.1 or [::1]) and must not contain a user name or password.');
  } else if (verify.type === 'llm') {
    if (!roles.has('api_key') || !roles.has('base_url')) return problem('verify_mismatch', 'LLM verification needs one field with role "api_key" and one with role "base_url".');
  }
  return undefined;
}

// Fills an empty base_url field from the provider catalog (Desktop repeats this as a fallback).
export function prefillTemplate(template: InputTemplate, providers: ModelProvider[]): InputTemplate {
  const baseUrl = template.providerId ? providers.find((item) => item.id === template.providerId)?.baseUrl : undefined;
  if (!baseUrl) return template;
  return { ...template, fields: template.fields.map((field) => field.role === 'base_url' && !field.value ? { ...field, value: baseUrl } : field) };
}

export function queueInput(directory: string, template: InputTemplate): string {
  const requestId = `input-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = { version: 1, requestId, kind: 'request_input', createdAt: new Date().toISOString(), template };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

export type InputStatus =
  | { status: 'waiting'; requestId: string }
  | { status: 'saved'; requestId: string; itemId: string; itemType?: string }
  | { status: 'cancelled'; requestId: string }
  | { status: 'not_found'; requestId: string }
  | { status: 'failed'; requestId: string; error: { code: string } };

const failed = (requestId: string, code: string): InputStatus => ({ status: 'failed', requestId, error: { code } });

export function getInputStatus(requestId: string, pendingDirectory: string, resultDirectory: string): InputStatus {
  // Desktop writes the result before removing the pending file, so the result wins.
  const resultPath = join(resultDirectory, `${requestId}.json`);
  if (existsSync(resultPath)) {
    let result: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(readFileSync(resultPath, 'utf8'));
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return failed(requestId, 'invalid_result');
      result = parsed as Record<string, unknown>;
    } catch { return failed(requestId, 'unreadable_result'); }
    // Only whitelisted fields are copied out of the file; nothing else leaves it.
    switch (result.status) {
      case 'saved': {
        if (typeof result.itemId !== 'string' || !result.itemId) return failed(requestId, 'invalid_result');
        const itemType = typeof result.itemType === 'string' && /^[a-z_]{1,32}$/.test(result.itemType) ? result.itemType : undefined;
        return { status: 'saved', requestId, itemId: result.itemId.slice(0, 200), ...(itemType ? { itemType } : {}) };
      }
      case 'cancelled': return { status: 'cancelled', requestId };
      case 'failed': {
        const code = (result.error as { code?: unknown } | null | undefined)?.code;
        return failed(requestId, typeof code === 'string' && code.length <= 64 && /^[a-z_]+$/.test(code) ? code : 'invalid_result');
      }
      default: return failed(requestId, 'invalid_result');
    }
  }
  if (existsSync(join(pendingDirectory, `${requestId}.json`))) return { status: 'waiting', requestId };
  return { status: 'not_found', requestId };
}
