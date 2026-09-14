import { invoke } from '@tauri-apps/api/core';

export type BackendSecret = { id: string; name: string; provider_id: string; env_key: string; description: string; tags: string[]; status: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; created_at: number; updated_at: number };
export type VaultStatus = { initialized: boolean; unlocked: boolean };
export type PendingPlan = { request_id: string; kind: string; project_path: string; secret_ids: string[]; created_at: string };

export function isTauriRuntime() { return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window; }

export const backend = {
  status: () => invoke<VaultStatus>('vault_status'),
  setupMaster: (password: string) => invoke<void>('setup_master', { password }),
  unlock: (password: string) => invoke<void>('unlock', { password }),
  lock: () => invoke<void>('lock'),
  list: (query?: string) => invoke<BackendSecret[]>('secret_list', { query }),
  create: (request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string }) => invoke<string>('secret_create', { request }),
  update: (id: string, request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string }) => invoke<void>('secret_update', { id, request }),
  remove: (id: string) => invoke<boolean>('secret_delete', { id }),
  importPreview: (request: { format: 'env' | 'json'; contents: string; confirmed: false }) => invoke<string[]>('secret_import_preview', { request }),
  importSecrets: (request: { format: 'env' | 'json'; contents: string; confirmed: true }) => invoke<string[]>('secret_import', { request }),
  copy: (id: string) => invoke<void>('secret_copy', { id }),
  copyAll: (id: string) => invoke<void>('secret_copy_all', { id }),
  reveal: (id: string) => invoke<void>('secret_reveal', { id }),
  openExternalUrl: (url: string) => invoke<void>('open_external_url', { url }),
  preview: (request: { directory: string; secret_ids: string[]; write_example: boolean; replace_existing: boolean; ensure_gitignore: boolean }) => invoke<string>('export_preview', { request }),
  export: (request: { directory: string; secret_ids: string[]; write_example: boolean; replace_existing: boolean; ensure_gitignore: boolean }) => invoke<void>('export_env', { request }),
  conflicts: (request: { directory: string; secret_ids: string[]; write_example: boolean; replace_existing: boolean; ensure_gitignore: boolean }) => invoke<string[]>('export_conflicts', { request }),
  chooseFolder: () => invoke<string | null>('choose_project_directory'),
  profiles: () => invoke<Array<{ id: string; name: string; description: string; secret_ids: string[] }>>('profile_list'),
  saveProfile: (profile: { id: string; name: string; description: string; secret_ids: string[] }) => invoke<void>('profile_save', { request: profile }),
  projects: () => invoke<Array<{ id: string; path: string; display_name: string; last_used_at: number }>>('project_list'),
  recordProject: (path: string, display_name: string) => invoke<void>('project_record', { path, display_name }),
  audit: () => invoke<Array<{ id: string; operation: string; result: string; created_at: number }>>('audit_list'),
  backup: (path: string) => invoke<void>('backup_export', { path }),
  pendingPlans: () => invoke<PendingPlan[]>('mcp_pending_plans'),
  confirmPlan: (request_id: string, confirmed: boolean) => invoke<void>('mcp_confirm_plan', { requestId: request_id, confirmed }),
};
