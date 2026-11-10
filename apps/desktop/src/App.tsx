import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { getTranslation, readStoredLocale, type Locale } from './i18n/translations';
import { CapabilitySuggestionBanner, CapabilitySuggestionModal, PlanReviewModal } from './McpReview';
import { AiImportNoticeBanner, CaptureRequestBanner, ImportRequestBanner, MediaRequestBanner, OrganizeWaitingBanner, SyncRequestBanner } from './AiRequests';
import { ImportReviewModal } from './ImportReview';
import { MediaLibrary } from './MediaLibrary';
import { CaptureModal } from './KeyCapture';
import { AUTO_LOCK_EVENT, backend, isTauriRuntime, type BackendModel, type BackendSettings, type CapabilityModelRow, type CapabilitySuggestionBatch, type CaptureRequest, type GroupPublic, type ImportApplyOutcome, type ImportReview, type PendingImportRequest, type PendingOrganizeRequest, type AiImportNotice, type ProfileVariables, type MediaRequest, type PendingPlan, type PendingSyncRequest, type PlanPreview, type ProjectEnvEntry, type VaultStatus } from './lib/backend';
import { CatalogSettings } from './CatalogSettings';
import { applyEffectiveCatalog, defaultApiKeyEnvKey, defaultEndpointEnvKey, groupProviders, providerKind, providerTemplates, resolveProviderId, type ProviderTemplate } from './providers';
import { SecretForm } from './SecretForm';
import { BankCardForm } from './items/BankCardForm';
import { ChangePasswordDialog } from './items/ChangePasswordDialog';
import { DevSecretForm } from './items/DevSecretForm';
import { ItemDetail } from './items/ItemDetail';
import { GroupDetail } from './items/GroupDetail';
import { GroupForm, type GroupSavePayload } from './items/GroupForm';
import { variableCountOf } from './items/groupTemplates';
import { ItemTypePicker } from './items/ItemTypePicker';
import { LoginForm } from './items/LoginForm';
import { NoteForm } from './items/NoteForm';
import { TypeIcon } from './items/TypeIcon';
import type { ItemSavePayload } from './items/formTypes';
import { hostOf, isAiVisible, isJsonItem, itemGroup, type ItemKind } from './items/itemTypes';
import { ImportPanel } from './ImportPanel';
import { ExportDialog } from './ExportDialog';
import { RestoreDialog } from './BackupRestore';
import type { CredentialPreset, SecretMetadata, SecretRequest } from './secretTypes';
import { formatRelativeTime } from './time';
import { CAPABILITY_ORDER, CapabilityModelsPanel, DEFAULT_CAPABILITY_ENV, capabilityError, capabilityLabel, needsCapabilitySection, normalizeRows } from './CapabilityModels';

type Profile = { id: string; name: string; description: string; secret_ids: string[]; variables?: ProfileVariables };
type Navigation = 'all' | ItemKind | 'providers' | 'profiles' | 'projects' | 'media' | 'settings' | 'audit';

const IS_MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent);
const SEARCH_SHORTCUT_LABEL = IS_MAC ? '⌘ K' : 'Ctrl K';
const nowSeconds = Math.floor(Date.now() / 1000);
const demoSecrets: SecretMetadata[] = [
  { id: 'openai-main', name: 'OpenAI Main', envKey: 'OPENAI_API_KEY', provider: 'OpenAI', tags: ['ai', 'production'], status: 'valid', updated: '', updatedAt: nowSeconds - 7200, favorite: true, valueType: 'api_key', category: 'ai', scope: 'global' },
  { id: 'anthropic-lab', name: 'Anthropic Lab', envKey: 'ANTHROPIC_API_KEY', provider: 'Anthropic', tags: ['ai', 'research'], status: 'valid', updated: '', updatedAt: nowSeconds - 86400, valueType: 'api_key', category: 'ai', scope: 'global' },
  { id: 'supabase-cuecut', name: 'Supabase CueCut', envKey: 'SUPABASE_URL', provider: 'Supabase', tags: ['database', 'cuecut'], status: 'unknown', updated: '', updatedAt: nowSeconds - 259200, valueType: 'dev_secret', category: 'database', scope: 'project' },
  { id: 'github-build', name: 'GitHub Build', envKey: 'GITHUB_TOKEN', provider: 'GitHub', tags: ['ci', 'developer'], status: 'valid', updated: '', updatedAt: nowSeconds - 432000, valueType: 'dev_secret', category: 'git', scope: 'global' },
];
const KIND_NAVS: Navigation[] = ['api_key', 'dev_secret', 'env_group', 'login', 'bank_card', 'note'];
const demoUsageCounts: Record<string, number> = { 'openai-main': 4, 'anthropic-lab': 2, 'supabase-cuecut': 1, 'github-build': 3 };

