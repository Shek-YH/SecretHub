import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { getTranslation, readStoredLocale, type Locale } from './i18n/translations';
import { backend, isTauriRuntime, type PendingPlan, type VaultStatus } from './lib/backend';
import { defaultApiKeyEnvKey, defaultEndpointEnvKey, providerTemplate, providerTemplates, type ProviderTemplate } from './providers';

type SecretMetadata = {
  id: string;
  name: string;
  envKey: string;
  provider: string;
  description?: string;
  tags: string[];
  status: string;
  updated: string;
  favorite?: boolean;
  valueType?: string;
  category?: string;
  scope?: string;
  archived?: boolean;
  modelId?: string;
  modelEnvKey?: string;
  endpointUrl?: string;
  endpointEnvKey?: string;
};

const demoSecrets: SecretMetadata[] = [
  { id: 'openai-main', name: 'OpenAI Main', envKey: 'OPENAI_API_KEY', provider: 'OpenAI', tags: ['ai', 'production'], status: 'valid', updated: '2h ago', favorite: true, valueType: 'api_key', category: 'ai', scope: 'global' },
  { id: 'anthropic-lab', name: 'Anthropic Lab', envKey: 'ANTHROPIC_API_KEY', provider: 'Anthropic', tags: ['ai', 'research'], status: 'valid', updated: '1d ago', valueType: 'api_key', category: 'ai', scope: 'global' },
  { id: 'supabase-cuecut', name: 'Supabase CueCut', envKey: 'SUPABASE_URL', provider: 'Supabase', tags: ['database', 'cuecut'], status: 'unknown', updated: '3d ago', valueType: 'url', category: 'database', scope: 'project' },
  { id: 'github-build', name: 'GitHub Build', envKey: 'GITHUB_TOKEN', provider: 'GitHub', tags: ['ci', 'developer'], status: 'valid', updated: '5d ago', valueType: 'token', category: 'git', scope: 'global' },
];

