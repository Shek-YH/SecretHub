import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { getTranslation, type Locale } from './i18n/translations';
import { backend, isTauriRuntime, type VaultStatus } from './lib/backend';

type SecretMetadata = {
  id: string;
  name: string;
  envKey: string;
  provider: string;
  tags: string[];
  status: 'valid' | 'unknown';
  updated: string;
  favorite?: boolean;
};

const demoSecrets: SecretMetadata[] = [
  { id: 'openai-main', name: 'OpenAI Main', envKey: 'OPENAI_API_KEY', provider: 'OpenAI', tags: ['ai', 'production'], status: 'valid', updated: '2h ago', favorite: true },
  { id: 'anthropic-lab', name: 'Anthropic Lab', envKey: 'ANTHROPIC_API_KEY', provider: 'Anthropic', tags: ['ai', 'research'], status: 'valid', updated: '1d ago' },
  { id: 'supabase-cuecut', name: 'Supabase CueCut', envKey: 'SUPABASE_URL', provider: 'Supabase', tags: ['database', 'cuecut'], status: 'unknown', updated: '3d ago' },
  { id: 'github-build', name: 'GitHub Build', envKey: 'GITHUB_TOKEN', provider: 'GitHub', tags: ['ci', 'developer'], status: 'valid', updated: '5d ago' },
];

export function App() {
  const [locale, setLocale] = useState<Locale>(() => (localStorage.getItem('secrethub-locale') as Locale) || 'zh-CN');
  const [secrets, setSecrets] = useState<SecretMetadata[]>(demoSecrets);
  const [vaultStatus, setVaultStatus] = useState<VaultStatus | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState(demoSecrets[0].id);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [showExport, setShowExport] = useState(false);
  const [activeNav, setActiveNav] = useState<'all' | 'profiles' | 'projects' | 'settings' | 'audit'>('all');
  const t = getTranslation(locale);
  const selected = secrets.find((secret) => secret.id === selectedId) ?? null;
  const filtered = useMemo(() => secrets.filter((secret) => `${secret.name} ${secret.envKey} ${secret.provider} ${secret.tags.join(' ')}`.toLowerCase().includes(query.toLowerCase())), [query, secrets]);

  async function refreshSecrets() {
    if (!isTauriRuntime()) return;
    try { setSecrets((await backend.list(query)).map((secret) => ({ id: secret.id, name: secret.name, envKey: secret.env_key, provider: secret.provider_id, tags: secret.tags, status: secret.status === 'valid' ? 'valid' : 'unknown', updated: 'now' }))); setRuntimeError(null); } catch (error) { setRuntimeError(String(error)); }
  }

  useEffect(() => {
    if (!isTauriRuntime()) return;
    backend.status().then((status) => { setVaultStatus(status); if (status.unlocked) void refreshSecrets(); else setSecrets([]); }).catch((error) => setRuntimeError(String(error)));
  }, []);

  function changeLocale(next: Locale) {
    setLocale(next);
    localStorage.setItem('secrethub-locale', next);
    document.documentElement.lang = next;
  }

  function toggleSelected(id: string) {
    setSelectedIds((current) => current.includes(id) ? current.filter((value) => value !== id) : [...current, id]);
  }

  async function saveSecret(request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string }) {
    if (isTauriRuntime()) { await backend.create(request); await refreshSecrets(); return; }
    const localSecret: SecretMetadata = { id: `local-${Date.now()}`, name: request.name, envKey: request.env_key, provider: request.provider_id, tags: request.tags, status: 'unknown', updated: 'now' };
    setSecrets((current) => [localSecret, ...current]);
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
            <button className="primary-button" onClick={() => setShowForm(true)}><span aria-hidden="true">+</span>{t.header.add}</button>
          </div>
        </header>
        <section className="security-strip" aria-label="Security status">
          <span className="shield-icon" aria-hidden="true">◆</span><span>{t.security.localOnly}</span><span className="strip-divider" /><span>{t.security.encrypted}</span><span className="strip-divider" /><span>{t.security.audit}</span>
        </section>
        {runtimeError && <div className="runtime-error" role="alert">{runtimeError}</div>}
        {vaultStatus && !vaultStatus.unlocked ? <VaultGate status={vaultStatus} t={t} onReady={() => { setVaultStatus({ initialized: true, unlocked: true }); void refreshSecrets(); }} /> : activeNav === 'settings' ? <SettingsPanel t={t} /> : activeNav === 'profiles' ? <ProfilePanel t={t} selectedIds={selectedIds} /> : activeNav === 'projects' ? <ProjectsPanel t={t} /> : activeNav === 'audit' ? <AuditPanel t={t} /> : (
          <>
            <div className="content-toolbar">
              <label className="search-box"><span aria-hidden="true">/</span><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t.header.search} aria-label={t.header.search} /><kbd>⌘ K</kbd></label>
              {selectedIds.length > 0 && <div className="selection-actions"><span>{selectedIds.length} {t.list.selected}</span><button onClick={() => setShowExport(true)}>{t.actions.export}</button><button onClick={() => undefined}>{t.actions.profile}</button><button onClick={() => setSelectedIds([])}>×</button></div>}
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
                  <div className="secret-visual"><div className="secret-orb"><span /></div><div><span className="field-label">{t.detail.envKey}</span><strong>{selected.envKey}</strong><span className="masked-value">••••••••••••••••Km2</span></div><span className="valid-badge">{selected.status === 'valid' ? t.list.valid : t.list.unknown}</span></div>
                  <div className="detail-fields"><Field label={t.detail.provider} value={selected.provider} /><Field label={t.detail.status} value={selected.status === 'valid' ? t.list.valid : t.list.unknown} /><div><span className="field-label">{t.detail.tags}</span><div className="tag-row">{selected.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div></div><Field label={t.detail.notes} value={locale === 'zh-CN' ? '仅用于本地开发环境。' : 'For local development only.'} /></div>
                  <div className="detail-actions"><button className="secondary-button" onClick={() => { if (isTauriRuntime()) void backend.copy(selected.id); }}>{t.detail.copy}</button><button className="secondary-button" onClick={() => { if (isTauriRuntime()) void backend.reveal(selected.id); }}>{t.detail.reveal}</button><button className="secondary-button" onClick={() => undefined}>{t.detail.validate}</button></div><button className="edit-button" onClick={() => setShowForm(true)}>{t.detail.edit}</button>
                  <div className="detail-note"><span className="lock-small">◆</span><span>{t.security.warning}</span></div>
                </> : <div className="empty-detail">{t.detail.noSelection}</div>}
              </section>
            </div>
          </>
        )}
        <footer className="app-footer"><span>SecretHub / {t.footer.version}</span><span>{t.footer.vault} · {t.footer.synced}</span></footer>
      </main>
      {showForm && <SecretForm t={t} onClose={() => setShowForm(false)} onSave={saveSecret} />}
      {showExport && <ExportPanel t={t} selectedIds={selectedIds} onClose={() => setShowExport(false)} />}
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