export function App() {
  const [locale, setLocale] = useState<Locale>(() => readStoredLocale(localStorage.getItem('secrethub-locale')));
  const [secrets, setSecrets] = useState<SecretMetadata[]>(demoSecrets);
  const [vaultStatus, setVaultStatus] = useState<VaultStatus | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [pendingPlans, setPendingPlans] = useState<PendingPlan[]>([]);
  const [reviewPlan, setReviewPlan] = useState<PendingPlan | null>(null);
  const [capBatches, setCapBatches] = useState<CapabilitySuggestionBatch[]>([]);
  const [reviewCaps, setReviewCaps] = useState<CapabilitySuggestionBatch | null>(null);
  const [pendingSync, setPendingSync] = useState<PendingSyncRequest[]>([]);
  const previousSyncCount = useRef(0);
  const [pendingOrganize, setPendingOrganize] = useState<PendingOrganizeRequest[]>([]);
  const [aiNotices, setAiNotices] = useState<AiImportNotice[]>([]);
  const previousOrganizeCount = useRef(0);
  const [captureRequests, setCaptureRequests] = useState<CaptureRequest[]>([]);
  const [captureName, setCaptureName] = useState<string | null>(null);
  const [mediaRequests, setMediaRequests] = useState<MediaRequest[]>([]);
  const [importRequests, setImportRequests] = useState<PendingImportRequest[]>([]);
  const [aiReview, setAiReview] = useState<ImportReview | null>(null);
  const [mediaRevision, setMediaRevision] = useState(0);
  const [modelsRevision, setModelsRevision] = useState(0);
  const [, setCatalogRevision] = useState(0);
  const [autoLocked, setAutoLocked] = useState(false);
  const [query, setQuery] = useState('');
  const [categoryFilter, setCategoryFilter] = useState('all');
  const [statusFilter, setStatusFilter] = useState('all');
  const [favoriteOnly, setFavoriteOnly] = useState(false);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [selectedId, setSelectedId] = useState(demoSecrets[0].id);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [showExport, setShowExport] = useState(false);
  // Variable subsets carried from a profile into the export dialog.
  const [exportVariables, setExportVariables] = useState<ProfileVariables>({});
  const [showImport, setShowImport] = useState(false);
  const [showProfilePrompt, setShowProfilePrompt] = useState(false);
  const [editingProfile, setEditingProfile] = useState<Profile | null>(null);
  const [profileRevision, setProfileRevision] = useState(0);
  const [editingSecret, setEditingSecret] = useState<SecretMetadata | null>(null);
  const [validationMessage, setValidationMessage] = useState('');
  const [validationRunning, setValidationRunning] = useState(false);
  const [models, setModels] = useState<BackendModel[]>([]);
  const [capRows, setCapRows] = useState<CapabilityModelRow[]>([]);
  const [modelSyncing, setModelSyncing] = useState(false);
  const [modelMessage, setModelMessage] = useState('');
  const [actionMessage, setActionMessage] = useState('');
  const [activeNav, setActiveNav] = useState<Navigation>('all');
  const [credentialPreset, setCredentialPreset] = useState<CredentialPreset | null>(null);
  // "What do you want to save?" step and the typed forms; `editFields` holds the non-secret fields (and the note body) of the item being edited.
  const [addStep, setAddStep] = useState<'picker' | Exclude<ItemKind, 'api_key'> | null>(null);
  const [editFields, setEditFields] = useState<Record<string, string>>({});
  const [editGroup, setEditGroup] = useState<GroupPublic | undefined>(undefined);
  
  const [sortMode, setSortMode] = useState<'updated' | 'usage' | 'name'>('updated');
  const [usageCounts, setUsageCounts] = useState<Record<string, number>>(demoUsageCounts);
  const t = getTranslation(locale);
  const selected = secrets.find((secret) => secret.id === selectedId) ?? null;
  useEffect(() => { setActionMessage(''); setValidationMessage(''); setModelMessage(''); }, [selectedId]);
  const selectedSyncedAt = secrets.find((item) => item.id === selectedId)?.modelsSyncedAt;
  useEffect(() => {
    if (!isTauriRuntime()) { setModels([]); return; }
    let cancelled = false;
    backend.models(selectedId).then((items) => { if (!cancelled) setModels(items); }).catch(() => { if (!cancelled) setModels([]); });
    return () => { cancelled = true; };
  }, [selectedId, selectedSyncedAt, modelsRevision]);
  useEffect(() => {
    if (!isTauriRuntime()) { setCapRows([]); return; }
    let cancelled = false;
    backend.capabilityModels(selectedId).then((rows) => { if (!cancelled) setCapRows(rows); }).catch(() => { if (!cancelled) setCapRows([]); });
    return () => { cancelled = true; };
  }, [selectedId, selectedSyncedAt, modelsRevision]);
  // Ctrl+K (Cmd+K on macOS) jumps to the credential search, switching back to the list view first if needed.
  const searchRef = useRef<HTMLInputElement>(null);
  const pendingSearchFocus = useRef(false);
  const locked = Boolean(vaultStatus && !vaultStatus.unlocked);
  const listViewActive = !locked && !['media', 'settings', 'providers', 'profiles', 'projects', 'audit'].includes(activeNav);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== 'k' || !(IS_MAC ? event.metaKey : event.ctrlKey) || event.altKey || locked) return;
      event.preventDefault();
      if (listViewActive) searchRef.current?.focus();
      else { pendingSearchFocus.current = true; setActiveNav('all'); }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [listViewActive, locked]);
  useEffect(() => {
    if (pendingSearchFocus.current && listViewActive) { pendingSearchFocus.current = false; searchRef.current?.focus(); }
  }, [listViewActive]);
  const filtered = useMemo(() => {
    const result = secrets.filter((secret) => `${secret.name} ${secret.envKey} ${secret.provider} ${secret.subtitle ?? ''} ${secret.tags.join(' ')}`.toLowerCase().includes(query.toLowerCase()) && (!KIND_NAVS.includes(activeNav) || itemGroup(secret) === activeNav) && (categoryFilter === 'all' || secret.category === categoryFilter) && (statusFilter === 'all' || secret.status === statusFilter) && (!favoriteOnly || Boolean(secret.favorite)) && (includeArchived || !secret.archived));
    return [...result].sort((left, right) => sortMode === 'name' ? left.name.localeCompare(right.name) : sortMode === 'usage' ? (usageCounts[right.id] ?? 0) - (usageCounts[left.id] ?? 0) : 0);
  }, [activeNav, categoryFilter, favoriteOnly, includeArchived, query, secrets, sortMode, statusFilter, usageCounts]);
  // Personal items (bank cards, notes, ordinary logins) can never be exported, so they are not selectable.
  const exportable = filtered.filter(isAiVisible);
  const allVisibleSelected = exportable.length > 0 && exportable.every((secret) => selectedIds.includes(secret.id));

  async function refreshSecrets() {
    if (!isTauriRuntime()) return;
    try {
      const [items, audit] = await Promise.all([backend.list(query), backend.audit()]);
      const counts = audit.reduce<Record<string, number>>((result, event) => { if (event.operation === 'env_exported' && event.secret_id) result[event.secret_id] = (result[event.secret_id] ?? 0) + 1; return result; }, {});
      setUsageCounts(counts);
      setSecrets(items.map((secret) => ({ id: secret.id, name: secret.name, envKey: secret.env_key, provider: secret.provider_id, description: secret.description, tags: secret.tags, status: secret.status === 'valid' ? 'valid' : secret.status, updated: '', updatedAt: secret.updated_at, validatedAt: secret.validated_at, valueType: secret.value_type, category: secret.category, scope: secret.scope, favorite: secret.favorite, archived: secret.archived, modelId: secret.model_id, modelEnvKey: secret.model_env_key, endpointUrl: secret.endpoint_url, endpointEnvKey: secret.endpoint_env_key, modelsSyncedAt: secret.models_synced_at, modelsSyncStatus: secret.models_sync_status, subtitle: secret.subtitle ?? '', devUse: secret.dev_use ?? false, aiValues: secret.ai_values ?? true })));
      setRuntimeError(null);
    } catch (error) { setRuntimeError(String(error)); }
  }

  // Provider templates come from the effective catalog in the desktop app; demo/test mode keeps the static import.
  const appliedCatalogSerial = useRef<number | null>(null);
  const reloadCatalog = () => { backend.catalogEffective().then((effective) => { if (appliedCatalogSerial.current === effective.serial) return; appliedCatalogSerial.current = effective.serial; applyEffectiveCatalog(effective); setCatalogRevision((value) => value + 1); }).catch(() => undefined); };
  // The backend may apply a verified remote catalog in the background; a cheap local poll picks it up.
  useEffect(() => {
    if (!isTauriRuntime()) return;
    reloadCatalog();
    const timer = window.setInterval(reloadCatalog, 30_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    backend.status().then((status) => { setVaultStatus(status); if (status.unlocked) { void refreshSecrets(); void validateAllApiKeys(true); } else setSecrets([]); }).catch((error) => setRuntimeError(String(error)));
  }, []);

  // Any backend call that hits the idle auto-lock drops back to the unlock screen
  // and discards open forms (they may hold a plaintext value).
  useEffect(() => {
    const onAutoLock = () => { setVaultStatus({ initialized: true, unlocked: false }); setSecrets([]); setShowForm(false); setEditingSecret(null); setAddStep(null); setEditFields({}); setShowExport(false); setReviewPlan(null); setReviewCaps(null); setRuntimeError(null); setAutoLocked(true); };
    window.addEventListener(AUTO_LOCK_EVENT, onAutoLock);
    return () => window.removeEventListener(AUTO_LOCK_EVENT, onAutoLock);
  }, []);

  // Typing or clicking counts as activity for the idle timer (throttled to once a minute).
  useEffect(() => {
    if (!isTauriRuntime() || !vaultStatus?.unlocked) return;
    let last = 0;
    const onActivity = () => { const now = Date.now(); if (now - last < 60_000) return; last = now; void backend.touch().catch(() => undefined); };
    window.addEventListener('keydown', onActivity);
    window.addEventListener('pointerdown', onActivity);
    return () => { window.removeEventListener('keydown', onActivity); window.removeEventListener('pointerdown', onActivity); };
  }, [vaultStatus?.unlocked]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const loadPlans = () => { void backend.pendingPlans().then(setPendingPlans).catch((error) => setRuntimeError(String(error))); void backend.pendingCapabilitySuggestions().then(setCapBatches).catch((error) => setRuntimeError(String(error)));
      // Queued AI sync requests run in the backend once unlocked; when the queue drains, reload what they changed.
      void backend.pendingSyncRequests().then((items) => { setPendingSync(items); if (previousSyncCount.current > 0 && items.length === 0) { void refreshSecretsRef.current(); setModelsRevision((value) => value + 1); } previousSyncCount.current = items.length; }).catch(() => undefined);
      // AI import plans are written by the backend once unlocked; the notices need the unlocked vault.
      void backend.pendingOrganizeRequests().then((items) => { setPendingOrganize(items); if (previousOrganizeCount.current > 0 && items.length === 0) void refreshSecretsRef.current(); previousOrganizeCount.current = items.length; }).catch(() => undefined);
      void backend.aiImportNotices().then(setAiNotices).catch(() => setAiNotices([]));
      void backend.pendingCaptureRequests().then(setCaptureRequests).catch(() => undefined);
      void backend.pendingMediaRequests().then(setMediaRequests).catch(() => undefined);
      void backend.pendingImportRequests().then(setImportRequests).catch(() => undefined); };
    loadPlans();
    const timer = window.setInterval(loadPlans, 2000);
    return () => window.clearInterval(timer);
  }, []);

  const refreshSecretsRef = useRef(refreshSecrets);
  refreshSecretsRef.current = refreshSecrets;

  // Capture opens the official key page and watches the clipboard in the backend; the UI only shows its state.
  async function startCapture(providerId: string, providerName: string) {
    if (!isTauriRuntime()) { setActionMessage(t.capture.desktopOnly); return; }
    try { await backend.captureStart(providerId); setShowForm(false); setCaptureName(providerName); }
    catch (error) { setRuntimeError(`${t.capture.failed}: ${String(error)}`); }
  }

  async function decideCapture(request: CaptureRequest, accept: boolean) {
    try {
      await backend.captureDecision(request.requestId, accept);
      setCaptureRequests((current) => current.filter((item) => item.requestId !== request.requestId));
      if (accept) setCaptureName(request.providerName);
    } catch (error) { setRuntimeError(`${t.capture.failed}: ${String(error)}`); }
  }

  async function viewImport(request: PendingImportRequest) {
    try { setAiReview(await backend.importScan(request.paths, request.requestId)); setRuntimeError(''); }
    catch (reason) { setRuntimeError(`${t.actions.operationFailed}: ${String(reason)}`); }
  }

  async function undoAiImport(notice: AiImportNotice) {
    try {
      const undone = await backend.aiImportUndo(notice.batchId);
      setAiNotices((current) => current.filter((item) => item.batchId !== notice.batchId));
      setActionMessage(t.ai.undone.replace('{items}', String(undone.removedItems)).replace('{vars}', String(undone.removedVariables)));
      void refreshSecrets();
    } catch (reason) { setRuntimeError(`${t.ai.undoFailed}: ${String(reason)}`); }
  }

  async function dismissAiNotice(notice: AiImportNotice) {
    setAiNotices((current) => current.filter((item) => item.batchId !== notice.batchId));
    try { await backend.aiImportDismiss(notice.batchId); } catch { /* the notice comes back on the next poll */ }
  }

  async function declineImport(request: PendingImportRequest) {
    try { await backend.importDecline(request.requestId); setImportRequests((current) => current.filter((item) => item.requestId !== request.requestId)); }
    catch (reason) { setRuntimeError(`${t.actions.operationFailed}: ${String(reason)}`); }
  }

  async function aiImportDone(outcome: ImportApplyOutcome) {
    setAiReview(null);
    for (const id of outcome.apiKeyIds) {
      try { await backend.validate(id); } catch { /* the key stays unchecked; the list shows it */ }
    }
    void refreshSecrets();
  }

  async function decideMedia(request: MediaRequest, accept: boolean) {
    try {
      await backend.mediaDecision(request.requestId, accept);
      setMediaRequests((current) => current.filter((item) => item.requestId !== request.requestId));
      setMediaRevision((value) => value + 1);
    } catch (error) { setRuntimeError(`${t.media.failed}: ${String(error)}`); }
  }

  const captureDone = useRef<(secretId: string) => void>(() => undefined);
  captureDone.current = (secretId: string) => { void refreshSecrets(); setActiveNav('all'); setSelectedId(secretId); setModelsRevision((value) => value + 1); };

  function planReviewed() {
    const id = reviewPlan?.request_id;
    setReviewPlan(null);
    if (id) setPendingPlans((current) => current.filter((item) => item.request_id !== id));
    void refreshSecrets();
  }

  function capsReviewed() {
    const id = reviewCaps?.requestId;
    setReviewCaps(null);
    if (id) setCapBatches((current) => current.filter((item) => item.requestId !== id));
    setModelsRevision((value) => value + 1);
  }

  function runBackendAction(action: () => Promise<unknown>, successMessage?: string) {
    if (!isTauriRuntime()) return;
    void action().then(() => { if (successMessage) setActionMessage(successMessage); }).catch((error) => setActionMessage(`${t.actions.operationFailed}: ${String(error)}`));
  }

  async function validateSecret(id: string) {
    if (!isTauriRuntime()) { setValidationMessage('Validation is available in the desktop app.'); return; }
    try {
      const status = await backend.validate(id);
      setSecrets((current) => current.map((secret) => secret.id === id ? { ...secret, status } : secret));
      setValidationMessage(statusText(status, t));
      void refreshSecrets();
    } catch (error) { setValidationMessage(String(error)); }
  }

  async function syncModels(id: string) {
    if (!isTauriRuntime()) { setModelMessage(t.models.desktopOnly); return; }
    setModelSyncing(true);
    try {
      const result = await backend.syncModels(id);
      setModelMessage(result.status === 'ok' ? `${t.models.ok} · ${result.count}` : modelStatusText(result.status, t));
      await refreshSecrets();
    } catch (error) { setModelMessage(String(error)); }
    finally { setModelSyncing(false); }
  }

  // Reuses secret_update with an empty value, which only rewrites metadata and never touches the encrypted secret.
  async function setDefaultModel(secret: SecretMetadata, modelId: string) {
    if (!isTauriRuntime()) { setModelMessage(t.models.desktopOnly); return; }
    try {
      await backend.update(secret.id, { name: secret.name, provider_id: secret.provider, env_key: secret.envKey, description: secret.description ?? '', tags: secret.tags, value: '', value_type: secret.valueType ?? 'api_key', category: secret.category ?? 'other', scope: secret.scope ?? 'global', favorite: secret.favorite ?? false, archived: secret.archived ?? false, model_id: modelId, model_env_key: secret.modelEnvKey ?? '', endpoint_url: secret.endpointUrl ?? '', endpoint_env_key: secret.endpointEnvKey ?? '' });
      setSecrets((current) => current.map((item) => item.id === secret.id ? { ...item, modelId } : item));
      setModelMessage(`${t.models.defaultSet}: ${modelId}`);
    } catch (error) { setModelMessage(String(error)); }
  }

  // Writes one capability row, keeping the user's env name or falling back to the default; the backend re-validates.
  async function setCapabilityDefault(secret: SecretMetadata, capability: string, modelId: string) {
    if (!isTauriRuntime()) { setModelMessage(t.models.desktopOnly); return; }
    const existing = capRows.find((row) => row.capability === capability);
    const next = normalizeRows([...capRows.filter((row) => row.capability !== capability), { capability, model_id: modelId, env_key: existing?.env_key || DEFAULT_CAPABILITY_ENV[capability] || '' }]);
    const problem = capabilityError(t, next, [secret.envKey, secret.modelEnvKey ?? '', secret.endpointEnvKey ?? '']);
    if (problem) { setModelMessage(problem); return; }
    try { await backend.setCapabilityModels(secret.id, next); setCapRows(next); setModelMessage(`${t.cap.defaultSet}: ${capabilityLabel(t, capability)} · ${modelId}`); } catch (error) { setModelMessage(String(error)); }
  }

  // New API keys: validate and sync right after saving, then show the detail (with the capability section for aggregators).
  async function openAfterSync(id: string) {
    setActiveNav('all'); setSelectedId(id); setModelSyncing(true);
    try { await backend.validate(id); await refreshSecrets(); setModelsRevision((value) => value + 1); setModelMessage(t.cap.autoSyncDone); }
    catch (error) { setModelMessage(String(error)); }
    finally { setModelSyncing(false); }
  }

  // Startup and unlock only re-check keys that were never checked or are a day old (`onlyStale`, silent);
  // the toolbar button checks every API key.
  async function validateAllApiKeys(onlyStale = false) {
    if (!isTauriRuntime()) { if (!onlyStale) setActionMessage(t.actions.validateAllDone); return; }
    if (!onlyStale) { setValidationRunning(true); setActionMessage(t.actions.validatingAll); }
    try {
      const results = await backend.validateMany(onlyStale);
      setSecrets((current) => current.map((secret) => { const result = results.find((item) => item.id === secret.id); return result ? { ...secret, status: result.status, modelsSyncStatus: result.models_sync_status || secret.modelsSyncStatus } : secret; }));
      if (results.length > 0) void refreshSecrets();
      if (!onlyStale) setActionMessage(results.length > 0 ? `${t.actions.validateAllDone} · ${results.length}` : t.actions.validateAllEmpty);
    } catch (error) { if (!onlyStale) setActionMessage(`${t.actions.operationFailed}: ${String(error)}`); }
    finally { if (!onlyStale) setValidationRunning(false); }
  }

  // After a restore the vault holds different data: drop selections and reload everything from the backend.
  function vaultRestored() {
    setAutoLocked(false);
    setVaultStatus({ initialized: true, unlocked: true });
    setSelectedId('');
    setSelectedIds([]);
    setModels([]);
    setCapRows([]);
    setProfileRevision((value) => value + 1);
    setModelsRevision((value) => value + 1);
    void refreshSecrets();
    void validateAllApiKeys(true);
  }

  function changeLocale(next: Locale) {
    setLocale(next);
    localStorage.setItem('secrethub-locale', next);
    document.documentElement.lang = next;
  }

  function toggleSelected(id: string) {
    if (!secrets.some((secret) => secret.id === id && isAiVisible(secret))) return;
    setSelectedIds((current) => current.includes(id) ? current.filter((value) => value !== id) : [...current, id]);
  }

  function toggleSelectAllVisible() {
    const visibleIds = exportable.map((secret) => secret.id);
    setSelectedIds((current) => allVisibleSelected ? current.filter((id) => !visibleIds.includes(id)) : Array.from(new Set([...current, ...visibleIds])));
  }

  function openAdd() { setEditingSecret(null); setCredentialPreset(null); setAddStep('picker'); }

  function pickKind(kind: ItemKind) {
    if (kind === 'api_key') { setAddStep(null); setShowForm(true); } else setAddStep(kind);
  }

  function closeItemForm() { setAddStep(null); setEditingSecret(null); setEditFields({}); setEditGroup(undefined); }

  async function startEdit(secret: SecretMetadata) {
    const group = itemGroup(secret);
    if (group === 'api_key') { setEditingSecret(secret); setShowForm(true); return; }
    if (group === 'env_group') {
      let loaded: GroupPublic | undefined;
      if (isTauriRuntime()) {
        try { loaded = await backend.groupPublic(secret.id); } catch (error) { setActionMessage(`${t.items.loadFailed}: ${String(error)}`); return; }
      }
      setEditGroup(loaded);
      setEditingSecret(secret);
      setAddStep('env_group');
      return;
    }
    let fields: Record<string, string> = {};
    if (isJsonItem(secret.valueType) && isTauriRuntime()) {
      try {
        fields = await backend.itemPublicFields(secret.id);
        // The note body is the one secret shown in a form, and only because the user asked to edit.
        if (group === 'note') fields = { ...fields, body: await backend.itemNoteBody(secret.id) };
      } catch (error) { setActionMessage(`${t.items.loadFailed}: ${String(error)}`); return; }
    }
    setEditFields(fields);
    setEditingSecret(secret);
    setAddStep(group);
  }

  async function saveGroup(payload: GroupSavePayload) {
    if (isTauriRuntime()) {
      if (editingSecret) await backend.groupUpdate(editingSecret.id, payload);
      else {
        const id = await backend.groupCreate({ ...payload, notes: payload.notes ?? '', variables: payload.variables.map((variable) => ({ name: variable.name, secret: variable.secret, value: variable.value ?? '' })) });
        setActiveNav('all');
        setSelectedId(id);
      }
      await refreshSecrets();
      return;
    }
    // Browser demo mode keeps only list-level data.
    const local: SecretMetadata = { id: editingSecret?.id ?? `local-${Date.now()}`, name: payload.name, envKey: '', provider: 'group', tags: payload.tags, status: 'unknown', updated: '', updatedAt: Math.floor(Date.now() / 1000), valueType: 'env_group', category: payload.category, scope: 'global', favorite: payload.favorite, aiValues: payload.aiValues, subtitle: `${payload.variables.length} 个变量` };
    setSecrets((current) => editingSecret ? current.map((secret) => (secret.id === editingSecret.id ? local : secret)) : [local, ...current]);
  }

  async function saveItem(payload: ItemSavePayload) {
    if (isTauriRuntime()) {
      if (editingSecret) await backend.itemUpdate(editingSecret.id, { name: payload.name, fields: payload.fields, clear: payload.clear, tags: payload.tags, favorite: payload.favorite, devUse: payload.devUse, envKey: payload.envKey });
      else {
        const id = await backend.itemCreate({ itemType: payload.itemType, name: payload.name, fields: payload.fields, tags: payload.tags, favorite: payload.favorite, devUse: payload.devUse, envKey: payload.envKey });
        setActiveNav('all');
        setSelectedId(id);
      }
      await refreshSecrets();
      return;
    }
    // Browser demo mode keeps only list-level data; there is no backend to hold the fields.
    const subtitle = payload.itemType === 'login' ? hostOf(payload.fields.website ?? '') : payload.itemType === 'bank_card' ? `${payload.fields.bank ?? ''} · ${(payload.fields.card_number ?? '').slice(-4)}` : '';
    const local: SecretMetadata = { id: editingSecret?.id ?? `local-${Date.now()}`, name: payload.name, envKey: payload.envKey ?? '', provider: 'personal', tags: payload.tags, status: 'unknown', updated: '', updatedAt: Math.floor(Date.now() / 1000), valueType: payload.itemType, category: 'personal', scope: 'global', favorite: payload.favorite, devUse: payload.devUse ?? false, subtitle };
    setSecrets((current) => editingSecret ? current.map((secret) => (secret.id === editingSecret.id ? local : secret)) : [local, ...current]);
  }

  async function saveSecret(fullRequest: SecretRequest) {
    const { capability_models: capabilityModels, ...request } = fullRequest;
    if (editingSecret) {
      if (isTauriRuntime()) { await backend.update(editingSecret.id, request); if (capabilityModels) await backend.setCapabilityModels(editingSecret.id, capabilityModels); }
      setSecrets((current) => current.map((secret) => secret.id === editingSecret.id ? { ...secret, name: request.name, envKey: request.env_key, provider: request.provider_id, description: request.description, tags: request.tags, updated: '', updatedAt: Math.floor(Date.now() / 1000), valueType: request.value_type, category: request.category, scope: request.scope, favorite: request.favorite, archived: request.archived, modelId: request.model_id, modelEnvKey: request.model_env_key, endpointUrl: request.endpoint_url, endpointEnvKey: request.endpoint_env_key } : secret));
      setEditingSecret(null);
      if (isTauriRuntime()) await refreshSecrets();
      return;
    }
    if (isTauriRuntime()) {
      const createdId = await backend.create(request);
      await refreshSecrets();
      if (request.value_type === 'api_key') void openAfterSync(createdId);
      return;
    }
    const localSecret: SecretMetadata = { id: `local-${Date.now()}`, name: request.name, envKey: request.env_key, provider: request.provider_id, description: request.description, tags: request.tags, status: 'unknown', updated: '', updatedAt: Math.floor(Date.now() / 1000), valueType: request.value_type, category: request.category, scope: request.scope, favorite: request.favorite, archived: request.archived, modelId: request.model_id, modelEnvKey: request.model_env_key, endpointUrl: request.endpoint_url, endpointEnvKey: request.endpoint_env_key };
    setSecrets((current) => [localSecret, ...current]);
  }

  async function removeSelected() {
    if (!selected || !window.confirm(t.detail.deleteConfirm)) return;
    if (isTauriRuntime()) await backend.remove(selected.id);
    setSecrets((current) => current.filter((secret) => secret.id !== selected.id));
    setSelectedId('');
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true"><span /></div>
          <div><strong>SecretHub</strong><small>Developer vault</small></div>
        </div>
        <nav className="nav-group" aria-label="Primary navigation">
          <NavItem active={activeNav === 'all'} label={t.nav.all} onClick={() => setActiveNav('all')} count={String(secrets.length)} />
          <NavItem active={activeNav === 'api_key'} label={t.nav.apiKey} onClick={() => setActiveNav('api_key')} />
          <NavItem active={activeNav === 'dev_secret'} label={t.nav.devSecret} onClick={() => setActiveNav('dev_secret')} />
          <NavItem active={activeNav === 'env_group'} label={t.nav.group} onClick={() => setActiveNav('env_group')} />
          <NavItem active={activeNav === 'login'} label={t.nav.login} onClick={() => setActiveNav('login')} />
          <NavItem active={activeNav === 'bank_card'} label={t.nav.bankCard} onClick={() => setActiveNav('bank_card')} />
          <NavItem active={activeNav === 'note'} label={t.nav.note} onClick={() => setActiveNav('note')} />
          <p className="nav-label">{t.nav.profiles} / {t.nav.projects}</p>
          <NavItem active={activeNav === 'profiles'} label={t.nav.profiles} onClick={() => setActiveNav('profiles')} />
          <NavItem active={activeNav === 'projects'} label={t.nav.projects} onClick={() => setActiveNav('projects')} />
          <NavItem active={activeNav === 'providers'} label={t.nav.providers} dot="green" onClick={() => setActiveNav('providers')} />
          <NavItem active={activeNav === 'media'} label={t.media.nav} onClick={() => setActiveNav('media')} />
        </nav>
        <div className="sidebar-bottom">
          <button className="nav-item" onClick={() => setActiveNav('settings')}><span className="nav-glyph">/</span>{t.nav.settings}</button>
          <button className="nav-item" onClick={() => setActiveNav('audit')}><span className="nav-glyph">·</span>{t.auditPanel.title}</button>
          <div className="vault-status"><span className="status-dot" /> <span>{t.footer.vault}</span><strong>{t.footer.synced}</strong></div>
        </div>
      </aside>

      <main className="main-panel">
        <header className="topbar">
          <div><p className="eyebrow">{t.header.eyebrow}</p><h1>{t.header.title}</h1><p className="subtitle">{t.header.subtitle}</p></div>
          <div className="header-actions">
            <button className="language-switch" onClick={() => changeLocale(locale === 'zh-CN' ? 'en-US' : 'zh-CN')} aria-label={t.header.language}>{t.header.language}</button>
            <button className="secondary-button" onClick={() => setShowImport(true)}>{t.actions.import}</button>
            {activeNav === 'api_key' && <><button className="secondary-button" disabled={validationRunning} onClick={() => void validateAllApiKeys()}>{validationRunning ? t.actions.validatingAll : t.actions.validateAll}</button>{actionMessage && <span className="header-feedback">{actionMessage}</span>}</>}
            <button className="primary-button" onClick={openAdd}><span aria-hidden="true">+</span>{t.header.add}</button>
          </div>
        </header>
        <section className="security-strip" aria-label="Security status">
          <span className="shield-icon" aria-hidden="true">◆</span><span>{t.security.localOnly}</span><span className="strip-divider" /><span>{t.security.encrypted}</span><span className="strip-divider" /><span>{t.security.audit}</span>
        </section>
        {pendingPlans.map((plan) => <McpPlanBanner key={plan.request_id} plan={plan} t={t} onReview={() => setReviewPlan(plan)} />)}
        {vaultStatus && !vaultStatus.unlocked && pendingSync.map((request) => <SyncRequestBanner key={request.requestId} request={request} t={t} />)}
        {vaultStatus && !vaultStatus.unlocked && pendingOrganize.map((request) => <OrganizeWaitingBanner key={request.requestId} request={request} t={t} />)}
        {vaultStatus?.unlocked && aiNotices.map((notice) => <AiImportNoticeBanner key={notice.batchId} notice={notice} t={t} onView={() => { setActiveNav('env_group'); void refreshSecrets(); }} onUndo={() => void undoAiImport(notice)} onDismiss={() => void dismissAiNotice(notice)} />)}
        {vaultStatus?.unlocked && captureRequests.map((request) => <CaptureRequestBanner key={request.requestId} request={request} t={t} onStart={() => void decideCapture(request, true)} onDecline={() => void decideCapture(request, false)} />)}
        {vaultStatus?.unlocked && importRequests.map((request) => <ImportRequestBanner key={request.requestId} request={request} t={t} onView={() => void viewImport(request)} onDecline={() => void declineImport(request)} />)}
        {aiReview && <ImportReviewModal t={t} review={aiReview} onClose={() => setAiReview(null)} onDone={(outcome) => void aiImportDone(outcome)} />}
        {mediaRequests.map((request) => <MediaRequestBanner key={request.requestId} request={request} t={t} onAccept={() => void decideMedia(request, true)} onDecline={() => void decideMedia(request, false)} />)}
        {capBatches.map((batch) => <CapabilitySuggestionBanner key={batch.requestId} batch={batch} t={t} onReview={() => setReviewCaps(batch)} />)}
        {reviewPlan && <PlanReviewModal plan={reviewPlan} t={t} onClose={() => setReviewPlan(null)} onDone={planReviewed} />}
        {reviewCaps && <CapabilitySuggestionModal batch={reviewCaps} t={t} onClose={() => setReviewCaps(null)} onDone={capsReviewed} />}
        {runtimeError && <div className="runtime-error" role="alert">{runtimeError}</div>}
        {vaultStatus && !vaultStatus.unlocked && activeNav !== 'media' ? <VaultGate status={vaultStatus} t={t} notice={autoLocked ? t.auth.autoLocked : undefined} onReady={() => { setAutoLocked(false); setVaultStatus({ initialized: true, unlocked: true }); void refreshSecrets(); void validateAllApiKeys(true); }} onRestored={vaultRestored} /> : activeNav === 'media' ? <MediaLibrary t={t} revision={mediaRevision} /> : activeNav === 'settings' ? <SettingsPanel t={t} locale={locale} onCatalogUpdated={reloadCatalog} onRestored={vaultRestored} /> : activeNav === 'providers' ? <ProviderPanel t={t} secrets={secrets} onCapture={(provider) => void startCapture(provider.id, provider.name)} onConfigure={(preset) => { setCredentialPreset(preset); setEditingSecret(null); setShowForm(true); }} /> : activeNav === 'profiles' ? <ProfilePanel t={t} secrets={secrets} revision={profileRevision} selectedIds={selectedIds} onEdit={setEditingProfile} onApply={(ids, variables) => { setSelectedIds(ids.filter((id) => !secrets.some((secret) => secret.id === id && !isAiVisible(secret)))); setExportVariables(variables); setActiveNav('all'); setShowExport(true); }} /> : activeNav === 'projects' ? <ProjectsPanel t={t} /> : activeNav === 'audit' ? <AuditPanel t={t} /> : (
          <>
            <div className="content-toolbar">
              <div className="toolbar-left"><label className="search-box"><span aria-hidden="true">/</span><input ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t.header.search} aria-label={t.header.search} /><kbd>{SEARCH_SHORTCUT_LABEL}</kbd></label><div className="filter-row"><button className="filter-button" onClick={toggleSelectAllVisible} disabled={exportable.length === 0}>{allVisibleSelected ? t.actions.clearAll : t.actions.selectAll}</button><select aria-label={t.nav.providers} value={categoryFilter === 'all' ? 'all' : categoryFilter} onChange={(event) => setCategoryFilter(event.target.value)}><option value="all">{t.list.allCategories}</option><option value="ai">AI</option><option value="database">Database</option><option value="cloud">Cloud</option><option value="git">Git</option><option value="other">Other</option></select><select aria-label={t.list.allStatuses} value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)}><option value="all">{t.list.allStatuses}</option><option value="valid">{t.list.valid}</option><option value="invalid">{t.list.invalid}</option><option value="unauthorized">{t.list.unauthorized}</option><option value="rate_limited">{t.list.rateLimited}</option><option value="network_error">{t.list.networkError}</option><option value="unsupported">{t.list.unsupported}</option><option value="unknown">{t.list.unknown}</option></select><select aria-label={t.list.sort} value={sortMode} onChange={(event) => setSortMode(event.target.value as 'updated' | 'usage' | 'name')}><option value="updated">{t.list.sortUpdated}</option><option value="usage">{t.list.sortUsage}</option><option value="name">{t.list.sortName}</option></select><button className={favoriteOnly ? 'filter-button active' : 'filter-button'} onClick={() => setFavoriteOnly((value) => !value)}>{t.list.favoriteOnly}</button><button className={includeArchived ? 'filter-button active' : 'filter-button'} onClick={() => setIncludeArchived((value) => !value)}>{t.list.includeArchived}</button></div></div>
              {selectedIds.length > 0 && <div className="selection-actions"><span>{selectedIds.length} {t.list.selected}</span><button onClick={() => { setExportVariables({}); setShowExport(true); }}>{t.actions.export}</button><button onClick={() => setShowProfilePrompt(true)}>{t.actions.profile}</button><button onClick={() => setSelectedIds([])}>×</button></div>}
            </div>
            <div className="workspace-grid">
              <section className="list-panel" aria-label={t.list.title}>
                <div className="panel-heading"><div><span className="section-kicker">CATALOG / 04</span><h2>{t.list.title}</h2></div><button className="icon-button" onClick={openAdd} aria-label={t.header.add}>+</button></div>
                <div className="secret-list">
                  {filtered.length === 0 ? <div className="empty-state"><div className="empty-orbit" /><strong>{query ? t.list.searchEmpty : t.list.empty}</strong><span>{t.security.warning}</span></div> : filtered.map((secret) => <SecretRow key={secret.id} secret={secret} selected={selected?.id === secret.id} checked={selectedIds.includes(secret.id)} t={t} onClick={() => setSelectedId(secret.id)} onToggle={() => toggleSelected(secret.id)} />)}
                </div>
                <div className="list-footer"><span>{secrets.length} {t.list.title.toLowerCase()}</span><span className="live-indicator"><i />{t.list.active}</span></div>
              </section>
              <section className="detail-panel" aria-label={t.detail.title}>
                {selected?.valueType === 'env_group' ? <GroupDetail key={selected.id} t={t} secret={selected} onEdit={() => void startEdit(selected)} onDelete={() => void removeSelected()} /> : selected && isJsonItem(selected.valueType) ? <ItemDetail key={selected.id} t={t} secret={selected} onEdit={() => void startEdit(selected)} onDelete={() => void removeSelected()} /> : selected ? <>
                  <div className="detail-heading"><div><span className="section-kicker">SECRET / {(itemGroup(selected) === 'api_key' ? selected.provider : t.items.types[itemGroup(selected)].name).toUpperCase()}</span><h2>{selected.name}</h2></div><button className="more-button" aria-label="More actions">•••</button></div>
                  <div className="secret-visual"><div className="secret-orb"><span /></div><div><span className="field-label">{t.detail.envKey}</span><strong>{selected.envKey || '—'}</strong><span className="masked-value" aria-label={t.detail.masked}>••••••••••••••••</span></div><span className="valid-badge">{statusText(selected.status, t)}</span></div>
                  <div className="detail-fields">{selected.valueType === 'api_key' && <><Field label={t.detail.provider} value={selected.provider} /><Field label={t.detail.status} value={statusText(selected.status, t)} /></>}{selected.valueType === 'api_key' && <Field label={t.tools.lastValidated} value={selected.validatedAt ? formatRelativeTime(selected.validatedAt, t.time) : t.tools.neverValidated} />}{selected.modelId && <Field label={t.form.model} value={selected.modelId} />}{selected.endpointUrl && <Field label={t.form.endpointUrl} value={selected.endpointUrl} />}<div><span className="field-label">{t.detail.tags}</span><div className="tag-row">{selected.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div></div><Field label={t.detail.notes} value={selected.description || (locale === 'zh-CN' ? '仅用于本地开发环境。' : 'For local development only.')} /></div>
                  <div className="detail-actions"><button className="secondary-button" onClick={() => runBackendAction(() => backend.copy(selected.id), t.detail.copyApiKeyDone)}>{selected.valueType === 'api_key' ? t.detail.copyApiKey : t.detail.copy}</button><button className="secondary-button" onClick={() => runBackendAction(() => backend.copyAll(selected.id), t.detail.copyAllDone)}>{t.detail.copyAll}</button><button className="secondary-button" onClick={() => runBackendAction(() => backend.reveal(selected.id))}>{t.detail.reveal}</button>{selected.valueType === 'api_key' && <button className="secondary-button" onClick={() => void validateSecret(selected.id)}>{t.detail.validate}</button>}</div>{actionMessage && <div className="form-success action-message">{actionMessage}</div>}{validationMessage && <div className="form-success validation-message">{validationMessage}</div>}{selected.valueType === 'api_key' && <ModelSyncSection t={t} locale={locale} status={selected.modelsSyncStatus ?? ''} syncedAt={selected.modelsSyncedAt ?? 0} models={models} syncing={modelSyncing} message={modelMessage} onSync={() => void syncModels(selected.id)} currentModelId={selected.modelId} onSetDefault={(modelId) => void setDefaultModel(selected, modelId)} capabilityRows={capRows} onSetCapabilityDefault={(capability, modelId) => void setCapabilityDefault(selected, capability, modelId)} />}{selected.valueType === 'api_key' && needsCapabilitySection(providerKind(selected.provider), models) && <CapabilityModelsPanel key={selected.id} t={t} models={models} rows={capRows} primaryModelId={selected.modelId} reservedEnvKeys={[selected.envKey, selected.modelEnvKey ?? '', selected.endpointEnvKey ?? '']} onSave={async (rows) => { await backend.setCapabilityModels(selected.id, rows); setCapRows(rows); }} />}<button className="edit-button" onClick={() => void startEdit(selected)}>{t.detail.edit}</button><button className="delete-button" onClick={() => void removeSelected()}>{t.detail.delete}</button>
                  <div className="detail-note"><span className="lock-small">◆</span><span>{t.security.warning}</span></div>
                </> : <div className="empty-detail">{t.detail.noSelection}</div>}
              </section>
            </div>
          </>
        )}
        <footer className="app-footer"><span>SecretHub / {t.footer.version}</span><span>{t.footer.vault} · {t.footer.synced}</span></footer>
      </main>
      {captureName && <CaptureModal t={t} providerName={captureName} onCaptured={(secretId) => captureDone.current(secretId)} onClose={() => { setCaptureName(null); void refreshSecrets(); setModelsRevision((value) => value + 1); }} />}
      {addStep === 'picker' && <ItemTypePicker t={t} onPick={pickKind} onClose={() => setAddStep(null)} />}
      {addStep === 'env_group' && <GroupForm key={editingSecret?.id ?? 'new'} t={t} knownTags={secrets.flatMap((secret) => secret.tags)} initial={editingSecret} initialPublic={editGroup} onBack={() => setAddStep('picker')} onClose={closeItemForm} onSave={saveGroup} />}
      {addStep === 'login' && <LoginForm key={editingSecret?.id ?? 'new'} t={t} knownTags={secrets.flatMap((secret) => secret.tags)} initial={editingSecret} initialFields={editFields} onBack={() => setAddStep('picker')} onClose={closeItemForm} onSave={saveItem} />}
      {addStep === 'bank_card' && <BankCardForm key={editingSecret?.id ?? 'new'} t={t} knownTags={secrets.flatMap((secret) => secret.tags)} initial={editingSecret} initialFields={editFields} onBack={() => setAddStep('picker')} onClose={closeItemForm} onSave={saveItem} />}
      {addStep === 'note' && <NoteForm key={editingSecret?.id ?? 'new'} t={t} knownTags={secrets.flatMap((secret) => secret.tags)} initial={editingSecret} initialFields={editFields} onBack={() => setAddStep('picker')} onClose={closeItemForm} onSave={saveItem} />}
      {addStep === 'dev_secret' && <DevSecretForm key={editingSecret?.id ?? 'new'} t={t} knownTags={secrets.flatMap((secret) => secret.tags)} initial={editingSecret} onBack={() => setAddStep('picker')} onClose={closeItemForm} onSave={saveSecret} />}
      {showForm && <SecretForm t={t} knownTags={secrets.flatMap((secret) => secret.tags)} existingSecrets={secrets.map((secret) => ({ name: secret.name, provider: secret.provider, archived: secret.archived }))} initial={editingSecret} preset={credentialPreset} syncedModels={editingSecret && editingSecret.id === selectedId ? models : []} capabilityRows={editingSecret && editingSecret.id === selectedId ? capRows : []} onCapture={(providerId, providerName) => void startCapture(providerId, providerName)} onChangeType={() => { setShowForm(false); setEditingSecret(null); setCredentialPreset(null); setAddStep('picker'); }} onClose={() => { setShowForm(false); setEditingSecret(null); setCredentialPreset(null); }} onSave={saveSecret} />}
      {showExport && <ExportDialog t={t} secrets={secrets} selectedIds={selectedIds} initialVariables={exportVariables} onClose={() => setShowExport(false)} onExported={() => void refreshSecrets()} onProfileSaved={() => setProfileRevision((value) => value + 1)} />}
      {showImport && <ImportPanel t={t} onClose={() => setShowImport(false)} onDone={() => { setShowImport(false); void refreshSecrets(); }} />}
      {showProfilePrompt && <ProfilePrompt t={t} selectedIds={selectedIds} onClose={() => setShowProfilePrompt(false)} onSaved={() => { setShowProfilePrompt(false); setActiveNav('profiles'); }} />}
      {editingProfile && <ProfileDetailModal t={t} profile={editingProfile} secrets={secrets} onClose={() => setEditingProfile(null)} onSaved={() => { setEditingProfile(null); setProfileRevision((value) => value + 1); }} />}
    </div>
  );
}

