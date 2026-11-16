import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { z } from 'zod';
import { MAX_IMPORT_PATHS } from './import.js';

// AI-organized import (design 2026-11-10). Two queue kinds:
//   inspect-*.json  paths only; Desktop answers with a masked view of the files (inspect-*.json in the results).
//   organize-*.json the AI's plan; values are placeholders of the masked view or text that appears in it.
// No real secret value ever passes through this server: the view is masked by Desktop and the plan can only
// refer to placeholders. Results hold names, counts and reason codes.

export const MAX_PLAN_ITEMS = 50;
export const MAX_PLAN_VARIABLES = 200;
const INSPECTION_TTL_MS = 30 * 60 * 1000;
const DEFAULT_VIEW_CHARS = 18_000;

export function isValidInspectRequestId(id: string): boolean { return id.length <= 128 && /^inspect-[A-Za-z0-9_-]+$/.test(id); }
export function isValidOrganizeRequestId(id: string): boolean { return id.length <= 128 && /^organize-[A-Za-z0-9_-]+$/.test(id); }

// `S3`, `«S3»` or `«S3:12 位»`, exactly as shown in the masked view.
const tokenRef = z.string().trim().regex(/^«?S[1-9][0-9]{0,5}(:[^»]{0,40})?»?$/, 'Use a placeholder from the masked view, for example S3');
const planValue = z.union([z.object({ token: tokenRef }).strict(), z.object({ literal: z.string().min(1).max(2000) }).strict()]);
export const planVariableSchema = z.object({
  name: z.string().regex(/^[A-Za-z_][A-Za-z0-9_]{0,127}$/, 'Variable names are letters, digits and underscores'),
  value: planValue,
  secret: z.boolean().optional(),
  description: z.string().max(200).optional(),
}).strict();
export const planItemSchema = z.object({
  type: z.enum(['env_group', 'api_key', 'login', 'note']),
  name: z.string().trim().min(1).max(100),
  category: z.enum(['server', 'database', 'storage', 'other']).optional(),
  providerId: z.string().trim().min(1).max(100).optional(),
  variables: z.array(planVariableSchema).max(MAX_PLAN_VARIABLES).optional(),
  notes: z.string().max(20_000).optional(),
}).strict();
export type PlanItem = z.infer<typeof planItemSchema>;

const LOGIN_FIELDS = new Set(['website', 'username', 'password', 'totp', 'notes']);

// Shape checks only; Desktop decides what each value may be and rejects the rest with a reason.
export function precheckOrganizePlan(inspectionId: string, items: PlanItem[], resultDirectory: string): string | undefined {
  if (!isValidInspectRequestId(inspectionId)) return 'inspection_id must be the requestId returned by secrethub_import_inspect.';
  if (!existsSync(join(resultDirectory, `${inspectionId}.json`))) return 'Unknown inspection_id: call secrethub_import_inspect, wait until secrethub_get_import_inspection reports done, then use that id.';
  if (items.length === 0 || items.length > MAX_PLAN_ITEMS) return `Provide between 1 and ${MAX_PLAN_ITEMS} items.`;
  for (const [index, item] of items.entries()) {
    const variables = item.variables ?? [];
    if (item.type === 'env_group' && variables.length === 0) return `Item ${index}: an env_group needs at least one variable.`;
    if (item.type === 'api_key' && (!item.providerId || variables.length === 0)) return `Item ${index}: an api_key needs providerId and its key variable.`;
    if (item.type === 'login' && variables.some((variable) => !LOGIN_FIELDS.has(variable.name))) return `Item ${index}: login variables must be named website, username, password, totp or notes.`;
    if (item.type === 'note' && !item.notes?.trim()) return `Item ${index}: a note needs notes text.`;
    const names = new Set<string>();
    for (const variable of variables) {
      if (names.has(variable.name)) return `Item ${index}: duplicate variable ${variable.name}.`;
      names.add(variable.name);
      if ('literal' in variable.value && /[«»]/.test(variable.value.literal)) return `Item ${index}: write a placeholder as { "token": "S3" }, not inside a literal.`;
    }
  }
  return undefined;
}

