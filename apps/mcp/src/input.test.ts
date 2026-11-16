import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { ModelProvider } from './models.js';
import { getInputStatus, isValidInputRequestId, precheckInput, prefillTemplate, queueInput, toTemplate, type InputTemplate } from './input.js';
import { isValidCaptureRequestId } from './capture.js';
import { isValidSyncRequestId } from './sync.js';
import { isValidRequestId } from './plan.js';

const providers: ModelProvider[] = [
  { id: 'deepseek', baseUrl: 'https://api.deepseek.com/v1', envKeys: ['DEEPSEEK_API_KEY'] },
  { id: 'nourl', envKeys: ['NOURL_API_KEY'] },
];

const login: InputTemplate = {
  title: 'Admin account', itemType: 'login', name: 'Admin',
  fields: [{ key: 'username', label: 'User', type: 'text', value: 'admin', required: true }, { key: 'password', label: 'Password', type: 'password', required: true }],
  verify: { type: 'login', url: 'https://example.com/api/login', bodyUser: 'username', bodyPass: 'password', okField: 'ok' },
};
const apiKey: InputTemplate = {
  title: 'DeepSeek key', itemType: 'api_key', name: 'DeepSeek', providerId: 'deepseek',
  fields: [
    { key: 'DEEPSEEK_API_KEY', label: 'Key', type: 'password', required: true, role: 'api_key' },
    { key: 'DEEPSEEK_BASE_URL', label: 'URL', type: 'text', role: 'base_url' },
    { key: 'DEEPSEEK_MODEL', label: 'Model', type: 'select', role: 'model', options: ['a', 'b'] },
  ],
  verify: { type: 'llm', auth: 'bearer' },
};
const envGroup: InputTemplate = {
  title: 'Server', itemType: 'env_group', name: 'Server', category: 'server',
  fields: [{ key: 'DB_HOST', label: 'Host', type: 'text', value: 'db.local' }, { key: 'DB_PASS', label: 'Pass', type: 'password' }],
};
const withFields = (base: InputTemplate, fields: InputTemplate['fields']): InputTemplate => ({ ...base, fields });
const code = (template: unknown) => precheckInput(template, providers)?.code;

describe('input request ids', () => {
  it('uses its own prefix and is not accepted by other id checks', () => {
    expect(isValidInputRequestId('input-1760000000000-ab12')).toBe(true);
    for (const bad of ['', 'input-', 'input-../x', 'capture-1', 'sync-1', 'plan-1', `input-${'a'.repeat(130)}`]) expect(isValidInputRequestId(bad)).toBe(false);
    for (const check of [isValidCaptureRequestId, isValidSyncRequestId, isValidRequestId]) expect(check('input-1')).toBe(false);
    expect(isValidInputRequestId('capture-1')).toBe(false);
    expect(isValidInputRequestId('sync-1')).toBe(false);
    expect(isValidInputRequestId('plan-1')).toBe(false);
  });
});