function VaultGate({ status, t, notice, onReady, onRestored }: { status: VaultStatus; t: ReturnType<typeof getTranslation>; notice?: string; onReady: () => void; onRestored: () => void }) {
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState('');
  const [restoring, setRestoring] = useState(false);
  const setup = !status.initialized;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (setup && password !== confirmation) { setError(t.auth.mismatch); return; }
    if (password.length < 9) { setError(t.auth.minLength); return; }
    try { if (setup) await backend.setupMaster(password); else await backend.unlock(password); onReady(); } catch (reason) { setError(String(reason)); }
  }
  return <><section className="vault-gate"><div className="gate-orb"><span /></div><span className="section-kicker">LOCAL VAULT / {setup ? 'FIRST RUN' : 'LOCKED'}</span><h2>{setup ? t.auth.setupTitle : t.auth.unlockTitle}</h2><p>{t.auth.subtitle}</p>{notice && <div className="form-error" role="status">{notice}</div>}<form onSubmit={submit}><label>{t.auth.password}<input autoFocus type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete={setup ? 'new-password' : 'current-password'} /></label>{setup && <label>{t.auth.confirmPassword}<input type="password" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} autoComplete="new-password" /></label>}{error && <div className="form-error" role="alert">{error}</div>}<button className="primary-button" type="submit">{setup ? t.auth.setup : t.auth.unlock}</button></form><button type="button" className="inline-link gate-restore-link" onClick={() => setRestoring(true)}>{t.tools.restoreOpen}</button></section>{restoring && <RestoreDialog t={t} onClose={() => setRestoring(false)} onRestored={() => { setRestoring(false); onRestored(); }} />}</>;
}

