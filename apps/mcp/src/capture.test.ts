import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { ModelProvider } from './models.js';
import { getCaptureStatus, isValidCaptureRequestId, precheckCapture, queueCapture } from './capture.js';
import { isValidSyncRequestId } from './sync.js';
import { isValidRequestId } from './plan.js';

const providers: ModelProvider[] = [
  { id: 'deepseek', keyUrl: 'https://platform.deepseek.com/api_keys', envKeys: ['DEEPSEEK_API_KEY'] },
  { id: 'plain', envKeys: ['PLAIN_KEY'] },
  { id: 'insecure', keyUrl: 'http://insecure.example/keys', envKeys: ['K'] },
  { id: 'nokey', keyUrl: 'https://nokey.example/keys', envKeys: [] },
];

describe('capture request ids and prechecks', () => {
  it('uses its own prefix', () => {
    expect(isValidCaptureRequestId('capture-1760000000000-ab12')).toBe(true);
    for (const bad of ['', 'capture-', 'capture-../x', 'sync-1', 'plan-1', `capture-${'a'.repeat(130)}`]) expect(isValidCaptureRequestId(bad)).toBe(false);
    expect(isValidSyncRequestId('capture-1')).toBe(false);
    expect(isValidRequestId('capture-1')).toBe(false);
  });
  it('accepts only providers with an https key page and an API key variable', () => {
    expect(precheckCapture('deepseek', providers)).toBeUndefined();
    expect(precheckCapture('missing', providers)).toMatch(/Unknown provider_id/);
    for (const id of ['plain', 'insecure', 'nokey']) expect(precheckCapture(id, providers)).toMatch(/no official API key page/);
  });
});

describe('capture queue and status', () => {
  const withDirs = (check: (pending: string, results: string) => void) => {
    const directory = mkdtempSync(join(tmpdir(), 'secrethub-capture-'));
    try { check(join(directory, 'pending'), join(directory, 'results')); } finally { rmSync(directory, { recursive: true, force: true }); }
  };

  it('queues a provider id only and reports waiting while the user has not decided', () => {
    withDirs((pending, results) => {
      const id = queueCapture(pending, 'deepseek');
      expect(JSON.parse(readFileSync(join(pending, `${id}.json`), 'utf8'))).toEqual(expect.objectContaining({ version: 1, requestId: id, kind: 'start_key_capture', providerId: 'deepseek' }));
      expect(getCaptureStatus(id, pending, results)).toEqual({ status: 'waiting', requestId: id, awaitingUserApproval: true });
      expect(getCaptureStatus('capture-missing', pending, results)).toEqual({ status: 'not_found', requestId: 'capture-missing' });
    });
  });

  it('maps result files to the five states and exposes the secret id only for captured', () => {
    withDirs((pending, results) => {
      mkdirSync(results, { recursive: true });
      const write = (id: string, body: Record<string, unknown>) => writeFileSync(join(results, `${id}.json`), JSON.stringify(body));
      write('capture-a', { requestId: 'capture-a', status: 'waiting', secretId: 'secret-leak?' });
      write('capture-b', { requestId: 'capture-b', status: 'captured', secretId: 'secret-1', value: 'fake-test-key-0123456789abcdef' });
      write('capture-c', { requestId: 'capture-c', status: 'timeout', secretId: 'secret-x' });
      write('capture-d', { requestId: 'capture-d', status: 'declined' });
      write('capture-e', { requestId: 'capture-e', status: 'cancelled' });
      write('capture-f', { requestId: 'capture-f', status: 'captured' });
      write('capture-g', { requestId: 'capture-g', status: 'written' });
      expect(getCaptureStatus('capture-a', pending, results)).toEqual({ status: 'waiting', requestId: 'capture-a' });
      const captured = getCaptureStatus('capture-b', pending, results);
      expect(captured).toEqual({ status: 'captured', requestId: 'capture-b', secretId: 'secret-1' });
      expect(JSON.stringify(captured)).not.toContain('fake-test-key');
      expect(getCaptureStatus('capture-c', pending, results)).toEqual({ status: 'timeout', requestId: 'capture-c' });
      expect(getCaptureStatus('capture-d', pending, results)).toEqual({ status: 'declined', requestId: 'capture-d' });
      expect(getCaptureStatus('capture-e', pending, results)).toEqual({ status: 'cancelled', requestId: 'capture-e' });
      expect(getCaptureStatus('capture-f', pending, results).status).toBe('failed');
      expect(getCaptureStatus('capture-g', pending, results).status).toBe('failed');
      writeFileSync(join(results, 'capture-h.json'), '{oops');
      expect(getCaptureStatus('capture-h', pending, results)).toEqual({ status: 'failed', requestId: 'capture-h', error: { code: 'unreadable_result' } });
    });
  });
});