function SecretRow({ secret, selected, checked, t, onClick, onToggle }: { secret: SecretMetadata; selected: boolean; checked: boolean; t: ReturnType<typeof getTranslation>; onClick: () => void; onToggle: () => void }) {
  return <article className={`secret-row ${selected ? 'selected' : ''}`} onClick={onClick}><input type="checkbox" checked={checked} onChange={onToggle} onClick={(event) => event.stopPropagation()} aria-label={`${t.actions.export} ${secret.name}`} /><div className="row-avatar">{secret.provider.slice(0, 1)}</div><div className="row-copy"><strong>{secret.name}</strong><span>{secret.envKey}</span><small>{secret.provider} · {secret.updated}</small></div><span className={`row-status ${secret.status}`} />{secret.favorite && <span className="favorite">★</span>}</article>;
}

function Field({ label, value }: { label: string; value: string }) { return <div><span className="field-label">{label}</span><strong className="field-value">{value}</strong></div>; }

function SettingsPanel({ t }: { t: ReturnType<typeof getTranslation> }) {
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">CONTROL CENTER</span><h2>{t.settings.title}</h2><p>{t.security.warning}</p></div><div className="settings-grid"><SettingGroup title={t.settings.general} items={[[t.settings.theme, 'Dark / Light'], [t.settings.defaultExport, '.env']]}/><SettingGroup title={t.settings.security} items={[[t.settings.autoLock, '15 min'], [t.settings.requireAuth, t.settings.on]]}/><SettingGroup title={t.settings.ai} items={[[t.settings.language, 'zh-CN / en-US'], ['Metadata access', t.settings.on]]}/></div></section>;
}