function NavItem({ label, active, count, dot, onClick }: { label: string; active?: boolean; count?: string; dot?: string; onClick: () => void }) {
  return <button className={`nav-item ${active ? 'active' : ''}`} onClick={onClick}><span className={dot ? `provider-dot ${dot}` : 'nav-glyph'}>{dot ? '' : '·'}</span>{label}{count && <span className="nav-count">{count}</span>}</button>;
}

function statusText(status: string, t: ReturnType<typeof getTranslation>): string {
  return ({ valid: t.list.valid, invalid: t.list.invalid, unauthorized: t.list.unauthorized, rate_limited: t.list.rateLimited, network_error: t.list.networkError, unsupported: t.list.unsupported, unknown: t.list.unknown } as Record<string, string>)[status] ?? status;
}

const MODEL_DISPLAY_CAP = 200;

function modelStatusText(status: string, t: ReturnType<typeof getTranslation>): string {
  return ({ ok: t.models.ok, failed: t.models.failed, unsupported: t.models.unsupported } as Record<string, string>)[status] ?? t.models.never;
}

export function ModelSyncSection({ t, locale, status, syncedAt, models, syncing, message, onSync, currentModelId, onSetDefault, capabilityRows = [], onSetCapabilityDefault }: { t: ReturnType<typeof getTranslation>; locale: Locale; status: string; syncedAt: number; models: BackendModel[]; syncing: boolean; message: string; onSync: () => void; currentModelId?: string; onSetDefault?: (modelId: string) => void; capabilityRows?: CapabilityModelRow[]; onSetCapabilityDefault?: (capability: string, modelId: string) => void }) {
  const shown = models.slice(0, MODEL_DISPLAY_CAP);
  const synced = syncedAt > 0 ? new Date(syncedAt * 1000).toLocaleString(locale) : '—';
  return <section className="model-sync" aria-label={t.models.title}>
    <div className="model-sync-head"><strong>{t.models.title}</strong><button className="secondary-button" disabled={syncing} onClick={onSync}>{syncing ? t.models.syncing : t.models.sync}</button></div>
    <div className="model-sync-meta"><span>{modelStatusText(status, t)}</span><span>{t.models.syncedAt}: {synced}</span><span>{t.models.count}: {models.length}</span></div>
    {message && <div className="form-success model-message">{message}</div>}
    <ul className="model-list">{shown.length === 0 ? <li className="model-empty">{t.models.empty}</li> : shown.map((model) => <li key={model.model_id}><code>{model.model_id}</code>{model.capabilities.map((cap) => <span className="tag" key={cap}>{cap}</span>)}<span className="model-source">{t.models.sourceLabel}: {model.capability_source}</span>{onSetDefault && (model.model_id === currentModelId ? <span className="model-default-mark">{t.models.currentDefault}</span> : <button type="button" className="secondary-button model-default-button" aria-label={`${t.models.setDefault} ${model.model_id}`} onClick={() => onSetDefault(model.model_id)}>{t.models.setDefault}</button>)}{onSetCapabilityDefault && <CapabilityDefaultMenu t={t} model={model} rows={capabilityRows} onPick={onSetCapabilityDefault} />}</li>)}</ul>
    {models.length > MODEL_DISPLAY_CAP && <div className="model-more">+{models.length - MODEL_DISPLAY_CAP} {t.models.more}</div>}
  </section>;
}