export function App() {
  const [locale, setLocale] = useState<Locale>(() => readStoredLocale(localStorage.getItem('secrethub-locale')));
  const [secrets, setSecrets] = useState<SecretMetadata[]>(demoSecrets);
  const [vaultStatus, setVaultStatus] = useState<VaultStatus | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [pendingPlans, setPendingPlans] = useState<PendingPlan[]>([]);
  const [query, setQuery] = useState('');
  const [categoryFilter, setCategoryFilter] = useState('all');
  const [statusFilter, setStatusFilter] = useState('all');
  const [favoriteOnly, setFavoriteOnly] = useState(false);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [selectedId, setSelectedId] = useState(demoSecrets[0].id);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [showExport, setShowExport] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showProfilePrompt, setShowProfilePrompt] = useState(false);
  const [editingSecret, setEditingSecret] = useState<SecretMetadata | null>(null);
  const [validationMessage, setValidationMessage] = useState('');
  const [actionMessage, setActionMessage] = useState('');
  const [activeNav, setActiveNav] = useState<'all' | 'profiles' | 'projects' | 'settings' | 'audit'>('all');
  const t = getTranslation(locale);
  const selected = secrets.find((secret) => secret.id === selectedId) ?? null;
  const filtered = useMemo(() => secrets.filter((secret) => `${secret.name} ${secret.envKey} ${secret.provider} ${secret.tags.join(' ')}`.toLowerCase().includes(query.toLowerCase()) && (categoryFilter === 'all' || secret.category === categoryFilter) && (statusFilter === 'all' || secret.status === statusFilter) && (!favoriteOnly || Boolean(secret.favorite)) && (includeArchived || !secret.archived)), [categoryFilter, favoriteOnly, includeArchived, query, secrets, statusFilter]);
  const allVisibleSelected = filtered.length > 0 && filtered.every((secret) => selectedIds.includes(secret.id));

  async function refreshSecrets() {
    if (!isTauriRuntime()) return;
    try { setSecrets((await backend.list(query)).map((secret) => ({ id: secret.id, name: secret.name, envKey: secret.env_key, provider: secret.provider_id, description: secret.description, tags: secret.tags, status: secret.status === 'valid' ? 'valid' : secret.status, updated: 'now', valueType: secret.value_type, category: secret.category, scope: secret.scope, favorite: secret.favorite, archived: secret.archived, modelId: secret.model_id, modelEnvKey: secret.model_env_key, endpointUrl: secret.endpoint_url, endpointEnvKey: secret.endpoint_env_key }))); setRuntimeError(null); } catch (error) { setRuntimeError(String(error)); }
  }

  useEffect(() => {
    if (!isTauriRuntime()) return;
    backend.status().then((status) => { setVaultStatus(status); if (status.unlocked) void refreshSecrets(); else setSecrets([]); }).catch((error) => setRuntimeError(String(error)));
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const loadPlans = () => { void backend.pendingPlans().then(setPendingPlans).catch((error) => setRuntimeError(String(error))); };
    loadPlans();
    const timer = window.setInterval(loadPlans, 2000);
    return () => window.clearInterval(timer);
  }, []);

  async function confirmPlan(plan: PendingPlan) {
    try { await backend.confirmPlan(plan.request_id, true); setPendingPlans((current) => current.filter((item) => item.request_id !== plan.request_id)); await refreshSecrets(); } catch (error) { setRuntimeError(String(error)); }
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
    } catch (error) { setValidationMessage(String(error)); }
  }

  function changeLocale(next: Locale) {
    setLocale(next);
    localStorage.setItem('secrethub-locale', next);
    document.documentElement.lang = next;
  }

  function toggleSelected(id: string) {
    setSelectedIds((current) => current.includes(id) ? current.filter((value) => value !== id) : [...current, id]);
  }

  function toggleSelectAllVisible() {
    const visibleIds = filtered.map((secret) => secret.id);
    setSelectedIds((current) => allVisibleSelected ? current.filter((id) => !visibleIds.includes(id)) : Array.from(new Set([...current, ...visibleIds])));
  }

  async function saveSecret(request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; endpoint_url: string; endpoint_env_key: string }) {
    if (editingSecret) {
      if (isTauriRuntime()) await backend.update(editingSecret.id, request);
      setSecrets((current) => current.map((secret) => secret.id === editingSecret.id ? { ...secret, name: request.name, envKey: request.env_key, provider: request.provider_id, description: request.description, tags: request.tags, updated: 'now', valueType: request.value_type, category: request.category, scope: request.scope, favorite: request.favorite, archived: request.archived, modelId: request.model_id, modelEnvKey: request.model_env_key, endpointUrl: request.endpoint_url, endpointEnvKey: request.endpoint_env_key } : secret));
      setEditingSecret(null);
      if (isTauriRuntime()) await refreshSecrets();
      return;
    }
    if (isTauriRuntime()) { await backend.create(request); await refreshSecrets(); return; }
    const localSecret: SecretMetadata = { id: `local-${Date.now()}`, name: request.name, envKey: request.env_key, provider: request.provider_id, description: request.description, tags: request.tags, status: 'unknown', updated: 'now', valueType: request.value_type, category: request.category, scope: request.scope, favorite: request.favorite, archived: request.archived, modelId: request.model_id, modelEnvKey: request.model_env_key, endpointUrl: request.endpoint_url, endpointEnvKey: request.endpoint_env_key };
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
          <NavItem active={activeNav === 'all'} label={t.nav.all} onClick={() => setActiveNav('all')} count="12" />
          <NavItem label={t.nav.favorites} onClick={() => undefined} />
          <NavItem label={t.nav.recent} onClick={() => undefined} />
          <p className="nav-label">{t.nav.providers}</p>
          <NavItem label="OpenAI" dot="green" onClick={() => undefined} />
          <NavItem label="Anthropic" dot="amber" onClick={() => undefined} />
          <NavItem label="Supabase" dot="violet" onClick={() => undefined} />
          <p className="nav-label">{t.nav.profiles}</p>
          <NavItem label="AI Standard" onClick={() => setActiveNav('profiles')} />
          <NavItem label="Web Stack" onClick={() => setActiveNav('profiles')} />
          <p className="nav-label">{t.nav.projects}</p>
          <NavItem label="CueCut" onClick={() => setActiveNav('projects')} />
          <NavItem label="Creator OS" onClick={() => setActiveNav('projects')} />
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
            <button className="primary-button" onClick={() => setShowForm(true)}><span aria-hidden="true">+</span>{t.header.add}</button>
          </div>
        </header>
        <section className="security-strip" aria-label="Security status">
          <span className="shield-icon" aria-hidden="true">◆</span><span>{t.security.localOnly}</span><span className="strip-divider" /><span>{t.security.encrypted}</span><span className="strip-divider" /><span>{t.security.audit}</span>
        </section>
        {pendingPlans.map((plan) => <McpPlanBanner key={plan.request_id} plan={plan} t={t} onConfirm={() => void confirmPlan(plan)} />)}
        {runtimeError && <div className="runtime-error" role="alert">{runtimeError}</div>}
        {vaultStatus && !vaultStatus.unlocked ? <VaultGate status={vaultStatus} t={t} onReady={() => { setVaultStatus({ initialized: true, unlocked: true }); void refreshSecrets(); }} /> : activeNav === 'settings' ? <SettingsPanel t={t} /> : activeNav === 'profiles' ? <ProfilePanel t={t} selectedIds={selectedIds} onApply={(ids) => { setSelectedIds(ids); setActiveNav('all'); setShowExport(true); }} /> : activeNav === 'projects' ? <ProjectsPanel t={t} /> : activeNav === 'audit' ? <AuditPanel t={t} /> : (
          <>
            <div className="content-toolbar">
              <div className="toolbar-left"><label className="search-box"><span aria-hidden="true">/</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t.header.search} aria-label={t.header.search} /><kbd>⌘ K</kbd></label><div className="filter-row"><button className="filter-button" onClick={toggleSelectAllVisible} disabled={filtered.length === 0}>{allVisibleSelected ? t.actions.clearAll : t.actions.selectAll}</button><select aria-label={t.nav.providers} value={categoryFilter === 'all' ? 'all' : categoryFilter} onChange={(event) => setCategoryFilter(event.target.value)}><option value="all">{t.list.allCategories}</option><option value="ai">AI</option><option value="database">Database</option><option value="cloud">Cloud</option><option value="git">Git</option><option value="other">Other</option></select><select aria-label={t.list.allStatuses} value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)}><option value="all">{t.list.allStatuses}</option><option value="valid">{t.list.valid}</option><option value="invalid">{t.list.invalid}</option><option value="unauthorized">{t.list.unauthorized}</option><option value="rate_limited">{t.list.rateLimited}</option><option value="network_error">{t.list.networkError}</option><option value="unsupported">{t.list.unsupported}</option><option value="unknown">{t.list.unknown}</option></select><button className={favoriteOnly ? 'filter-button active' : 'filter-button'} onClick={() => setFavoriteOnly((value) => !value)}>{t.list.favoriteOnly}</button><button className={includeArchived ? 'filter-button active' : 'filter-button'} onClick={() => setIncludeArchived((value) => !value)}>{t.list.includeArchived}</button></div></div>
              {selectedIds.length > 0 && <div className="selection-actions"><span>{selectedIds.length} {t.list.selected}</span><button onClick={() => setShowExport(true)}>{t.actions.export}</button><button onClick={() => setShowProfilePrompt(true)}>{t.actions.profile}</button><button onClick={() => setSelectedIds([])}>×</button></div>}
            </div>
            <div className="workspace-grid">
              <section className="list-panel" aria-label={t.list.title}>
                <div className="panel-heading"><div><span className="section-kicker">CATALOG / 04</span><h2>{t.list.title}</h2></div><button className="icon-button" onClick={() => setShowForm(true)} aria-label={t.header.add}>+</button></div>
                <div className="secret-list">
                  {filtered.length === 0 ? <div className="empty-state"><div className="empty-orbit" /><strong>{query ? t.list.searchEmpty : t.list.empty}</strong><span>{t.security.warning}</span></div> : filtered.map((secret) => <SecretRow key={secret.id} secret={secret} selected={selected?.id === secret.id} checked={selectedIds.includes(secret.id)} t={t} onClick={() => setSelectedId(secret.id)} onToggle={() => toggleSelected(secret.id)} />)}
                </div>
                <div className="list-footer"><span>{secrets.length} {t.list.title.toLowerCase()}</span><span className="live-indicator"><i />{t.list.active}</span></div>
              </section>
              <section className="detail-panel" aria-label={t.detail.title}>
                {selected ? <>
                  <div className="detail-heading"><div><span className="section-kicker">SECRET / {selected.provider.toUpperCase()}</span><h2>{selected.name}</h2></div><button className="more-button" aria-label="More actions">•••</button></div>
                  <div className="secret-visual"><div className="secret-orb"><span /></div><div><span className="field-label">{t.detail.envKey}</span><strong>{selected.envKey || '—'}</strong><span className="masked-value">••••••••••••••••Km2</span></div><span className="valid-badge">{statusText(selected.status, t)}</span></div>
                  <div className="detail-fields"><Field label={t.detail.provider} value={selected.provider} /><Field label={t.detail.status} value={statusText(selected.status, t)} /><div><span className="field-label">{t.detail.tags}</span><div className="tag-row">{selected.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div></div><Field label={t.detail.notes} value={selected.description || (locale === 'zh-CN' ? '仅用于本地开发环境。' : 'For local development only.')} /></div>
                  <div className="detail-actions"><button className="secondary-button" onClick={() => runBackendAction(() => backend.copy(selected.id), t.detail.copyApiKeyDone)}>{selected.valueType === 'api_key' ? t.detail.copyApiKey : t.detail.copy}</button><button className="secondary-button" onClick={() => runBackendAction(() => backend.copyAll(selected.id), t.detail.copyAllDone)}>{t.detail.copyAll}</button><button className="secondary-button" onClick={() => runBackendAction(() => backend.reveal(selected.id))}>{t.detail.reveal}</button><button className="secondary-button" onClick={() => void validateSecret(selected.id)}>{t.detail.validate}</button></div>{actionMessage && <div className="form-success action-message">{actionMessage}</div>}{validationMessage && <div className="form-success validation-message">{validationMessage}</div>}<button className="edit-button" onClick={() => { setEditingSecret(selected); setShowForm(true); }}>{t.detail.edit}</button><button className="delete-button" onClick={() => void removeSelected()}>{t.detail.delete}</button>
                  <div className="detail-note"><span className="lock-small">◆</span><span>{t.security.warning}</span></div>
                </> : <div className="empty-detail">{t.detail.noSelection}</div>}
              </section>
            </div>
          </>
        )}
        <footer className="app-footer"><span>SecretHub / {t.footer.version}</span><span>{t.footer.vault} · {t.footer.synced}</span></footer>
      </main>
      {showForm && <SecretForm t={t} initial={editingSecret} onClose={() => { setShowForm(false); setEditingSecret(null); }} onSave={saveSecret} />}
      {showExport && <ExportPanel t={t} selectedIds={selectedIds} onClose={() => setShowExport(false)} />}
      {showImport && <ImportPanel t={t} onClose={() => setShowImport(false)} onDone={() => { setShowImport(false); void refreshSecrets(); }} />}
      {showProfilePrompt && <ProfilePrompt t={t} selectedIds={selectedIds} onClose={() => setShowProfilePrompt(false)} onSaved={() => { setShowProfilePrompt(false); setActiveNav('profiles'); }} />}
    </div>
  );
}

