import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { ModelProvider } from './models.js';

// Key-capture requests (design M3): queued as capture-*.json holding only a provider id.
// Desktop asks the user before it reads the clipboard; no status here ever contains a key value.
export function isValidCaptureRequestId(id: string): boolean { return id.length <= 128 && /^capture-[A-Za-z0-9_-]+$/.test(id); }

// Returns a problem description, or undefined when the capture can be queued.
export function precheckCapture(providerId: string, providers: ModelProvider[]): string | undefined {
  const provider = providers.find((item) => item.id === providerId);
  if (!provider) return `Unknown provider_id: ${providerId}`;
  let https = false;
  try { https = Boolean(provider.keyUrl) && new URL(provider.keyUrl!).protocol === 'https:'; } catch { https = false; }
  if (!https || !provider.envKeys?.length) return `Provider ${providerId} has no official API key page to open; ask the user to add the key manually in SecretHub Desktop.`;
  return undefined;
}

export function queueCapture(directory: string, providerId: string): string {
  const requestId = `capture-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = { version: 1, requestId, kind: 'start_key_capture', providerId, createdAt: new Date().toISOString() };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

export const CAPTURE_STATES = ['waiting', 'captured', 'timeout', 'cancelled', 'declined'] as const;
export type CaptureStatus =
  | { status: 'waiting'; requestId: string; awaitingUserApproval?: true }
  | { status: 'captured'; requestId: string; secretId: string }
  | { status: 'timeout' | 'cancelled' | 'declined'; requestId: string }
  | { status: 'not_found'; requestId: string }
  | { status: 'failed'; requestId: string; error: { code: string } };

export function getCaptureStatus(requestId: string, pendingDirectory: string, resultDirectory: string): CaptureStatus {
  // Desktop writes the result before removing the pending file, so the result wins.
  const resultPath = join(resultDirectory, `${requestId}.json`);
  if (existsSync(resultPath)) {
    let result: Record<string, unknown>;
    try { result = JSON.parse(readFileSync(resultPath, 'utf8')); } catch { return { status: 'failed', requestId, error: { code: 'unreadable_result' } }; }
    const status = CAPTURE_STATES.find((item) => item === result.status);
    if (!status) return { status: 'failed', requestId, error: { code: 'invalid_result' } };
    // Only whitelisted fields are copied out of the file; the secret id appears on captured only.
    if (status === 'captured') return typeof result.secretId === 'string' && result.secretId ? { status, requestId, secretId: result.secretId.slice(0, 200) } : { status: 'failed', requestId, error: { code: 'invalid_result' } };
    if (status === 'waiting') return { status, requestId };
    return { status, requestId };
  }
  if (existsSync(join(pendingDirectory, `${requestId}.json`))) return { status: 'waiting', requestId, awaitingUserApproval: true };
  return { status: 'not_found', requestId };
}