function ProfilePanel({ t, selectedIds }: { t: ReturnType<typeof getTranslation>; selectedIds: string[] }) {
  const [name, setName] = useState('AI Standard'); const [profiles, setProfiles] = useState<Array<{ id: string; name: string; secret_ids: string[] }>>([]); const [message, setMessage] = useState('');
  useEffect(() => { if (isTauriRuntime()) void backend.profiles().then(setProfiles).catch(() => undefined); }, []);
  async function save() { const profile = { id: `profile-${Date.now()}`, name, description: '', secret_ids: selectedIds }; if (isTauriRuntime()) await backend.saveProfile(profile); setProfiles((current) => [profile, ...current]); setMessage(t.profile.saved); }
  return <section className="settings-panel"><div className="settings-heading"><span className="section-kicker">PROJECT RECIPE / {selectedIds.length} SELECTED</span><h2>{t.profile.title}</h2><p>{t.security.warning}</p></div><div className="profile-editor"><label>{t.profile.name}<input value={name} onChange={(event) => setName(event.target.value)} /></label><button className="primary-button" onClick={save}>{t.profile.save}</button>{message && <span className="form-success">{message}</span>}</div><div className="profile-list">{profiles.length === 0 ? <div className="empty-state"><strong>{t.profile.empty}</strong></div> : profiles.map((profile) => <div className="profile-row" key={profile.id}><div><strong>{profile.name}</strong><span>{profile.secret_ids.length} {t.nav.secrets}</span></div><button className="secondary-button" onClick={() => setMessage(`${t.profile.apply}: ${profile.name}`)}>{t.profile.apply}</button></div>)}</div></section>;
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

function SettingGroup({ title, items }: { title: string; items: string[][] }) { return <div className="setting-group"><h3>{title}</h3>{items.map(([label, value]) => <div className="setting-row" key={label}><span>{label}</span><strong>{value}</strong></div>)}</div>; }

function SecretForm({ t, onClose, onSave }: { t: ReturnType<typeof getTranslation>; onClose: () => void; onSave: (request: { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string }) => Promise<void> }) {
  const [name, setName] = useState(''); const [envKey, setEnvKey] = useState(''); const [provider, setProvider] = useState('OpenAI'); const [value, setValue] = useState(''); const [tags, setTags] = useState(''); const [notes, setNotes] = useState(''); const [error, setError] = useState('');
  async function submit(event: FormEvent) { event.preventDefault(); try { await onSave({ name, provider_id: provider.toLowerCase(), env_key: envKey, description: notes, tags: tags.split(',').map((tag) => tag.trim()).filter(Boolean), value }); onClose(); } catch (reason) { setError(String(reason)); } }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.form.addTitle}><form className="secret-form" onSubmit={submit}><div className="form-heading"><div><span className="section-kicker">NEW ENTRY / ENCRYPTED</span><h2>{t.form.addTitle}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><label>{t.form.name}<input required value={name} onChange={(event) => setName(event.target.value)} placeholder={t.form.required} /></label><label>{t.form.envKey}<input required value={envKey} onChange={(event) => setEnvKey(event.target.value)} placeholder="OPENAI_API_KEY" /></label><label>{t.form.provider}<select value={provider} onChange={(event) => setProvider(event.target.value)}><option>OpenAI</option><option>Anthropic</option><option>Generic</option></select></label><label>{t.form.value}<input required value={value} onChange={(event) => setValue(event.target.value)} type="password" autoComplete="new-password" /></label><label>{t.form.tags}<input value={tags} onChange={(event) => setTags(event.target.value)} placeholder="ai, production" /></label><label>{t.form.notes}<textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={3} /></label>{error && <div className="form-error" role="alert">{error}</div>}<div className="form-security"><span>◆</span>{t.security.encrypted}</div><div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button><button className="primary-button" type="submit">{t.form.save}</button></div></form></div>;
}

function ExportPanel({ t, selectedIds, onClose }: { t: ReturnType<typeof getTranslation>; selectedIds: string[]; onClose: () => void }) {
  const [directory, setDirectory] = useState('.'); const [preview, setPreview] = useState(''); const [message, setMessage] = useState('');
  const request = { directory, secret_ids: selectedIds, write_example: true, replace_existing: false, ensure_gitignore: true };
  async function chooseFolder() { if (isTauriRuntime()) { const folder = await backend.chooseFolder(); if (folder) setDirectory(folder); } }
  async function showPreview() { if (isTauriRuntime()) setPreview(await backend.preview(request)); else setPreview(selectedIds.map((id) => `${id.toUpperCase()}="••••••••"`).join('\n')); }
  async function exportFiles() { if (isTauriRuntime()) await backend.export(request); setMessage('Export complete'); }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.actions.export}><div className="secret-form export-form"><div className="form-heading"><div><span className="section-kicker">PROJECT OUTPUT / SAFE WRITE</span><h2>{t.actions.export}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div><label>{t.actions.chooseFolder}<div className="folder-row"><input value={directory} onChange={(event) => setDirectory(event.target.value)} /><button type="button" className="secondary-button" onClick={chooseFolder}>...</button></div></label><div className="preview-box">{preview || 'Select Preview to inspect masked output.'}</div><div className="form-security"><span>◆</span>{t.security.warning}</div>{message && <div className="form-success">{message}</div>}<div className="form-actions"><button type="button" className="secondary-button" onClick={showPreview}>{t.actions.preview}</button><button className="primary-button" type="button" onClick={exportFiles}>{t.actions.export}</button></div></div></div>;
}