function queueFile(directory: string, prefix: string, kind: string, fields: Record<string, unknown>): string {
  const requestId = `${prefix}${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(directory, { recursive: true });
  const payload = { version: 1, requestId, kind, ...fields, createdAt: new Date().toISOString() };
  writeFileSync(join(directory, `${requestId}.json`), JSON.stringify(payload) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

export function precheckInspect(paths: string[]): string | undefined {
  if (paths.length === 0) return 'Provide at least one file or folder path.';
  if (paths.length > MAX_IMPORT_PATHS) return `At most ${MAX_IMPORT_PATHS} paths are allowed.`;
  for (const path of paths) {
    if (/[\u0000-\u001f\u007f]/.test(path)) return 'Paths must not contain control characters.';
    if (!isAbsolute(path)) return 'Every path must be absolute.';
  }
  return undefined;
}

export function queueInspect(directory: string, paths: string[]): string {
  return queueFile(directory, 'inspect-', 'import_inspect', { paths: [...new Set(paths)] });
}

export function queueOrganize(directory: string, inspectionId: string, items: PlanItem[]): string {
  return queueFile(directory, 'organize-', 'import_organize', { inspectionId, items });
}

const count = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0);
const text = (value: unknown, limit: number) => (typeof value === 'string' ? value.slice(0, limit) : '');
const stringList = (value: unknown, limit: number, each: number): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string').slice(0, limit).map((item) => item.slice(0, each)) : []);

function readResult(path: string): Record<string, unknown> | undefined {
  try { const value = JSON.parse(readFileSync(path, 'utf8')); return value && typeof value === 'object' ? (value as Record<string, unknown>) : undefined; } catch { return undefined; }
}

export type InspectionPage =
  | { status: 'pending' | 'not_found' | 'expired'; requestId: string }
  | { status: 'failed'; requestId: string; error: { code: string } }
  | { status: 'done'; requestId: string; inspectionId: string; expiresInSeconds: number; tokenCount: number; truncated: boolean; totalLines: number; offset: number; nextOffset: number | null; files: Array<{ name: string; startLine: number; lines: string[] }>; skipped: Array<{ name: string; reason: string }> };

// The masked view, paged by line so that large folders fit into the response limit. Only whitelisted fields
// are copied from the result file.
export function getInspection(requestId: string, pendingDirectory: string, resultDirectory: string, offset = 0, maxChars = DEFAULT_VIEW_CHARS, now = Date.now()): InspectionPage {
  const result = readResult(join(resultDirectory, `${requestId}.json`));
  if (!result) return existsSync(join(pendingDirectory, `${requestId}.json`)) ? { status: 'pending', requestId } : { status: 'not_found', requestId };
  if (result.status !== 'done') return { status: 'failed', requestId, error: { code: text((result.error as { code?: unknown } | undefined)?.code, 64) || 'unknown' } };
  const finished = Date.parse(text(result.finishedAt, 40));
  if (Number.isFinite(finished) && now - finished > INSPECTION_TTL_MS) return { status: 'expired', requestId };
  const files = Array.isArray(result.files) ? result.files : [];
  const out: Array<{ name: string; startLine: number; lines: string[] }> = [];
  let total = 0;
  let used = 0;
  let next: number | null = null;
  for (const file of files as Array<Record<string, unknown>>) {
    const name = text(file.name, 200);
    const lines = stringList(file.lines, 20_000, 2_000);
    let block: string[] | undefined;
    for (const [index, line] of lines.entries()) {
      const global = total + index;
      if (global < offset) continue;
      if (next !== null) continue;
      if (used + line.length + 1 > maxChars && used > 0) { next = global; continue; }
      used += line.length + 1;
      if (!block) { block = []; out.push({ name, startLine: global, lines: block }); }
      block.push(line);
    }
    total += lines.length;
  }
  const skipped = (Array.isArray(result.skipped) ? result.skipped : []).slice(0, 200).map((item: Record<string, unknown>) => ({ name: text(item?.name, 200), reason: text(item?.reason, 64) }));
  return { status: 'done', requestId, inspectionId: requestId, expiresInSeconds: count(result.expiresInSeconds), tokenCount: count(result.tokenCount), truncated: result.truncated === true, totalLines: total, offset, nextOffset: next, files: out, skipped };
}

export type OrganizeStatus =
  | { status: 'pending' | 'not_found'; requestId: string }
  | { status: 'failed'; requestId: string; error: { code: string } }
  | { status: 'done'; requestId: string; batchId: string | null; counts: Record<string, number>; items: Array<Record<string, unknown>>; rejected: Array<{ item: number; variable?: string; reason: string }> };

const COUNT_KEYS = ['createdGroups', 'addedVariables', 'savedSeparately', 'otherItems', 'skippedVariables'];

export function getOrganizeStatus(requestId: string, pendingDirectory: string, resultDirectory: string): OrganizeStatus {
  const result = readResult(join(resultDirectory, `${requestId}.json`));
  if (!result) return existsSync(join(pendingDirectory, `${requestId}.json`)) ? { status: 'pending', requestId } : { status: 'not_found', requestId };
  if (result.status !== 'done') return { status: 'failed', requestId, error: { code: text((result.error as { code?: unknown } | undefined)?.code, 64) || 'unknown' } };
  const counts = (result.counts ?? {}) as Record<string, unknown>;
  const items = (Array.isArray(result.items) ? result.items : []).slice(0, MAX_PLAN_ITEMS).map((item: Record<string, unknown>) => ({ index: count(item?.index), name: text(item?.name, 100), type: text(item?.type, 32), result: text(item?.result, 32), addedVariables: count(item?.addedVariables), savedSeparately: count(item?.savedSeparately), skippedVariables: count(item?.skippedVariables) }));
  const rejected = (Array.isArray(result.rejected) ? result.rejected : []).slice(0, 500).map((item: Record<string, unknown>) => ({ item: count(item?.item), ...(typeof item?.variable === 'string' && /^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(item.variable) ? { variable: item.variable } : {}), reason: text(item?.reason, 64) }));
  return { status: 'done', requestId, batchId: typeof result.batchId === 'string' ? result.batchId.slice(0, 80) : null, counts: Object.fromEntries(COUNT_KEYS.map((key) => [key, count(counts[key])])), items, rejected };
}