describe('precheckInput', () => {
  it('accepts valid login, api_key and env_group templates', () => {
    expect(precheckInput(login, providers)).toBeUndefined();
    expect(precheckInput(apiKey, providers)).toBeUndefined();
    expect(precheckInput(envGroup, providers)).toBeUndefined();
  });

  it('bad_shape', () => {
    expect(code({ ...login, title: '' })).toBe('bad_shape');
    expect(code({ ...login, itemType: 'other' })).toBe('bad_shape');
    expect(code(withFields(login, []))).toBe('bad_shape');
    expect(code({ ...login, verify: { type: 'login', url: 'https://x.example/', bodyUser: 'bad name' } })).toBe('bad_shape');
  });
  it('duplicate_key', () => {
    expect(code(withFields(login, [...login.fields, { key: 'username', label: 'Again', type: 'text' }]))).toBe('duplicate_key');
  });
  it('prefilled_secret', () => {
    expect(code(withFields(login, [login.fields[0], { key: 'password', label: 'P', type: 'password', value: 'fake-pass' }]))).toBe('prefilled_secret');
  });
  it('options_not_select', () => {
    expect(code(withFields(login, [{ ...login.fields[0], options: ['x'] }, login.fields[1]]))).toBe('options_not_select');
  });
  it('bad_key', () => {
    expect(code(withFields(login, [login.fields[0], { key: 'token', label: 'T', type: 'text' }]))).toBe('bad_key');
    expect(code(withFields(envGroup, [{ key: '1BAD', label: 'x', type: 'text' }]))).toBe('bad_key');
    expect(code(withFields(envGroup, [{ key: 'has-dash', label: 'x', type: 'text' }]))).toBe('bad_key');
  });
  it('login_needs_username', () => {
    expect(code({ ...login, verify: undefined, fields: [{ key: 'password', label: 'P', type: 'password' }] })).toBe('login_needs_username');
  });
  it('role_not_allowed', () => {
    expect(code(withFields(login, [login.fields[0], { ...login.fields[1], role: 'api_key' }]))).toBe('role_not_allowed');
  });
  it('duplicate_role', () => {
    expect(code(withFields(envGroup, [{ key: 'A_KEY', label: 'a', type: 'password', role: 'api_key' }, { key: 'B_KEY', label: 'b', type: 'password', role: 'api_key' }]))).toBe('duplicate_role');
  });
  it('api_key_fields', () => {
    expect(code(withFields(apiKey, apiKey.fields.slice(1)))).toBe('api_key_fields');
    expect(code(withFields(apiKey, [{ ...apiKey.fields[0], type: 'text' }, ...apiKey.fields.slice(1)]))).toBe('api_key_fields');
    expect(code(withFields(apiKey, [...apiKey.fields, { key: 'EXTRA', label: 'x', type: 'text' }]))).toBe('api_key_fields');
    expect(code(withFields(apiKey, [apiKey.fields[0], { ...apiKey.fields[1], type: 'select' }]))).toBe('api_key_fields');
    expect(code(withFields(apiKey, [apiKey.fields[0], { key: 'M', label: 'M', type: 'password', role: 'model' }]))).toBe('api_key_fields');
  });
  it('unknown_provider', () => {
    expect(code({ ...apiKey, providerId: 'missing' })).toBe('unknown_provider');
    expect(code({ ...apiKey, providerId: undefined })).toBe('unknown_provider');
    expect(code({ ...envGroup, providerId: 'missing' })).toBe('unknown_provider');
  });
  it('verify_mismatch', () => {
    expect(code({ ...envGroup, verify: { type: 'login', url: 'https://example.com/login' } })).toBe('verify_mismatch');
    expect(code({ ...login, fields: [login.fields[0]] })).toBe('verify_mismatch');
    expect(code({ ...envGroup, verify: { type: 'llm', auth: 'bearer' } })).toBe('verify_mismatch');
    expect(code(withFields(apiKey, apiKey.fields.slice(0, 1)))).toBe('verify_mismatch');
  });
  it('bad_url', () => {
    for (const url of ['http://example.com/login', 'ftp://example.com/', 'not a url', 'https://user:pw@example.com/login']) expect(code({ ...login, verify: { type: 'login', url } })).toBe('bad_url');
    for (const url of ['http://localhost:3000/login', 'http://127.0.0.1/login', 'http://[::1]:8080/login']) expect(code({ ...login, verify: { type: 'login', url } })).toBeUndefined();
  });
});

describe('prefillTemplate', () => {
  it('fills an empty base_url from the provider catalog and keeps user-supplied values', () => {
    expect(prefillTemplate(apiKey, providers).fields[1].value).toBe('https://api.deepseek.com/v1');
    const custom = withFields(apiKey, [apiKey.fields[0], { ...apiKey.fields[1], value: 'https://proxy.example/v1' }, apiKey.fields[2]]);
    expect(prefillTemplate(custom, providers).fields[1].value).toBe('https://proxy.example/v1');
    expect(prefillTemplate({ ...apiKey, providerId: 'nourl' }, providers).fields[1].value).toBeUndefined();
    expect(prefillTemplate(login, providers)).toBe(login);
  });
});