/** Per-row "set as <capability> default" menu; offers only the model's non-chat capabilities. */
function CapabilityDefaultMenu({ t, model, rows, onPick }: { t: ReturnType<typeof getTranslation>; model: BackendModel; rows: CapabilityModelRow[]; onPick: (capability: string, modelId: string) => void }) {
  const offered = CAPABILITY_ORDER.filter((capability) => model.capabilities.includes(capability));
  if (offered.length === 0) return null;
  return <select className="model-capability-menu" aria-label={`${t.cap.menuAria} ${model.model_id}`} value="" onChange={(event) => { if (event.target.value) onPick(event.target.value, model.model_id); }}><option value="">{t.cap.menuPlaceholder}</option>{offered.map((capability) => <option key={capability} value={capability}>{t.cap.setFor.replace('{cap}', capabilityLabel(t, capability))}{rows.some((row) => row.capability === capability && row.model_id === model.model_id) ? ' ✓' : ''}</option>)}</select>;
}

/** Localized "N variables" line for a config group, from the count in the plaintext subtitle. */
function groupCount(secret: SecretMetadata, t: ReturnType<typeof getTranslation>) {
  const count = variableCountOf(secret.subtitle);
  return count === undefined ? secret.subtitle ?? '' : t.groups.count.replace('{n}', String(count));
}