function VaultGate({ status, t, onReady }: { status: VaultStatus; t: ReturnType<typeof getTranslation>; onReady: () => void }) {
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState('');
  const setup = !status.initialized;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (setup && password !== confirmation) { setError(t.auth.mismatch); return; }
    if (password.length < 12) { setError(t.auth.minLength); return; }
    try { if (setup) await backend.setupMaster(password); else await backend.unlock(password); onReady(); } catch (reason) { setError(String(reason)); }
  }
  return <section className="vault-gate"><div className="gate-orb"><span /></div><span className="section-kicker">LOCAL VAULT / {setup ? 'FIRST RUN' : 'LOCKED'}</span><h2>{setup ? t.auth.setupTitle : t.auth.unlockTitle}</h2><p>{t.auth.subtitle}</p><form onSubmit={submit}><label>{t.auth.password}<input autoFocus type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete={setup ? 'new-password' : 'current-password'} /></label>{setup && <label>{t.auth.confirmPassword}<input type="password" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} autoComplete="new-password" /></label>}{error && <div className="form-error" role="alert">{error}</div>}<button className="primary-button" type="submit">{setup ? t.auth.setup : t.auth.unlock}</button></form></section>;
}

function NavItem({ label, active, count, dot, onClick }: { label: string; active?: boolean; count?: string; dot?: string; onClick: () => void }) {
  return <button className={`nav-item ${active ? 'active' : ''}`} onClick={onClick}><span className={dot ? `provider-dot ${dot}` : 'nav-glyph'}>{dot ? '' : '·'}</span>{label}{count && <span className="nav-count">{count}</span>}</button>;
}