describe('toTemplate', () => {
  it('maps snake_case arguments to the camelCase template', () => {
    const template = toTemplate({ title: 't', item_type: 'login', name: 'n', fields: login.fields, verify: { type: 'login', url: 'https://example.com/l', body_user: 'u', body_pass: 'p', ok_field: 'ok' } });
    expect(template).toEqual({ title: 't', itemType: 'login', name: 'n', fields: login.fields, verify: { type: 'login', url: 'https://example.com/l', bodyUser: 'u', bodyPass: 'p', okField: 'ok' } });
    expect(precheckInput(template, providers)).toBeUndefined();
  });
});

describe('input queue and status', () => {
  const withDirs = (check: (pending: string, results: string) => void) => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-input-'));
    try { check(join(directory, 'pending'), join(directory, 'results')); } finally { rmSync(directory, { recursive: true, force: true }); }
  };

  it('writes a camelCase queue file and reports waiting / not_found', () => {
    withDirs((pending, results) => {
      const id = queueInput(pending, apiKey);
      expect(isValidInputRequestId(id)).toBe(true);
      const file = JSON.parse(readFileSync(join(pending, `${id}.json`), 'utf8'));
      expect(file).toEqual({ version: 1, requestId: id, kind: 'request_input', createdAt: expect.any(String), template: apiKey });
      expect(getInputStatus(id, pending, results)).toEqual({ status: 'waiting', requestId: id });
      expect(getInputStatus('input-missing', pending, results)).toEqual({ status: 'not_found', requestId: 'input-missing' });
    });
  });

  it('maps result files and never leaks extra fields', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      const write = (id: string, body: unknown) => writeFileSync(join(results, `${id}.json`), typeof body === 'string' ? body : JSON.stringify(body));
      write('input-a', { requestId: 'input-a', status: 'saved', itemId: 'item-1', itemType: 'login', value: 'fake-test-secret-0000', password: 'fake-pass', values: { password: 'fake-pass' } });
      write('input-b', { requestId: 'input-b', status: 'cancelled', value: 'fake-test-secret-0000' });
      write('input-c', { requestId: 'input-c', status: 'failed', error: { code: 'expired', detail: 'fake-pass' }, password: 'fake-pass' });
      write('input-d', { requestId: 'input-d', status: 'saved' });
      write('input-e', { requestId: 'input-e', status: 'saved', itemId: 42 });
      write('input-f', { requestId: 'input-f', status: 'failed', error: { code: 'Bad Code!' } });
      write('input-g', { requestId: 'input-g', status: 'failed' });
      write('input-h', '{oops');
      write('input-i', { requestId: 'input-i', status: 'written', value: 'fake-test-secret-0000' });
      write('input-j', 'null');

      const saved = getInputStatus('input-a', pending, results);
      expect(saved).toEqual({ status: 'saved', requestId: 'input-a', itemId: 'item-1', itemType: 'login' });
      const cancelled = getInputStatus('input-b', pending, results);
      expect(cancelled).toEqual({ status: 'cancelled', requestId: 'input-b' });
      const failed = getInputStatus('input-c', pending, results);
      expect(failed).toEqual({ status: 'failed', requestId: 'input-c', error: { code: 'expired' } });
      for (const status of [saved, cancelled, failed, getInputStatus('input-i', pending, results)]) {
        const text = JSON.stringify(status);
        expect(text).not.toContain('fake-test-secret-0000');
        expect(text).not.toContain('fake-pass');
      }
      for (const id of ['input-d', 'input-e', 'input-f', 'input-g', 'input-i', 'input-j']) expect(getInputStatus(id, pending, results)).toEqual({ status: 'failed', requestId: id, error: { code: 'invalid_result' } });
      expect(getInputStatus('input-h', pending, results)).toEqual({ status: 'failed', requestId: 'input-h', error: { code: 'unreadable_result' } });
    });
  });

  it('prefers the result over a still-present pending file', () => {
    withDirs((pending, results) => {
      const id = queueInput(pending, login);
      mkdirSync(results, { recursive: true });
      writeFileSync(join(results, `${id}.json`), JSON.stringify({ requestId: id, status: 'cancelled' }));
      expect(getInputStatus(id, pending, results)).toEqual({ status: 'cancelled', requestId: id });
    });
  });
});