function SecretRow({ secret, selected, checked, t, onClick, onToggle }: { secret: SecretMetadata; selected: boolean; checked: boolean; t: ReturnType<typeof getTranslation>; onClick: () => void; onToggle: () => void }) {
  const group = itemGroup(secret);
  const age = secret.updatedAt ? formatRelativeTime(secret.updatedAt, t.time) : secret.updated;
  return <article className={`secret-row ${selected ? 'selected' : ''}`} onClick={onClick}>{isAiVisible(secret) ? <input type="checkbox" checked={checked} onChange={onToggle} onClick={(event) => event.stopPropagation()} aria-label={`${t.actions.export} ${secret.name}`} /> : <span className="row-check-spacer" aria-hidden="true" />}<div className="row-avatar"><TypeIcon kind={group} /></div><div className="row-copy"><strong>{secret.name}</strong><span>{group === 'api_key' ? secret.envKey : group === 'env_group' ? groupCount(secret, t) : secret.subtitle ?? ''}</span><small>{group === 'api_key' ? `${secret.provider} · ${age}` : age}</small></div><span className={`row-status ${secret.status}`} />{secret.favorite && <span className="favorite">★</span>}</article>;
}

function Field({ label, value }: { label: string; value: string }) { return <div><span className="field-label">{label}</span><strong className="field-value">{value}</strong></div>; }

function ProviderPanel({ t, secrets, onConfigure, onCapture }: { t: ReturnType<typeof getTranslation>; secrets: SecretMetadata[]; onConfigure: (preset: CredentialPreset) => void; onCapture: (provider: ProviderTemplate) => void }) {
  const [query, setQuery] = useState('');
  const [moreOpen, setMoreOpen] = useState(false);
  const usedProviderIds = secrets.filter((secret) => !secret.archived).map((secret) => resolveProviderId(secret.provider)).filter((id): id is string => Boolean(id));
  const allGroups = groupProviders(providerTemplates, usedProviderIds);
  const groups = groupProviders(providerTemplates, usedProviderIds, query);
  const showMore = moreOpen || groups.searching;
  async function openWebsite(url: string) { if (isTauriRuntime()) await backend.openExternalUrl(url); else window.open(url, '_blank', 'noopener,noreferrer'); }
  function configure(provider: ProviderTemplate) {
    const model = provider.models.find((item) => item.id === provider.defaultModel) ?? provider.models[0];
    onConfigure({ provider: provider.id, name: provider.name, envKey: defaultApiKeyEnvKey(provider.id), modelId: model.id, modelEnvKey: model.envKey, endpointUrl: provider.baseUrl, endpointEnvKey: defaultEndpointEnvKey(provider.id) });
  }
  const card = (provider: ProviderTemplate) => <article className="provider-directory-card" key={provider.id}><div className="provider-directory-heading"><div><strong>{provider.name}</strong><span>{provider.models.length} {t.providerPanel.templates}</span></div><span className="provider-dot green" /></div><div className="provider-models">{provider.models.slice(0, 4).map((model) => <span key={model.id}>{model.label}</span>)}{provider.models.length > 4 && <span>+ {provider.models.length - 4}</span>}</div><div className="provider-directory-url">{provider.baseUrl || t.providerPanel.endpoint}</div><div className="provider-card-actions"><button className="secondary-button" onClick={() => configure(provider)}>{t.providerPanel.configure}</button>{provider.keyUrl && <button className="inline-link" onClick={() => void openWebsite(provider.keyUrl)}>{t.providerPanel.officialKey}</button>}{provider.keyUrl && <button className="inline-link" onClick={() => onCapture(provider)}>{t.capture.button}</button>}</div></article>;
  return <section className="settings-panel provider-panel">
    <div className="settings-heading"><span className="section-kicker">{t.providerPanel.kicker}</span><h2>{t.nav.providers}</h2><p>{t.providerPanel.subtitle}</p></div>
    <input className="provider-search-input provider-panel-search" aria-label={t.form.providerSearch} placeholder={t.form.providerSearch} value={query} onChange={(event) => setQuery(event.target.value)} />
    <div className="provider-directory-grid">{groups.common.map(card)}</div>
    {groups.searching && groups.common.length === 0 && groups.more.length === 0 && <div className="empty-state provider-empty"><strong>{t.form.providerSearchEmpty}</strong></div>}
    {groups.more.length > 0 && <>
      <button type="button" className="advanced-toggle provider-more-toggle" aria-expanded={showMore} onClick={() => setMoreOpen((open) => !open)}><span aria-hidden="true">{showMore ? '▾' : '▸'}</span> {t.quick.moreCount.replace('{n}', String(groups.searching ? groups.more.length : allGroups.more.length))}</button>
      {showMore && <div className="provider-directory-grid">{groups.more.map(card)}</div>}
    </>}
    <div className="form-security"><span>◆</span>{t.providerPanel.security}</div>
  </section>;
}