function statusText(status: string, t: ReturnType<typeof getTranslation>): string {
  return ({ valid: t.list.valid, invalid: t.list.invalid, unauthorized: t.list.unauthorized, rate_limited: t.list.rateLimited, network_error: t.list.networkError, unsupported: t.list.unsupported, unknown: t.list.unknown } as Record<string, string>)[status] ?? status;
}

function SecretRow({ secret, selected, checked, t, onClick, onToggle }: { secret: SecretMetadata; selected: boolean; checked: boolean; t: ReturnType<typeof getTranslation>; onClick: () => void; onToggle: () => void }) {
  return <article className={`secret-row ${selected ? 'selected' : ''}`} onClick={onClick}><input type="checkbox" checked={checked} onChange={onToggle} onClick={(event) => event.stopPropagation()} aria-label={`${t.actions.export} ${secret.name}`} /><div className="row-avatar">{secret.provider.slice(0, 1)}</div><div className="row-copy"><strong>{secret.name}</strong><span>{secret.envKey}</span><small>{secret.provider} · {secret.updated}</small></div><span className={`row-status ${secret.status}`} />{secret.favorite && <span className="favorite">★</span>}</article>;
}

function Field({ label, value }: { label: string; value: string }) { return <div><span className="field-label">{label}</span><strong className="field-value">{value}</strong></div>; }

