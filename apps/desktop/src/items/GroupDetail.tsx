import { useEffect, useState } from 'react';
import { backend, isTauriRuntime, type GroupPublic } from '../lib/backend';
import type { SecretMetadata } from '../secretTypes';
import type { T } from './ItemFormShell';
import { isKeyName } from './groupTemplates';
import { TypeIcon } from './TypeIcon';

const DOTS = '••••••••';

/**
 * Detail page of a config group. Plain values are shown; secret variables show dots and are
 * revealed by the native dialog or copied by the backend, so their values never enter this
 * component. The notes text appears only after the user asks for it.
 */
export function GroupDetail({ t, secret, onEdit, onDelete }: { t: T; secret: SecretMetadata; onEdit: () => void; onDelete: () => void }) {
  const [group, setGroup] = useState<GroupPublic | null>(null);
  const [notes, setNotes] = useState<string | null>(null);
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setNotes(null);
    setMessage('');
    setGroup(null);
    if (!isTauriRuntime()) return;
    let cancelled = false;
    backend.groupPublic(secret.id)
      .then((loaded) => { if (!cancelled) setGroup(loaded); })
      .catch((reason) => { if (!cancelled) { setFailed(true); setMessage(`${t.items.loadFailed}: ${String(reason)}`); } });
    return () => { cancelled = true; };
  }, [secret.id, secret.updatedAt, t.items.loadFailed]);

  async function act(action: () => Promise<unknown>, done?: string) {
    if (!isTauriRuntime()) return;
    try { await action(); setFailed(false); setMessage(done ?? ''); }
    catch (reason) { setFailed(true); setMessage(`${t.items.actionFailed}: ${String(reason)}`); }
  }

  async function exportKey(name: string) {
    if (!isTauriRuntime()) return;
    try {
      const path = await backend.groupChooseKeyPath(name.toLowerCase());
      if (!path) return;
      await backend.groupExportKeyFile(secret.id, name, path);
      setFailed(false);
      setMessage(`${name}: ${t.groups.keyExported}`);
    } catch (reason) { setFailed(true); setMessage(`${t.items.actionFailed}: ${String(reason)}`); }
  }

  async function showNotes() {
    if (!isTauriRuntime()) return;
    try { setNotes(await backend.groupNotes(secret.id)); setMessage(''); }
    catch (reason) { setFailed(true); setMessage(`${t.items.actionFailed}: ${String(reason)}`); }
  }

  const plainCount = group ? group.variables.filter((variable) => !variable.secret).length : 0;
  const aiInfo = !group ? '' : !group.aiValues ? `${t.groups.aiInfoNoValues} (${t.groups.aiInfoOff})` : plainCount > 0 ? t.groups.aiInfo.replace('{n}', String(plainCount)) : t.groups.aiInfoNoValues;

  return (
    <>
      <div className="detail-heading">
        <div>
          <span className="section-kicker"><TypeIcon kind="env_group" size={12} /> {t.items.types.env_group.name}</span>
          <h2>{secret.name}</h2>
          {group && <small className="detail-subtitle">{t.groups.count.replace('{n}', String(group.variables.length))}</small>}
        </div>
      </div>
      <div className="detail-fields item-fields group-fields">
        {group && group.variables.length === 0 && <p className="cap-hint">{t.groups.empty}</p>}
        {group?.variables.map((variable) => (
          <div className="item-field" key={variable.name} data-variable={variable.name} data-secret={variable.secret ? 'true' : 'false'}>
            <span className="field-label">{variable.name} <small>{variable.secret ? t.groups.secret : t.groups.plain}</small></span>
            {variable.description && <small className="field-description">{variable.description}</small>}
            {variable.secret
              ? <strong className="field-value masked-value" aria-label={`${variable.name}: ${t.detail.masked}`}>{DOTS}</strong>
              : <strong className="field-value">{variable.value || t.groups.emptyValue}</strong>}
            <div className="item-field-actions">
              {variable.secret && <button type="button" className="secondary-button" aria-label={`${t.items.show} ${variable.name}`} onClick={() => void act(() => backend.groupVarReveal(secret.id, variable.name))}>{t.items.show}</button>}
              <button type="button" className="secondary-button" aria-label={`${t.items.copy} ${variable.name}`} onClick={() => void act(() => backend.groupVarCopy(secret.id, variable.name), `${variable.name}: ${t.items.copied}`)}>{t.items.copy}</button>
              {variable.secret && isKeyName(variable.name) && <button type="button" className="secondary-button" aria-label={`${t.groups.exportKey} ${variable.name}`} onClick={() => void exportKey(variable.name)}>{t.groups.exportKey}</button>}
            </div>
          </div>
        ))}
        {group?.hasNotes && (
          <div className="item-field" data-field="notes">
            <span className="field-label">{t.groups.notes}</span>
            {notes !== null ? <pre className="note-body">{notes}</pre> : <strong className="field-value masked-value" aria-label={`${t.groups.notes}: ${t.detail.masked}`}>{DOTS}</strong>}
            <div className="item-field-actions">
              {notes === null
                ? <button type="button" className="secondary-button" onClick={() => void showNotes()}>{t.groups.showNotes}</button>
                : <button type="button" className="secondary-button" onClick={() => setNotes(null)}>{t.groups.hideNotes}</button>}
            </div>
          </div>
        )}
        <div><span className="field-label">{t.detail.tags}</span><div className="tag-row">{secret.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div></div>
      </div>
      {message && <div className={failed ? 'form-error' : 'form-success'} role="status">{message}</div>}
      <button className="edit-button" onClick={onEdit}>{t.detail.edit}</button>
      <button className="delete-button" onClick={onDelete}>{t.detail.delete}</button>
      <div className="detail-note"><span className="lock-small">◆</span><span>{aiInfo || t.groups.groupNote}</span></div>
    </>
  );
}