const AUTO_LOCK_CHOICES = [1, 5, 15, 30, 60];

function SettingsPanel({ t, locale, onCatalogUpdated, onRestored }: { t: ReturnType<typeof getTranslation>; locale: Locale; onCatalogUpdated: () => void; onRestored: () => void }) {
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [settings, setSettings] = useState<BackendSettings | null>(null);
  const [autoLock, setAutoLock] = useState(15);
  const [showRestore, setShowRestore] = useState(false);
  const [showChange, setShowChange] = useState(false);
  useEffect(() => {
    if (!isTauriRuntime()) return;
    void backend.settingsGet().then((value) => { setSettings(value); setAutoLock(value.auto_lock_minutes); }).catch((reason) => setError(String(reason)));
  }, []);
  // The backend rewrites the running auto-lock policy as part of settings_update, so the change applies at once.
  async function changeAutoLock(minutes: number) {
    const previous = autoLock;
    setAutoLock(minutes);
    if (!isTauriRuntime() || !settings) return;
    try {
      const next = { ...settings, auto_lock_minutes: minutes };
      await backend.settingsUpdate(next);
      setSettings(next);
      setError('');
    } catch (reason) { setAutoLock(previous); setError(String(reason)); }
  }
  async function exportBackup() {
    try {
      const folder = isTauriRuntime() ? await backend.chooseFolder() : '.';
      if (!folder) return;
      const written = isTauriRuntime() ? await backend.backup(folder) : '';
      setMessage(written ? `${t.tools.backupSavedTo}${written}` : t.settings.backupSaved);
      setError('');
    } catch (reason) { setError(String(reason)); }
  }
  const choices = AUTO_LOCK_CHOICES.includes(autoLock) ? AUTO_LOCK_CHOICES : [...AUTO_LOCK_CHOICES, autoLock].sort((left, right) => left - right);
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">CONTROL CENTER</span><h2>{t.settings.title}</h2><p>{t.security.warning}</p></div><div className="settings-grid"><div className="setting-group"><h3>{t.settings.security}</h3><div className="setting-row"><label htmlFor="auto-lock-minutes">{t.settings.autoLock}</label><select id="auto-lock-minutes" value={autoLock} onChange={(event) => void changeAutoLock(Number(event.target.value))}>{choices.map((minutes) => <option key={minutes} value={minutes}>{minutes} {t.tools.autoLockUnit}</option>)}</select></div><p className="cap-hint">{t.tools.autoLockHint}</p><div className="setting-row"><span>{t.items.changePassword}</span><button type="button" className="secondary-button" onClick={() => setShowChange(true)}>{t.items.changePassword}</button></div></div><SettingGroup title={t.settings.ai} items={[[t.settings.language, 'zh-CN / en-US']]}/></div><CatalogSettings t={t} locale={locale} onUpdated={onCatalogUpdated} /><div className="settings-footer"><button className="secondary-button" onClick={() => void exportBackup()}>{t.settings.backup}</button><button className="secondary-button" onClick={() => setShowRestore(true)}>{t.tools.restoreOpen}</button>{message && <span className="form-success">{message}</span>}{error && <span className="form-error" role="alert">{error}</span>}</div>{showRestore && <RestoreDialog t={t} onClose={() => setShowRestore(false)} onRestored={() => { setShowRestore(false); setMessage(t.tools.restoreDone); onRestored(); }} />}{showChange && <ChangePasswordDialog t={t} onClose={() => setShowChange(false)} />}</section>;
}

function ProfilePrompt({ t, selectedIds, onClose, onSaved }: { t: ReturnType<typeof getTranslation>; selectedIds: string[]; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [error, setError] = useState('');
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!name.trim()) { setError(t.form.required); return; }
    try { if (isTauriRuntime()) await backend.saveProfile({ id: `profile-${Date.now()}`, name: name.trim(), description, secret_ids: selectedIds }); onSaved(); } catch (reason) { setError(String(reason)); }
  }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.profile.create}><form className="secret-form profile-prompt" onSubmit={(event) => void submit(event)}><div className="form-heading"><div><span className="section-kicker">PROFILE / {selectedIds.length} SELECTED</span><h2>{t.profile.create}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><label>{t.profile.name}<input autoFocus required value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t.profile.description}<textarea value={description} onChange={(event) => setDescription(event.target.value)} rows={3} /></label>{error && <div className="form-error" role="alert">{error}</div>}<div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.profile.cancel}</button><button className="primary-button" type="submit">{t.profile.save}</button></div></form></div>;
}

export function ProfileDetailModal({ t, profile, secrets, onClose, onSaved }: { t: ReturnType<typeof getTranslation>; profile: Profile; secrets: SecretMetadata[]; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState(profile.name);
  const [description, setDescription] = useState(profile.description);
  const [draftIds, setDraftIds] = useState(profile.secret_ids);
  const [showPicker, setShowPicker] = useState(false);
  const [error, setError] = useState('');
  async function save() {
    if (!name.trim()) { setError(t.form.required); return; }
    try { if (isTauriRuntime()) await backend.saveProfile({ id: profile.id, name: name.trim(), description, secret_ids: draftIds, variables: profile.variables }); onSaved(); } catch (reason) { setError(String(reason)); }
  }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.profile.detailTitle}><div className="secret-form profile-detail-modal"><div className="form-heading"><div><span className="section-kicker">PROFILE / {draftIds.length} {t.nav.secrets.toUpperCase()}</span><h2>{t.profile.detailTitle}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><label>{t.profile.name}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t.profile.description}<textarea value={description} onChange={(event) => setDescription(event.target.value)} rows={2} /></label><div className="profile-detail-heading"><strong>{t.profile.selectCredentials}</strong><button type="button" className="secondary-button" onClick={() => setShowPicker(true)}>{t.profile.addCredential}</button></div><div className="profile-secret-list">{draftIds.length === 0 ? <div className="empty-state"><strong>{t.profile.empty}</strong></div> : draftIds.map((id) => { const secret = secrets.find((item) => item.id === id); return <div className="profile-secret-row" key={id}><div><strong>{secret?.name ?? id}</strong><span>{secret?.envKey || secret?.provider || 'Missing from current vault'}</span>{profile.variables?.[id] && <small className="profile-subset">{t.exportDialog.subsetOf}: {profile.variables[id].join(', ')}</small>}</div><button type="button" className="delete-button" onClick={() => setDraftIds((current) => current.filter((item) => item !== id))}>{t.profile.removeCredential}</button></div>; })}</div>{error && <div className="form-error" role="alert">{error}</div>}<div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.profile.cancel}</button><button type="button" className="primary-button" onClick={() => void save()}>{t.profile.save}</button></div>{showPicker && <ProfileSecretPicker t={t} secrets={secrets} selectedIds={draftIds} onClose={() => setShowPicker(false)} onConfirm={(ids) => { setDraftIds(ids); setShowPicker(false); }} />}</div></div>;
}

export function ProfileSecretPicker({ t, secrets, selectedIds, onClose, onConfirm }: { t: ReturnType<typeof getTranslation>; secrets: SecretMetadata[]; selectedIds: string[]; onClose: () => void; onConfirm: (ids: string[]) => void }) {
  const [draftIds, setDraftIds] = useState(selectedIds);
  function toggle(id: string) { setDraftIds((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]); }
  return <div className="modal-backdrop nested-modal" role="dialog" aria-modal="true" aria-label={t.profile.selectCredentials}><div className="secret-form profile-picker"><div className="form-heading"><div><span className="section-kicker">PROFILE / PICK CREDENTIALS</span><h2>{t.profile.selectCredentials}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><div className="picker-list">{secrets.filter(isAiVisible).length === 0 ? <div className="empty-state"><strong>{t.list.empty}</strong></div> : secrets.filter(isAiVisible).map((secret) => <label className="picker-row" key={secret.id}><input type="checkbox" checked={draftIds.includes(secret.id)} onChange={() => toggle(secret.id)} /><span><strong>{secret.name}</strong><small>{secret.provider} · {secret.envKey || 'No environment key'}</small></span></label>)}</div><div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.profile.cancel}</button><button type="button" className="primary-button" onClick={() => onConfirm(draftIds)}>{t.profile.confirmSelection}</button></div></div></div>;
}