function SettingsPanel({ t }: { t: ReturnType<typeof getTranslation> }) {
  const [message, setMessage] = useState('');
  async function exportBackup() {
    const folder = isTauriRuntime() ? await backend.chooseFolder() : '.';
    if (!folder) return;
    if (isTauriRuntime()) await backend.backup(`${folder}/SecretHub.secrethub-backup`);
    setMessage(t.settings.backupSaved);
  }
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">CONTROL CENTER</span><h2>{t.settings.title}</h2><p>{t.security.warning}</p></div><div className="settings-grid"><SettingGroup title={t.settings.general} items={[[t.settings.theme, 'Dark / Light'], [t.settings.defaultExport, '.env']]}/><SettingGroup title={t.settings.security} items={[[t.settings.autoLock, '15 min'], [t.settings.requireAuth, t.settings.on]]}/><SettingGroup title={t.settings.ai} items={[[t.settings.language, 'zh-CN / en-US'], ['Metadata access', t.settings.on]]}/></div><div className="settings-footer"><button className="secondary-button" onClick={() => void exportBackup()}>{t.settings.backup}</button>{message && <span className="form-success">{message}</span>}</div></section>;
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

function ProfilePanel({ t, selectedIds, onApply }: { t: ReturnType<typeof getTranslation>; selectedIds: string[]; onApply: (ids: string[]) => void }) {
  type Profile = { id: string; name: string; description: string; secret_ids: string[] };
  const [name, setName] = useState(''); const [description, setDescription] = useState(''); const [profiles, setProfiles] = useState<Profile[]>([]); const [editingId, setEditingId] = useState<string | null>(null); const [message, setMessage] = useState(''); const [error, setError] = useState('');
  useEffect(() => { if (isTauriRuntime()) void backend.profiles().then(setProfiles).catch((reason) => setError(String(reason))); }, []);
  async function save() { if (!name.trim()) { setError(t.form.required); return; } const profile = { id: editingId ?? `profile-${Date.now()}`, name: name.trim(), description, secret_ids: selectedIds }; try { if (isTauriRuntime()) await backend.saveProfile(profile); setProfiles((current) => editingId ? current.map((item) => item.id === editingId ? profile : item) : [profile, ...current]); setName(''); setDescription(''); setEditingId(null); setMessage(t.profile.saved); setError(''); } catch (reason) { setError(String(reason)); } }
  async function remove(id: string) { if (!window.confirm(t.profile.deleteConfirm)) return; try { if (isTauriRuntime()) await backend.deleteProfile(id); setProfiles((current) => current.filter((item) => item.id !== id)); } catch (reason) { setError(String(reason)); } }
  function edit(profile: Profile) { setEditingId(profile.id); setName(profile.name); setDescription(profile.description); setMessage(''); }
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">PROJECT RECIPE / {selectedIds.length} SELECTED</span><h2>{t.profile.title}</h2><p>{t.security.warning}</p></div><div className="profile-editor"><label>{t.profile.name}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t.profile.description}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button className="primary-button" onClick={() => void save()}>{editingId ? t.profile.edit : t.profile.create}</button>{editingId && <button className="secondary-button" onClick={() => { setEditingId(null); setName(''); setDescription(''); }}>{t.profile.cancel}</button>}{message && <span className="form-success">{message}</span>}{error && <span className="form-error" role="alert">{error}</span>}</div><div className="profile-list">{profiles.length === 0 ? <div className="empty-state"><strong>{t.profile.empty}</strong></div> : profiles.map((profile) => <div className="profile-row" key={profile.id}><div><strong>{profile.name}</strong><span>{profile.secret_ids.length} {t.nav.secrets}{profile.description ? ` · ${profile.description}` : ''}</span></div><div className="profile-actions"><button className="secondary-button" onClick={() => edit(profile)}>{t.profile.edit}</button><button className="delete-button" onClick={() => void remove(profile.id)}>{t.profile.delete}</button><button className="secondary-button" onClick={() => onApply(profile.secret_ids)}>{t.profile.apply}</button></div></div>)}</div></section>;
}

function ProjectsPanel({ t }: { t: ReturnType<typeof getTranslation> }) {
  const [projects, setProjects] = useState<Array<{ id: string; path: string; display_name: string }>>([]); const [message, setMessage] = useState('');
  useEffect(() => { if (isTauriRuntime()) void backend.projects().then(setProjects).catch(() => undefined); }, []);
  async function record() { const path = isTauriRuntime() ? await backend.chooseFolder() : '.'; if (!path) return; const display = path.split(/[\\/]/).filter(Boolean).pop() || 'project'; if (isTauriRuntime()) await backend.recordProject(path, display); setProjects((current) => [{ id: `project-${Date.now()}`, path, display_name: display }, ...current]); setMessage(t.project.record); }
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">PROJECT CONTEXT / LOCAL ONLY</span><h2>{t.project.title}</h2><p>{t.project.recent}</p></div><button className="primary-button" onClick={record}>{t.project.record}</button>{message && <div className="form-success project-message">{message}</div>}<div className="project-list">{projects.length === 0 ? <div className="empty-state"><strong>{t.project.empty}</strong></div> : projects.map((project) => <div className="project-row" key={project.id}><strong>{project.display_name}</strong><span>{project.path}</span></div>)}</div></section>;
}

function AuditPanel({ t }: { t: ReturnType<typeof getTranslation> }) {
  const [events, setEvents] = useState<Array<{ id: string; operation: string; result: string }>>([]);
  useEffect(() => { if (isTauriRuntime()) void backend.audit().then(setEvents).catch(() => undefined); }, []);
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">SECURITY / TRACEABLE</span><h2>{t.auditPanel.title}</h2><p>{t.security.audit}</p></div><div className="audit-list">{events.length === 0 ? <div className="empty-state"><strong>{t.auditPanel.empty}</strong></div> : events.map((event) => <div className="audit-row" key={event.id}><strong>{event.operation}</strong><span>{event.result}</span></div>)}</div></section>;
}

function McpPlanBanner({ plan, t, onConfirm }: { plan: PendingPlan; t: ReturnType<typeof getTranslation>; onConfirm: () => void }) {
  return <section className="mcp-plan" aria-label={t.mcp.title}><div><span className="section-kicker">MCP / CONFIRMATION REQUIRED</span><strong>{t.mcp.title}</strong><p>{t.mcp.description}</p><code>{plan.project_path}</code><small>{plan.secret_ids.length} {t.nav.secrets}</small></div><button className="primary-button" onClick={onConfirm}>{t.mcp.confirm}</button></section>;
}

function SettingGroup({ title, items }: { title: string; items: string[][] }) { return <div className="setting-group"><h3>{title}</h3>{items.map(([label, value]) => <div className="setting-row" key={label}><span>{label}</span><strong>{value}</strong></div>)}</div>; }

type SecretRequest = { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; endpoint_url: string; endpoint_env_key: string };
type CredentialType = 'api_key' | 'token' | 'url' | 'password' | 'text';

function SecretForm({ t, initial, onClose, onSave }: { t: ReturnType<typeof getTranslation>; initial: SecretMetadata | null; onClose: () => void; onSave: (request: SecretRequest) => Promise<void> }) {
  const existingProvider = initial ? providerTemplates.find((item) => item.id === initial.provider || item.name === initial.provider)?.id ?? 'custom' : '';
  const [mode, setMode] = useState<'type' | 'provider' | 'form'>(initial ? 'form' : 'type');
  const [credentialType, setCredentialType] = useState<CredentialType>((initial?.valueType as CredentialType) ?? 'api_key');
  const [name, setName] = useState(initial?.name ?? '');
  const [envKey, setEnvKey] = useState(initial?.envKey ?? '');
  const [provider, setProvider] = useState(existingProvider);
  const [value, setValue] = useState('');
  const [valueType, setValueType] = useState(initial?.valueType ?? 'api_key');
  const [category, setCategory] = useState(initial?.category ?? 'other');
  const [scope, setScope] = useState(initial?.scope ?? 'global');
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState(initial?.tags.join(', ') ?? '');
  const [notes, setNotes] = useState(initial?.description ?? '');
  const [modelId, setModelId] = useState(initial?.modelId ?? '');
  const [modelEnvKey, setModelEnvKey] = useState(initial?.modelEnvKey ?? '');
  const [endpointUrl, setEndpointUrl] = useState(initial?.endpointUrl ?? '');
  const [endpointEnvKey, setEndpointEnvKey] = useState(initial?.endpointEnvKey ?? '');
  const [error, setError] = useState('');
  const template: ProviderTemplate = providerTemplate(provider);
  const isCustomModel = template.id === 'custom' || !template.models.some((model) => model.id === modelId);

  function selectTemplate(next: ProviderTemplate) {
    const firstModel = next.models[0];
    setProvider(next.id);
    setName((current) => current || next.name);
    setEnvKey((current) => current || defaultApiKeyEnvKey(next.id));
    setModelId(firstModel.id);
    setModelEnvKey(firstModel.envKey);
    setEndpointUrl(next.baseUrl);
    setEndpointEnvKey(defaultEndpointEnvKey(next.id));
    setCategory(next.id === 'custom' ? 'other' : 'ai');
    setMode('form');
  }

  function selectCredentialType(next: CredentialType) {
    setCredentialType(next);
    setValueType(next);
    setProvider(next === 'api_key' ? '' : 'custom');
    setModelId('');
    setModelEnvKey('');
    setEnvKey(next === 'api_key' ? '' : next === 'token' ? 'TOKEN' : next === 'url' ? 'URL' : next.toUpperCase());
    setMode(next === 'api_key' ? 'provider' : 'form');
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    try {
      if (credentialType === 'api_key' && (!modelId.trim() || !endpointUrl.trim())) throw new Error(t.form.apiKeyFieldsRequired);
      await onSave({ name, provider_id: provider || 'custom', env_key: envKey.trim().toUpperCase(), description: notes, tags: tags.split(',').map((tag) => tag.trim()).filter(Boolean), value, value_type: valueType, category, scope, favorite, archived: initial?.archived ?? false, model_id: modelId.trim(), model_env_key: modelEnvKey.trim().toUpperCase(), endpoint_url: endpointUrl.trim(), endpoint_env_key: endpointEnvKey.trim().toUpperCase() });
      onClose();
    } catch (reason) { setError(String(reason)); }
  }

  if (mode === 'type') {
    const typeOptions: Array<{ id: CredentialType; label: string; hint: string }> = [
      { id: 'api_key', label: t.form.apiKeyType, hint: t.form.apiKeyHint },
      { id: 'token', label: t.form.tokenType, hint: t.form.tokenHint },
      { id: 'url', label: t.form.urlType, hint: t.form.urlHint },
      { id: 'password', label: t.form.passwordType, hint: t.form.passwordHint },
      { id: 'text', label: t.form.textType, hint: t.form.textHint },
    ];
    return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.form.typeTitle}><div className="secret-form credential-type-picker"><div className="form-heading"><div><span className="section-kicker">CREDENTIAL ROUTER / TYPE</span><h2>{t.form.typeTitle}</h2><p className="form-intro">不同类型凭据进入不同表单，API KEY 才需要选择 Provider 和模型。</p></div><button type="button" className="close-button" onClick={onClose}>×</button></div><div className="credential-type-grid">{typeOptions.map((item) => <button type="button" className="credential-type-card" key={item.id} onClick={() => selectCredentialType(item.id)}><strong>{item.label}</strong><span>{item.hint}</span></button>)}</div></div></div>;
  }

  if (mode === 'provider') {
    return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.form.addTitle}><div className="secret-form provider-picker"><div className="form-heading"><div><span className="section-kicker">API KEY / PROVIDER</span><h2>{t.form.addTitle}</h2><p className="form-intro">先选择服务商，下一步会自动填充模型和环境变量模板。</p></div><div className="form-heading-actions"><button type="button" className="back-button" onClick={() => setMode('type')}>{t.form.back}</button><button type="button" className="close-button" onClick={onClose}>×</button></div></div><div className="provider-grid">{providerTemplates.map((item) => <button type="button" className="provider-card" key={item.id} onClick={() => selectTemplate(item)}><strong>{item.name}</strong><span>{item.models.length} models</span>{item.keyUrl && <small>API Key ↗</small>}</button>)}</div></div></div>;
  }

  const modelOptions = template.models;
  async function openExternal(url: string) {
    try { if (isTauriRuntime()) await backend.openExternalUrl(url); else window.open(url, '_blank', 'noopener,noreferrer'); } catch (reason) { setError(String(reason)); }
  }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={initial ? t.form.editTitle : t.form.addTitle}><form className="secret-form" onSubmit={submit}><div className="form-heading"><div><span className="section-kicker">{credentialType === 'api_key' ? `API KEY / ${template.name.toUpperCase()}` : `${credentialType.toUpperCase()} / ${credentialType.toUpperCase()}`}</span><h2>{initial ? t.form.editTitle : t.form.addTitle}</h2><p className="form-intro">{credentialType === 'api_key' ? '已预填该服务商常用配置，可按需修改。' : '这是该凭据类型的专用表单。'}</p></div><button type="button" className="close-button" onClick={onClose}>×</button></div>{credentialType === 'api_key' && <><label>{t.form.provider}<select value={provider} onChange={(event) => selectTemplate(providerTemplate(event.target.value))}>{providerTemplates.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}</select></label>{template.keyUrl && <div className="provider-link-row"><span>API Key</span><a href={template.keyUrl} target="_blank" rel="noreferrer" onClick={(event) => { event.preventDefault(); void openExternal(template.keyUrl); }}>前往官网生成 / 更新 ↗</a></div>}</>}<label>{t.form.name}<input required value={name} onChange={(event) => setName(event.target.value)} placeholder={t.form.required} /></label><label>{t.form.envKey}<span className="optional-label">（可选）</span><input value={envKey} onChange={(event) => setEnvKey(event.target.value)} placeholder="OPENAI_API_KEY" /></label><label>{t.form.value}<input required={!initial} value={value} onChange={(event) => setValue(event.target.value)} type={credentialType === 'url' ? 'url' : credentialType === 'text' ? 'text' : 'password'} autoComplete="new-password" placeholder={initial ? t.form.keepExistingValue : undefined} />{credentialType === 'url' && value.trim() && <button type="button" className="inline-link" onClick={() => void openExternal(value.trim())}>打开网址 ↗</button>}</label>{credentialType === 'api_key' && <><div className="form-two-col"><label>{t.form.model}<select value={isCustomModel ? '__custom__' : modelId} onChange={(event) => { const next = event.target.value; if (next === '__custom__') { setModelId(''); setModelEnvKey('MODEL_NAME'); } else { const found = modelOptions.find((item) => item.id === next); setModelId(next); if (found) setModelEnvKey(found.envKey); } }}>{modelOptions.map((model) => <option value={model.id} key={model.id}>{model.label} · {model.id}</option>)}<option value="__custom__">自定义模型 ID</option></select>{isCustomModel && <input value={modelId} onChange={(event) => setModelId(event.target.value)} placeholder="your-model-id" />}</label><label>{t.form.modelEnvKey}<input value={modelEnvKey} onChange={(event) => setModelEnvKey(event.target.value)} placeholder="OPENAI_MODEL" /></label></div><div className="form-two-col"><label>{t.form.endpointUrl}<input required value={endpointUrl} onChange={(event) => setEndpointUrl(event.target.value)} placeholder="https://api.example.com/v1" /></label><label>{t.form.endpointEnvKey}<input required value={endpointEnvKey} onChange={(event) => setEndpointEnvKey(event.target.value)} placeholder="OPENAI_BASE_URL" /></label></div></>}<div className="form-two-col"><label>{t.form.valueType}<select value={valueType} onChange={(event) => { const next = event.target.value as CredentialType; setCredentialType(next); setValueType(next); }}>{credentialType === 'api_key' && <option value="api_key">API key</option>}{credentialType !== 'api_key' && <option value={credentialType}>{credentialType.toUpperCase()}</option>}</select></label><label>{t.form.category}<select value={category} onChange={(event) => setCategory(event.target.value)}><option value="ai">AI</option><option value="database">Database</option><option value="cloud">Cloud</option><option value="git">Git</option><option value="other">Other</option></select></label></div><label>{t.form.scope}<select value={scope} onChange={(event) => setScope(event.target.value)}><option value="global">Global</option><option value="project">Project</option><option value="profile">Profile</option></select></label><label className="checkbox-label"><input type="checkbox" checked={favorite} onChange={(event) => setFavorite(event.target.checked)} />{t.form.favorite}</label><label>{t.form.tags}<input value={tags} onChange={(event) => setTags(event.target.value)} placeholder="ai, production" /></label><label>{t.form.notes}<textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={3} /></label>{error && <div className="form-error" role="alert">{error}</div>}<div className="form-security"><span>◆</span>{t.security.encrypted}</div><div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button><button className="primary-button" type="submit">{t.form.save}</button></div></form></div>;
}

