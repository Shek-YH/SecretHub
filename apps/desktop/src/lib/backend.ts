import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import type { EffectiveModels } from '../providers';

/** Dispatched on window whenever a backend call fails because the vault auto-locked. */
export const AUTO_LOCK_EVENT = 'secrethub:auto-locked';

export function isAutoLockError(error: unknown): boolean {
  return String(error).includes('auto-locked');
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    if (isAutoLockError(error)) window.dispatchEvent(new Event(AUTO_LOCK_EVENT));
    throw error;
  }
}

export type BackendSecret = { id: string; name: string; provider_id: string; env_key: string; description: string; tags: string[]; status: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; endpoint_url: string; endpoint_env_key: string; models_synced_at: number; models_sync_status: string; created_at: number; updated_at: number; validated_at: number; subtitle?: string; dev_use?: boolean };
/** Create/update payloads of typed items. `fields` carries only what the user typed; secrets are never read back. */
export type ItemCreateRequest = { itemType: string; name: string; fields: Record<string, string>; tags: string[]; favorite: boolean; devUse?: boolean; envKey?: string };
export type ItemUpdateRequest = { name: string; fields: Record<string, string>; clear: string[]; tags: string[]; favorite: boolean; devUse?: boolean; envKey?: string };
export type ImportPreviewRow = { label: string; generic: boolean; keys: string[] };
export type ImportOutcome = { ids: string[]; apiKeyIds: string[] };
export type BackendSettings = { locale: string; theme: string; auto_lock_minutes: number; clipboard_timeout_seconds: number; require_auth_before_export: boolean; require_auth_before_reveal: boolean };
export type VaultStatus = { initialized: boolean; unlocked: boolean };
export type PendingPlan = { request_id: string; kind: string; project_path: string; secret_ids: string[]; version?: number; bindings?: unknown[]; created_at: string };
export type PlanPreviewEntry = { key: string; secretName?: string; kind: 'api_key' | 'base_url' | 'model' | 'v1'; display: string; capability?: string; conflict: boolean; warnings: string[] };
export type PlanPreview = { requestId: string; projectPath: string; version: number; entries: PlanPreviewEntry[]; errors: Array<{ code: string; message: string }> };
export type CapabilitySuggestion = { modelId: string; capabilities: string[]; evidenceUrl?: string; current?: { capabilities: string[]; source: string } };
export type CapabilitySuggestionBatch = { requestId: string; providerId: string; createdAt: string; suggestions: CapabilitySuggestion[] };
export type BackendAuditEvent = { id: string; operation: string; secret_id?: string | null; result: string; created_at: number };
export type BackendValidationResult = { id: string; status: string; models_sync_status: string };
export type BackendModel = { model_id: string; display_name: string; input: string[]; output: string[]; capabilities: string[]; capability_source: string; synced_at: number };
export type CapabilityModelRow = { capability: string; model_id: string; env_key: string };
export type PendingSyncRequest = { requestId: string; secretCount: number | null; createdAt: string };
export type CaptureStatus = { state: 'idle' | 'waiting' | 'captured' | 'timeout' | 'cancelled'; providerId: string | null; remainingSeconds: number; secretId: string | null; validation: string | null };
export type CaptureRequest = { requestId: string; providerId: string; providerName: string; createdAt: string };
export type MediaAsset = { id: string; kind: string; filename: string; stored_path: string; sha256: string; size_bytes: number; mime: string; width: number | null; height: number | null; duration_ms: number | null; description: string; tags: string[]; source: string; created_at: number };
export type MediaListing = { assets: MediaAsset[]; count: number; usage_bytes: number; ffprobe: boolean };
export type MediaImportItem = { path: string; ok: boolean; id: string | null; duplicate: boolean; error: string | null };
export type MediaRequest = { requestId: string; path: string; filename: string; sizeBytes: number | null; problem: string | null; description: string; tags: string[]; createdAt: string };
export type BackendModelSync = { id: string; status: string; count: number };
export type ProjectEnvEntry = { original_key: string; key: string; value: string; source: 'managed' | 'manual'; secret_id?: string | null };