function ProfilePanel({ t, selectedIds, secrets, revision, onEdit, onApply }: { t: ReturnType<typeof getTranslation>; selectedIds: string[]; secrets: SecretMetadata[]; revision: number; onEdit: (profile: Profile) => void; onApply: (ids: string[], variables: ProfileVariables) => void }) {
  const [name, setName] = useState(''); const [description, setDescription] = useState(''); const [profiles, setProfiles] = useState<Profile[]>([]); const [editingId, setEditingId] = useState<string | null>(null); const [message, setMessage] = useState(''); const [error, setError] = useState('');
  useEffect(() => { if (isTauriRuntime()) void backend.profiles().then(setProfiles).catch((reason) => setError(String(reason))); }, [revision]);
  async function save() { if (!name.trim()) { setError(t.form.required); return; } const profile = { id: editingId ?? `profile-${Date.now()}`, name: name.trim(), description, secret_ids: selectedIds }; try { if (isTauriRuntime()) await backend.saveProfile(profile); setProfiles((current) => editingId ? current.map((item) => item.id === editingId ? profile : item) : [profile, ...current]); setName(''); setDescription(''); setEditingId(null); setMessage(t.profile.saved); setError(''); } catch (reason) { setError(String(reason)); } }
  async function remove(id: string) { if (!window.confirm(t.profile.deleteConfirm)) return; try { if (isTauriRuntime()) await backend.deleteProfile(id); setProfiles((current) => current.filter((item) => item.id !== id)); } catch (reason) { setError(String(reason)); } }
  function edit(profile: Profile) { onEdit(profile); }
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">PROJECT RECIPE / {selectedIds.length} SELECTED</span><h2>{t.profile.title}</h2><p>{t.security.warning}</p></div><div className="profile-editor"><label>{t.profile.name}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t.profile.description}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button className="primary-button" onClick={() => void save()}>{editingId ? t.profile.edit : t.profile.create}</button>{editingId && <button className="secondary-button" onClick={() => { setEditingId(null); setName(''); setDescription(''); }}>{t.profile.cancel}</button>}{message && <span className="form-success">{message}</span>}{error && <span className="form-error" role="alert">{error}</span>}</div><div className="profile-list">{profiles.length === 0 ? <div className="empty-state"><strong>{t.profile.empty}</strong></div> : profiles.map((profile) => <div className="profile-row" key={profile.id}><div><strong>{profile.name}</strong><span>{profile.secret_ids.length} {t.nav.secrets}{profile.description ? ` · ${profile.description}` : ''}</span></div><div className="profile-actions"><button className="secondary-button" onClick={() => edit(profile)}>{t.profile.edit}</button><button className="delete-button" onClick={() => void remove(profile.id)}>{t.profile.delete}</button><button className="secondary-button" onClick={() => onApply(profile.secret_ids, profile.variables ?? {})}>{t.profile.apply}</button></div></div>)}</div></section>;
}

function ProjectsPanel({ t }: { t: ReturnType<typeof getTranslation> }) {
  const [projects, setProjects] = useState<Array<{ id: string; path: string; display_name: string; last_used_at?: number }>>([]);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [editingProject, setEditingProject] = useState<{ id: string; path: string; display_name: string; entries: ProjectEnvEntry[] } | null>(null);
  useEffect(() => { if (isTauriRuntime()) void backend.projects().then(setProjects).catch((reason) => setError(String(reason))); }, []);
  async function record() { const path = isTauriRuntime() ? await backend.chooseFolder() : '.'; if (!path) return; const display = path.split(/[\\/]/).filter(Boolean).pop() || 'project'; try { if (isTauriRuntime()) { const project = await backend.recordProject(path, display); setProjects((current) => [project, ...current.filter((item) => item.path !== path)]); } else setProjects((current) => [{ id: `project-${Date.now()}`, path, display_name: display }, ...current]); setMessage(t.project.record); setError(''); } catch (reason) { setError(String(reason)); } }
  async function open(path: string) { try { if (isTauriRuntime()) await backend.openProjectDirectory(path); else window.open(path, '_blank'); setMessage(t.project.opened); } catch (reason) { setError(String(reason)); } }
  async function editEnv(project: { id: string; path: string; display_name: string }) { try { const entries = isTauriRuntime() ? await backend.projectEnvPreview(project.path) : [{ original_key: 'PORT', key: 'PORT', value: '3000', source: 'manual' as const }, { original_key: 'OPENAI_API_KEY', key: 'OPENAI_API_KEY', value: '••••••••', source: 'managed' as const, secret_id: 'demo' }]; setEditingProject({ ...project, entries }); setError(''); } catch (reason) { setError(String(reason)); } }
  return <><section className="settings-panel"><div className="settings-heading"><span className="section-kicker">PROJECT CONTEXT / LOCAL ONLY</span><h2>{t.project.title}</h2><p>{t.project.recent}</p></div><button className="primary-button" onClick={() => void record()}>{t.project.record}</button>{message && <div className="form-success project-message">{message}</div>}{error && <div className="form-error" role="alert">{error}</div>}<div className="project-list">{projects.length === 0 ? <div className="empty-state"><strong>{t.project.empty}</strong></div> : projects.map((project) => <div className="project-row" key={project.id}><div><strong>{project.display_name}</strong><span>{project.path}</span></div><div className="project-actions"><button className="secondary-button" onClick={() => void open(project.path)}>{t.project.open}</button><button className="secondary-button" onClick={() => void editEnv(project)}>{t.project.editEnv}</button></div></div>)}</div></section>{editingProject && <ProjectEnvEditor t={t} project={editingProject} entries={editingProject.entries} onClose={() => setEditingProject(null)} onSaved={() => { setEditingProject(null); setMessage(t.project.envSaved); }} />}</>;
}

export function ProjectEnvEditor({ t, project, entries, onClose, onSaved }: { t: ReturnType<typeof getTranslation>; project: { id: string; path: string; display_name: string }; entries: ProjectEnvEntry[]; onClose: () => void; onSaved: () => void }) {
  const [draft, setDraft] = useState(entries);
  const [error, setError] = useState('');
  function update(index: number, field: 'key' | 'value', value: string) { setDraft((current) => current.map((entry, entryIndex) => entryIndex === index ? { ...entry, [field]: value } : entry)); }
  function add() { setDraft((current) => [...current, { original_key: '', key: '', value: '', source: 'manual' }]); }
  async function save() { const keys = draft.map((entry) => entry.key.trim()); if (keys.some((key) => !key)) { setError(t.form.required); return; } if (new Set(keys).size !== keys.length) { setError(t.form.duplicateEnvKey); return; } try { if (isTauriRuntime()) await backend.projectEnvSave({ directory: project.path, entries: draft.map((entry, index) => ({ ...entry, key: keys[index] })) }); onSaved(); } catch (reason) { setError(String(reason)); } }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.project.envTitle}><div className="secret-form project-env-modal"><div className="form-heading"><div><span className="section-kicker">PROJECT / {project.display_name}</span><h2>{t.project.envTitle}</h2><p className="form-intro">{project.path}</p></div><button type="button" className="close-button" onClick={onClose}>×</button></div><div className="env-warning">◆ {t.project.envWarning}</div><div className="env-grid"><div className="env-grid-head">{t.project.envKey}</div><div className="env-grid-head">{t.project.envValue}</div><div className="env-grid-head">{t.project.envSource}</div><div className="env-grid-head" />{draft.map((entry, index) => <div className="env-grid-row" key={`${entry.original_key}-${index}`}><input value={entry.key} readOnly={entry.source === 'managed'} onChange={(event) => update(index, 'key', event.target.value)} /><input value={entry.value} readOnly={entry.source === 'managed'} onChange={(event) => update(index, 'value', event.target.value)} /><span className={`env-source ${entry.source}`}>{entry.source === 'managed' ? t.project.envManaged : t.project.envManual}</span><button type="button" className="delete-button" onClick={() => setDraft((current) => current.filter((_, entryIndex) => entryIndex !== index))}>{t.project.removeEnv}</button></div>)}</div>{draft.length === 0 && <div className="empty-state env-empty"><strong>{t.project.empty}</strong></div>}{error && <div className="form-error" role="alert">{error}</div>}<div className="form-actions"><button type="button" className="secondary-button" onClick={add}>{t.project.addEnv}</button><button type="button" className="secondary-button" onClick={onClose}>{t.profile.cancel}</button><button type="button" className="primary-button" onClick={() => void save()}>{t.project.saveEnv}</button></div></div></div>;
}

function AuditPanel({ t }: { t: ReturnType<typeof getTranslation> }) {
  const [events, setEvents] = useState<Array<{ id: string; operation: string; result: string }>>([]);
  useEffect(() => { if (isTauriRuntime()) void backend.audit().then(setEvents).catch(() => undefined); }, []);
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">SECURITY / TRACEABLE</span><h2>{t.auditPanel.title}</h2><p>{t.security.audit}</p></div><div className="audit-list">{events.length === 0 ? <div className="empty-state"><strong>{t.auditPanel.empty}</strong></div> : events.map((event) => <div className="audit-row" key={event.id}><strong>{event.operation}</strong><span>{event.result}</span></div>)}</div></section>;
}

export function McpPlanBanner({ plan, t, onReview }: { plan: PendingPlan; t: ReturnType<typeof getTranslation>; onReview: () => void }) {
  const [preview, setPreview] = useState<PlanPreview | null>(null);
  useEffect(() => { if (isTauriRuntime()) void backend.planPreview(plan.request_id).then(setPreview).catch(() => setPreview(null)); }, [plan.request_id]); // plans are immutable per request id; avoid re-reading on every poll
  const conflicts = preview?.entries.filter((entry) => entry.conflict).map((entry) => entry.key) ?? [];
  const warningCount = preview?.entries.reduce((total, entry) => total + entry.warnings.length, 0) ?? 0;
  const errors = preview?.errors ?? [];
  const keyCount = preview ? preview.entries.length : plan.secret_ids.length;
  return <section className="mcp-plan" aria-label={t.mcp.title}><div><span className="section-kicker">MCP / CONFIRMATION REQUIRED</span><strong>{t.mcp.title}</strong><p>{t.mcp.description}</p><code>{plan.project_path}</code><small>{keyCount} {t.nav.secrets}</small>{preview && preview.entries.length > 0 && <small>{preview.entries.map((entry) => entry.key).join(', ')}</small>}{conflicts.length > 0 && <div className="conflict-box">{t.mcp.overwrite}{conflicts.join(', ')}</div>}{warningCount > 0 && <div className="conflict-box">{warningCount} {t.mcp.warnings}</div>}{errors.length > 0 && <div className="form-error" role="alert">{t.mcp.errors}{errors.map((error) => error.code).join(', ')}</div>}</div><div className="mcp-plan-actions"><button className="primary-button" onClick={onReview}>{t.review.open}</button></div></section>;
}

function SettingGroup({ title, items }: { title: string; items: string[][] }) { return <div className="setting-group"><h3>{title}</h3>{items.map(([label, value]) => <div className="setting-row" key={label}><span>{label}</span><strong>{value}</strong></div>)}</div>; }