function ExportPanel({ t, selectedIds, onClose }: { t: ReturnType<typeof getTranslation>; selectedIds: string[]; onClose: () => void }) {
  const [directory, setDirectory] = useState('.'); const [preview, setPreview] = useState(''); const [message, setMessage] = useState(''); const [error, setError] = useState(''); const [conflicts, setConflicts] = useState<string[]>([]);
  const request = { directory, secret_ids: selectedIds, write_example: true, replace_existing: false, ensure_gitignore: true };
  async function chooseFolder() { if (isTauriRuntime()) { const folder = await backend.chooseFolder(); if (folder) setDirectory(folder); } }
  async function showPreview() { try { if (selectedIds.length === 0) throw new Error('select at least one secret'); if (isTauriRuntime()) { setPreview(await backend.preview(request)); setConflicts(await backend.conflicts(request)); } else { setPreview(selectedIds.map((id) => `${id.toUpperCase()}="••••••••"`).join('\n')); setConflicts([]); } setError(''); } catch (reason) { setError(`${t.actions.operationFailed}: ${String(reason)}`); } }
  async function exportFiles() { try { if (selectedIds.length === 0) throw new Error('select at least one secret'); if (isTauriRuntime()) await backend.export(request); setMessage(t.actions.exportDone); setError(''); } catch (reason) { setError(`${t.actions.operationFailed}: ${String(reason)}`); } }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.actions.export}><div className="secret-form export-form"><div className="form-heading"><div><span className="section-kicker">PROJECT OUTPUT / SAFE WRITE</span><h2>{t.actions.export}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><label>{t.actions.chooseFolder}<div className="folder-row"><input value={directory} onChange={(event) => setDirectory(event.target.value)} /><button type="button" className="secondary-button" onClick={chooseFolder}>...</button></div></label><div className="preview-box">{preview || 'Select Preview to inspect masked output.'}</div>{conflicts.length > 0 && <div className="conflict-box">Conflict keys: {conflicts.join(', ')}</div>}{error && <div className="form-error" role="alert">{error}</div>}<div className="form-security"><span>◆</span>{t.security.warning}</div>{message && <div className="form-success">{message}</div>}<div className="form-actions"><button type="button" className="secondary-button" onClick={() => void showPreview()}>{t.actions.preview}</button><button className="primary-button" type="button" onClick={() => void exportFiles()}>{t.actions.export}</button></div></div></div>;
}