export type CatalogLastCheck = { at: number; result: 'updated' | 'up_to_date' | 'failed'; code?: string | null; detail: string };
export type CatalogStatus = { serial: number; publishedAt: string; source: 'bundled' | 'remote'; urlConfigured: boolean; autoUpdate: boolean; lastCheck?: CatalogLastCheck | null };
export type CatalogCheckResult = { result: 'updated' | 'up_to_date' | 'not_configured' | 'failed'; detail: string; status: CatalogStatus };

export function isTauriRuntime() { return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window; }

export const backend = {
  status: () => invoke<VaultStatus>('vault_status'),
  setupMaster: (password: string) => invoke<void>('setup_master', { password }),
  unlock: (password: string) => invoke<void>('unlock', { password }),
  lock: () => invoke<void>('lock'),
  touch: () => invoke<void>('vault_touch'),
  list: (query?: string) => invoke<BackendSecret[]>('secret_list', { query }),
  create: (request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; endpoint_url: string; endpoint_env_key: string }) => invoke<string>('secret_create', { request }),
  update: (id: string, request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; endpoint_url: string; endpoint_env_key: string }) => invoke<void>('secret_update', { id, request }),
  remove: (id: string) => invoke<boolean>('secret_delete', { id }),
  itemCreate: (request: ItemCreateRequest) => invoke<string>('item_create', { request }),
  itemUpdate: (id: string, request: ItemUpdateRequest) => invoke<void>('item_update', { id, request }),
  itemPublicFields: (id: string) => invoke<Record<string, string>>('item_public_fields', { id }),
  itemSecretFields: (id: string) => invoke<string[]>('item_secret_fields', { id }),
  itemFieldReveal: (id: string, field: string) => invoke<void>('item_field_reveal', { id, field }),
  itemFieldCopy: (id: string, field: string) => invoke<void>('item_field_copy', { id, field }),
  itemNoteBody: (id: string) => invoke<string>('item_note_body', { id }),
  changePassword: (current: string, newPassword: string) => invoke<void>('vault_change_password', { current, newPassword }),
  importPreview: (request: { format: 'env' | 'json'; contents: string; confirmed: false }) => invoke<ImportPreviewRow[]>('secret_import_preview', { request }),
  importSecrets: (request: { format: 'env' | 'json'; contents: string; confirmed: true }) => invoke<ImportOutcome>('secret_import', { request }),
  chooseImportFile: () => invoke<string | null>('choose_import_file'),
  importFilePreview: (path: string) => invoke<ImportPreviewRow[]>('secret_import_file_preview', { path }),
  importFile: (path: string) => invoke<ImportOutcome>('secret_import_file', { path }),
  copy: (id: string) => invoke<void>('secret_copy', { id }),
  copyAll: (id: string) => invoke<void>('secret_copy_all', { id }),
  reveal: (id: string) => invoke<void>('secret_reveal', { id }),
  validate: (id: string) => invoke<string>('secret_validate', { id }),
  validateMany: (onlyStale = false) => invoke<BackendValidationResult[]>('secret_validate_many', { onlyStale }),
  syncModels: (id: string) => invoke<BackendModelSync>('secret_sync_models', { id }),
  models: (id: string) => invoke<BackendModel[]>('secret_models', { id }),
  capabilityModels: (id: string) => invoke<CapabilityModelRow[]>('secret_capability_models', { id }),
  setCapabilityModels: (id: string, rows: CapabilityModelRow[]) => invoke<void>('secret_set_capability_models', { id, rows }),
  openExternalUrl: (url: string) => invoke<void>('open_external_url', { url }),
  preview: (request: { directory: string; secret_ids: string[]; write_example: boolean; replace_existing: boolean; ensure_gitignore: boolean }) => invoke<string>('export_preview', { request }),
  export: (request: { directory: string; secret_ids: string[]; write_example: boolean; replace_existing: boolean; ensure_gitignore: boolean }) => invoke<void>('export_env', { request }),
  conflicts: (request: { directory: string; secret_ids: string[]; write_example: boolean; replace_existing: boolean; ensure_gitignore: boolean }) => invoke<string[]>('export_conflicts', { request }),
  chooseFolder: () => invoke<string | null>('choose_project_directory'),
  profiles: () => invoke<Array<{ id: string; name: string; description: string; secret_ids: string[] }>>('profile_list'),
  saveProfile: (profile: { id: string; name: string; description: string; secret_ids: string[] }) => invoke<void>('profile_save', { request: profile }),
  deleteProfile: (id: string) => invoke<boolean>('profile_delete', { id }),
  projects: () => invoke<Array<{ id: string; path: string; display_name: string; last_used_at: number }>>('project_list'),
  recordProject: (path: string, display_name: string) => invoke<{ id: string; path: string; display_name: string; last_used_at: number }>('project_record', { path, display_name }),
  openProjectDirectory: (path: string) => invoke<void>('open_project_directory', { path }),
  projectEnvPreview: (path: string) => invoke<ProjectEnvEntry[]>('project_env_preview', { path }),
  projectEnvSave: (request: { directory: string; entries: ProjectEnvEntry[] }) => invoke<void>('project_env_save', { request }),
  tagList: () => invoke<string[]>('tag_list'),
  tagCreate: (name: string) => invoke<string>('tag_create', { name }),
  tagDelete: (name: string) => invoke<boolean>('tag_delete', { name }),
  catalogEffective: () => invoke<EffectiveModels>('catalog_effective'),
  catalogStatus: () => invoke<CatalogStatus>('catalog_status'),
  catalogSetAutoUpdate: (enabled: boolean) => invoke<CatalogStatus>('catalog_set_auto_update', { enabled }),
  catalogCheckNow: () => invoke<CatalogCheckResult>('catalog_check_now'),
  audit: () => invoke<BackendAuditEvent[]>('audit_list'),
  backup: (directory: string) => invoke<string>('backup_export', { directory }),
  chooseBackupFile: () => invoke<string | null>('choose_backup_file'),
  restoreBackup: (path: string, password: string) => invoke<void>('backup_restore', { path, password }),
  settingsGet: () => invoke<BackendSettings>('settings_get'),
  settingsUpdate: (settings: BackendSettings) => invoke<void>('settings_update', { settings }),
  pendingPlans: () => invoke<PendingPlan[]>('mcp_pending_plans'),
  planPreview: (request_id: string) => invoke<PlanPreview>('mcp_plan_preview', { requestId: request_id }),
  pendingCapabilitySuggestions: () => invoke<CapabilitySuggestionBatch[]>('mcp_pending_capability_suggestions'),
  resolveCapabilitySuggestions: (request_id: string, accepted_model_ids: string[]) => invoke<void>('mcp_resolve_capability_suggestions', { requestId: request_id, acceptedModelIds: accepted_model_ids }),
  pendingSyncRequests: () => invoke<PendingSyncRequest[]>('mcp_pending_sync_requests'),
  captureStart: (providerId: string) => invoke<CaptureStatus>('key_capture_start', { providerId }),
  captureCancel: () => invoke<CaptureStatus>('key_capture_cancel'),
  captureStatus: () => invoke<CaptureStatus>('key_capture_status'),
  pendingCaptureRequests: () => invoke<CaptureRequest[]>('mcp_pending_capture_requests'),
  captureDecision: (requestId: string, accept: boolean) => invoke<CaptureStatus>('mcp_capture_decision', { requestId, accept }),
  mediaList: () => invoke<MediaListing>('media_list'),
  mediaChooseFiles: () => invoke<string[] | null>('media_choose_files'),
  mediaImport: (paths: string[]) => invoke<MediaImportItem[]>('media_import', { paths }),
  mediaCreateText: (name: string, content: string, description: string, tags: string[]) => invoke<MediaAsset>('media_create_text', { name, content, description, tags }),
  mediaUpdate: (id: string, description: string, tags: string[]) => invoke<boolean>('media_update', { id, description, tags }),
  mediaDelete: (id: string) => invoke<boolean>('media_delete', { id }),
  mediaCopyPath: (id: string) => invoke<void>('media_copy_path', { id }),
  mediaReveal: (id: string) => invoke<void>('media_reveal', { id }),
  mediaThumbnail: (id: string) => invoke<string | null>('media_thumbnail', { id }),
  pendingMediaRequests: () => invoke<MediaRequest[]>('mcp_pending_media_requests'),
  mediaDecision: (requestId: string, accept: boolean) => invoke<void>('mcp_media_decision', { requestId, accept }),
  confirmPlan: (request_id: string, confirmed: boolean, skip_keys?: string[]) => invoke<void>('mcp_confirm_plan', { requestId: request_id, confirmed, skipKeys: skip_keys }),
};
