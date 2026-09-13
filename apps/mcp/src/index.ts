import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { mkdirSync, writeFileSync } from 'node:fs';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';
import { openCatalogStore, recommendForStack } from './catalog.js';

const CHARACTER_LIMIT = 25_000;
const dbPath = process.env.SECRETHUB_DB_PATH ?? join(process.env.APPDATA ?? homedir(), 'com.secrethub.desktop', 'vault.sqlite');
const planDirectory = process.env.SECRETHUB_PLAN_DIR ?? join(dirname(dbPath), 'mcp-pending');
const catalog = openCatalogStore(dbPath);
const server = new McpServer({ name: 'secrethub-mcp-server', version: '0.1.0' });

const responseFormat = z.enum(['json', 'markdown']).default('markdown');
const paging = { limit: z.number().int().min(1).max(100).default(20), offset: z.number().int().min(0).default(0), response_format: responseFormat };

server.registerTool('secrethub_list_secret_catalog', { title: 'List Secret Catalog', description: 'List local SecretHub metadata only. Returns IDs, names, provider, environment keys, tags, status and hasValue; never returns Secret values.', inputSchema: paging, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ limit, offset, response_format }) => catalogResult(catalog.list({ limit, offset }), response_format));
server.registerTool('secrethub_search_secrets', { title: 'Search Secret Metadata', description: 'Search local SecretHub metadata by name, provider, environment key or tag. Secret values are never indexed or returned.', inputSchema: { query: z.string().trim().min(1).max(200), ...paging }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ query, limit, offset, response_format }) => catalogResult(catalog.list({ query, limit, offset }), response_format));
server.registerTool('secrethub_get_secret_metadata', { title: 'Get Secret Metadata', description: 'Get one SecretHub catalog entry by ID. This tool deliberately excludes the encrypted payload and plaintext value.', inputSchema: { id: z.string().min(1).max(200), response_format: responseFormat }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ id, response_format }) => { const item = catalog.get(id); const result = item ? { found: true, item } : { found: false, id }; return catalogResult(result, response_format); });
server.registerTool('secrethub_list_profiles', { title: 'List SecretHub Profiles', description: 'List saved profiles and Secret IDs without revealing values.', inputSchema: { response_format: responseFormat }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ response_format }) => catalogResult({ profiles: catalog.profiles() }, response_format));
server.registerTool('secrethub_list_projects', { title: 'List SecretHub Projects', description: 'List local project history with paths and Secret IDs are intentionally omitted from this metadata view.', inputSchema: { response_format: responseFormat }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ response_format }) => catalogResult({ projects: catalog.projects() }, response_format));
server.registerTool('secrethub_recommend_secrets_for_project', { title: 'Recommend Secrets for Project', description: 'Map a project stack to expected environment keys and report which catalog entries are available or missing. No network probe and no plaintext access.', inputSchema: { stack: z.array(z.string().trim().min(1).max(80)).min(1).max(30), response_format: responseFormat }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ stack, response_format }) => catalogResult(recommendForStack(stack, catalog), response_format));
server.registerTool('secrethub_prepare_project_env', { title: 'Prepare Project Environment Plan', description: 'Queue a metadata-only project environment proposal for SecretHub Desktop. The queue contains no Secret values; Desktop must show a masked preview and receive user confirmation before writing files.', inputSchema: { project_path: z.string().min(1).max(400), secret_ids: z.array(z.string().min(1).max(200)).min(1).max(100), response_format: responseFormat }, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false } }, ({ project_path, secret_ids, response_format }) => { const requestId = queuePlan({ kind: 'prepare_project_env', projectPath: project_path, secretIds: secret_ids }); return catalogResult({ requestId, projectPath: project_path, secretIds: secret_ids, requiresUserConfirmation: true, written: false, nextStep: 'Open SecretHub Desktop and approve the masked preview.' }, response_format); });
server.registerTool('secrethub_validate_secret', { title: 'Validate Secret Metadata', description: 'Report the locally stored validation status for a catalog entry. It does not make a network request and never decrypts or returns the Secret value.', inputSchema: { id: z.string().min(1).max(200), response_format: responseFormat }, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, ({ id, response_format }) => { const item = catalog.get(id); return catalogResult(item ? { id, status: item.status, networkCheckPerformed: false, nextStep: 'Use SecretHub Desktop to request an explicit provider validation.' } : { found: false, id }, response_format); });
server.registerTool('secrethub_apply_profile', { title: 'Apply Profile Plan', description: 'Queue a metadata-only profile application plan. SecretHub Desktop must show the masked preview and receive user confirmation before any project write; this MCP tool never writes project files.', inputSchema: { profile_id: z.string().min(1).max(200), project_path: z.string().min(1).max(400), response_format: responseFormat }, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false } }, ({ profile_id, project_path, response_format }) => { const profile = catalog.profile(profile_id); if (!profile) return catalogResult({ found: false, profileId: profile_id }, response_format); const requestId = queuePlan({ kind: 'apply_profile', projectPath: project_path, secretIds: profile.secretIds }); return catalogResult({ requestId, profileId: profile.id, projectPath: project_path, secretIds: profile.secretIds, requiresUserConfirmation: true, written: false }, response_format); });

function catalogResult(value: Record<string, unknown>, format: 'json' | 'markdown') { const text = format === 'json' ? JSON.stringify(value, null, 2) : markdown(value); return { content: [{ type: 'text' as const, text: text.slice(0, CHARACTER_LIMIT) }], structuredContent: value }; }
function markdown(value: unknown) { return '# SecretHub\n\n```json\n' + JSON.stringify(value, null, 2) + '\n```'; }

function queuePlan(input: { kind: string; projectPath: string; secretIds: string[] }) {
  const requestId = `plan-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  mkdirSync(planDirectory, { recursive: true });
  writeFileSync(join(planDirectory, `${requestId}.json`), JSON.stringify({ requestId, ...input, createdAt: new Date().toISOString() }) + '\n', { encoding: 'utf8', flag: 'wx' });
  return requestId;
}

async function main() { const transport = new StdioServerTransport(); await server.connect(transport); }
main().catch((error) => { process.stderr.write(`SecretHub MCP server failed: ${String(error)}\n`); process.exitCode = 1; });