function ImportPanel({ t, onClose, onDone }: { t: ReturnType<typeof getTranslation>; onClose: () => void; onDone: () => void }) {
  const [format, setFormat] = useState<'env' | 'json'>('env'); const [contents, setContents] = useState(''); const [keys, setKeys] = useState<string[]>([]); const [error, setError] = useState(''); const [message, setMessage] = useState('');
  async function preview() { try { if (isTauriRuntime()) setKeys(await backend.importPreview({ format, contents, confirmed: false })); else setKeys(format === 'env' ? contents.split(/\r?\n/).map((line) => line.split('=')[0].trim()).filter(Boolean) : Object.keys(JSON.parse(contents))); setError(''); } catch (reason) { setError(String(reason)); setKeys([]); } }
  async function confirm() { try { if (isTauriRuntime()) await backend.importSecrets({ format, contents, confirmed: true }); setMessage(t.actions.imported); setTimeout(onDone, 500); } catch (reason) { setError(String(reason)); } }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.actions.import}><form className="secret-form import-form" onSubmit={(event) => { event.preventDefault(); void preview(); }}><div className="form-heading"><div><span className="section-kicker">IMPORT / ENCRYPTED</span><h2>{t.actions.import}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><label>{t.actions.importFormat}<select value={format} onChange={(event) => setFormat(event.target.value as 'env' | 'json')}><option value="env">.env</option><option value="json">JSON</option></select></label><label>{t.actions.importContent}<textarea className="import-textarea" rows={8} value={contents} onChange={(event) => setContents(event.target.value)} /></label>{keys.length > 0 && <div className="preview-box">{keys.map((key) => `${key} = [encrypted]`).join('\n')}</div>}{error && <div className="form-error" role="alert">{error}</div>}{message && <div className="form-success">{message}</div>}<div className="form-security"><span>◆</span>{t.security.warning}</div><div className="form-actions"><button type="button" className="secondary-button" onClick={preview}>{t.actions.importPreview}</button><button type="button" className="primary-button" disabled={keys.length === 0} onClick={() => void confirm()}>{t.actions.confirmImport}</button></div></form></div>;
}
