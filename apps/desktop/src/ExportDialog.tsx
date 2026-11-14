import { useEffect, useMemo, useState } from 'react';
import type { getTranslation } from './i18n/translations';
import { backend, isTauriRuntime, type ExportRow, type ExportSelection, type GroupPublic, type ProfileVariables } from './lib/backend';
import type { SecretMetadata } from './secretTypes';

type T = ReturnType<typeof getTranslation>;
type Recent = { id: string; path: string; display_name: string };

type Props = {
  t: T;
  secrets: SecretMetadata[];
  selectedIds: string[];
  /** Subsets carried over from a profile; a group without an entry starts with every variable checked. */
  initialVariables?: ProfileVariables;
  onClose: () => void;
  onExported: () => void;
  onProfileSaved?: () => void;
};

/**
 * Export of a mixed selection: list the chosen items (config groups expand into per-variable
 * checkboxes), pick the project folder, review a preview of every variable and its state, choose
 * overwrite or skip for names already in the target `.env`, then write. Secret values never reach
 * this component: the preview carries masked text for them.
 */
export function ExportDialog({ t, secrets, selectedIds, initialVariables = {}, onClose, onExported, onProfileSaved }: Props) {
  const [ids, setIds] = useState(selectedIds);
  const [groups, setGroups] = useState<Record<string, GroupPublic>>({});
  // Checked variable names per group; absent means "not loaded yet".
  const [checked, setChecked] = useState<Record<string, string[]>>({});
  const [expanded, setExpanded] = useState<string[]>([]);
  const [directory, setDirectory] = useState('');
  const [recent, setRecent] = useState<Recent[]>([]);
  const [rows, setRows] = useState<ExportRow[]>([]);
  const [skip, setSkip] = useState<string[]>([]);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [profileName, setProfileName] = useState('');

  const items = ids.map((id) => secrets.find((secret) => secret.id === id)).filter((item): item is SecretMetadata => Boolean(item));

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void backend.projects().then((projects) => setRecent(projects.slice(0, 8))).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    for (const item of items) {
      if (item.valueType !== 'env_group' || groups[item.id]) continue;
      void backend.groupPublic(item.id).then((group) => {
        setGroups((current) => ({ ...current, [item.id]: group }));
        const wanted = initialVariables[item.id];
        const names = group.variables.map((variable) => variable.name);
        setChecked((current) => ({ ...current, [item.id]: wanted ? names.filter((name) => wanted.includes(name)) : names }));
      }).catch((reason) => setError(String(reason)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ids]);

  const selections = useMemo<ExportSelection[]>(() => items.map((item) => {
    if (item.valueType !== 'env_group') return { secretId: item.id };
    const group = groups[item.id];
    const names = checked[item.id];
    // All checked means "the whole group", so later additions to the group are still exported.
    if (!group || !names || names.length === group.variables.length) return { secretId: item.id };
    return { secretId: item.id, variables: names };
  }), [items, groups, checked]);

  // A group still loading, or one with nothing checked, would export the wrong thing.
  const incomplete = items.some((item) => item.valueType === 'env_group' && (!groups[item.id] || (checked[item.id]?.length ?? 0) === 0));
  const selectionKey = JSON.stringify(selections);

  useEffect(() => {
    setRows([]);
    if (!isTauriRuntime() || !directory.trim() || ids.length === 0 || incomplete) return;
    let cancelled = false;
    backend.exportPreviewV2({ directory, selections, skipKeys: skip })
      .then((preview) => { if (!cancelled) { setRows(preview.rows); setError(''); } })
      .catch((reason) => { if (!cancelled) setError(`${t.actions.operationFailed}: ${String(reason)}`); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [directory, selectionKey, skip.join('\n'), incomplete]);

  const conflicts = rows.some((row) => row.status === 'conflict');
  const clashing = new Set(rows.filter((row) => row.status === 'conflict').map((row) => row.key));

  async function chooseFolder() {
    if (!isTauriRuntime()) return;
    const folder = await backend.chooseFolder();
    if (folder) setDirectory(folder);
  }

  function toggleVariable(id: string, name: string) {
    setChecked((current) => {
      const names = current[id] ?? [];
      return { ...current, [id]: names.includes(name) ? names.filter((item) => item !== name) : [...names, name] };
    });
  }

  function toggleSkip(key: string) {
    setSkip((current) => current.includes(key) ? current.filter((item) => item !== key) : [...current, key]);
  }

  async function write() {
    if (!directory.trim()) { setError(t.exportDialog.needFolder); return; }
    setBusy(true);
    try {
      const outcome = await backend.exportApplyV2({ directory, selections, skipKeys: skip });
      setMessage(t.exportDialog.done.replace('{n}', String(outcome.written.length)).replace('{m}', String(outcome.skipped.length)));
      setError('');
      onExported();
    } catch (reason) { setError(`${t.actions.operationFailed}: ${String(reason)}`); }
    finally { setBusy(false); }
  }

  async function saveAsProfile() {
    if (!profileName.trim()) { setError(t.form.required); return; }
    const variables: ProfileVariables = {};
    for (const selection of selections) if (selection.variables) variables[selection.secretId] = selection.variables;
    try {
      if (isTauriRuntime()) await backend.saveProfile({ id: `profile-${Date.now()}`, name: profileName.trim(), description: '', secret_ids: ids, variables });
      setMessage(t.exportDialog.profileSaved);
      setError('');
      onProfileSaved?.();
    } catch (reason) { setError(`${t.actions.operationFailed}: ${String(reason)}`); }
  }

  const canWrite = !busy && ids.length > 0 && directory.trim() !== '' && rows.length > 0 && !conflicts && !incomplete;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.exportDialog.title}>
      <div className="secret-form export-form export-dialog">
        <div className="form-heading">
          <div><span className="section-kicker">PROJECT OUTPUT / SAFE WRITE</span><h2>{t.exportDialog.title}</h2></div>
          <button type="button" className="close-button" onClick={onClose} aria-label={t.form.cancel}>×</button>
        </div>
        <div className="export-items" aria-label={t.exportDialog.items}>
          {items.map((item) => {
            const group = groups[item.id];
            const names = checked[item.id] ?? [];
            const open = expanded.includes(item.id);
            return (
              <div className="export-item" key={item.id} data-item={item.id}>
                <div className="export-item-head">
                  <strong>{item.name}</strong>
                  {item.valueType === 'env_group' && group && <small>{t.exportDialog.subset.replace('{n}', String(names.length)).replace('{m}', String(group.variables.length))}</small>}
                  {item.valueType === 'env_group' && <button type="button" className="secondary-button" aria-expanded={open} onClick={() => setExpanded((current) => open ? current.filter((id) => id !== item.id) : [...current, item.id])}>{open ? t.exportDialog.collapse : t.exportDialog.expand}</button>}
                  <button type="button" className="secondary-button" aria-label={`${t.exportDialog.removeItem} ${item.name}`} onClick={() => setIds((current) => current.filter((id) => id !== item.id))}>×</button>
                </div>
                {open && group && (
                  <div className="export-variables">
                    {group.variables.map((variable) => (
                      <label className="checkbox-label" key={variable.name}>
                        <input type="checkbox" checked={names.includes(variable.name)} onChange={() => toggleVariable(item.id, variable.name)} />
                        {variable.name}
                      </label>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        <label>
          {t.exportDialog.folder}
          <div className="folder-row">
            <input value={directory} placeholder={t.exportDialog.folderPlaceholder} onChange={(event) => setDirectory(event.target.value)} />
            <button type="button" className="secondary-button" onClick={() => void chooseFolder()}>{t.actions.chooseFolder}</button>
          </div>
        </label>
        {recent.length > 0 && (
          <div className="recent-projects" aria-label={t.exportDialog.recent}>
            <small>{t.exportDialog.recent}</small>
            {recent.map((project) => <button type="button" className="tag recent-chip" key={project.id} title={project.path} onClick={() => setDirectory(project.path)}>{project.display_name}</button>)}
          </div>
        )}
        <table className="export-preview" aria-label={t.exportDialog.preview}>
          <thead><tr><th>{t.exportDialog.colName}</th><th>{t.exportDialog.colSource}</th><th>{t.exportDialog.colValue}</th><th>{t.exportDialog.colStatus}</th></tr></thead>
          <tbody>
            {rows.length === 0 && <tr><td colSpan={4}>{t.exportDialog.noRows}</td></tr>}
            {rows.map((row, index) => (
              <tr key={`${row.secretId}:${row.key}:${index}`} data-key={row.key} data-status={row.status} className={row.status === 'conflict' ? 'export-row-conflict' : ''}>
                <td>{row.key}</td>
                <td>{row.sourceName}</td>
                <td>{row.display}</td>
                <td>
                  {row.status === 'new' && t.exportDialog.statusNew}
                  {row.status === 'conflict' && <span role="alert">{t.exportDialog.statusConflict}</span>}
                  {row.status === 'overwrite' && (
                    <select aria-label={`${row.key} ${t.exportDialog.statusOverwrite}`} value={skip.includes(row.key) ? 'skip' : 'overwrite'} onChange={() => toggleSkip(row.key)}>
                      <option value="overwrite">{t.exportDialog.overwrite}</option>
                      <option value="skip">{t.exportDialog.skip}</option>
                    </select>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {conflicts && <div className="conflict-box" role="alert">{t.exportDialog.conflictHint} ({Array.from(clashing).join(', ')})</div>}
        {error && <div className="form-error" role="alert">{error}</div>}
        {message && <div className="form-success" role="status">{message}</div>}
        <div className="form-security"><span>◆</span>{t.security.warning}</div>
        <div className="export-profile-row">
          <input aria-label={t.exportDialog.profileName} placeholder={t.exportDialog.profileName} value={profileName} onChange={(event) => setProfileName(event.target.value)} />
          <button type="button" className="secondary-button" onClick={() => void saveAsProfile()}>{t.exportDialog.saveProfile}</button>
        </div>
        <div className="form-actions">
          <button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button>
          <button className="primary-button" type="button" disabled={!canWrite} onClick={() => void write()}>{busy ? t.exportDialog.writing : t.exportDialog.write}</button>
        </div>
      </div>
    </div>
  );
}
