import { useEffect, useState } from 'react';
import type { getTranslation } from '../i18n/translations';
import { backend, isTauriRuntime } from '../lib/backend';
import type { SecretMetadata } from '../secretTypes';
import { itemGroup, itemType, fieldLabel, type FieldDef } from './itemTypes';
import { TypeIcon } from './TypeIcon';

type T = ReturnType<typeof getTranslation>;
const DOTS = '••••••••';

/**
 * Detail page of a login, bank card or note. Secret fields show dots only; the value is shown
 * by the native dialog or copied by the backend, so it never enters this component. The note
 * body is the single exception and appears only after the user asks for it.
 */
export function ItemDetail({ t, secret, onEdit, onDelete }: { t: T; secret: SecretMetadata; onEdit: () => void; onDelete: () => void }) {
  const kind = itemGroup(secret);
  const definition = itemType(kind);
  const [fields, setFields] = useState<Record<string, string>>({});
  const [secretKeys, setSecretKeys] = useState<string[]>([]);
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);
  const [body, setBody] = useState<string | null>(null);

  useEffect(() => {
    // A different record (or a saved edit) drops everything that was loaded, including the note body.
    setBody(null);
    setMessage('');
    if (!isTauriRuntime()) { setFields({}); setSecretKeys([]); return; }
    let cancelled = false;
    Promise.all([backend.itemPublicFields(secret.id), backend.itemSecretFields(secret.id)])
      .then(([publicFields, keys]) => { if (!cancelled) { setFields(publicFields); setSecretKeys(keys); } })
      .catch((reason) => { if (!cancelled) { setFailed(true); setMessage(`${t.items.loadFailed}: ${String(reason)}`); } });
    return () => { cancelled = true; };
  }, [secret.id, secret.updatedAt, t.items.loadFailed]);

  async function act(action: () => Promise<unknown>, done?: string) {
    if (!isTauriRuntime()) return;
    try { await action(); setFailed(false); setMessage(done ?? ''); }
    catch (reason) { setFailed(true); setMessage(`${t.items.actionFailed}: ${String(reason)}`); }
  }

  async function showBody() {
    if (!isTauriRuntime()) return;
    try { setBody(await backend.itemNoteBody(secret.id)); setMessage(''); }
    catch (reason) { setFailed(true); setMessage(`${t.items.actionFailed}: ${String(reason)}`); }
  }

  const label = (field: FieldDef) => fieldLabel(field, t.items.labelLocale);
  const website = fields.website ?? '';
  const prefix = secret.devUse ? secret.envKey : '';

  const row = (field: FieldDef) => {
    const name = label(field);
    if (field.sensitive) {
      if (!secretKeys.includes(field.key)) return null;
      const isBody = kind === 'note' && field.key === 'body';
      return (
        <div className="item-field" key={field.key} data-field={field.key}>
          <span className="field-label">{name}</span>
          {isBody && body !== null
            ? <pre className="note-body">{body}</pre>
            : <strong className="field-value masked-value" aria-label={`${name}: ${t.detail.masked}`}>{DOTS}</strong>}
          <div className="item-field-actions">
            {isBody
              ? (body === null
                ? <button type="button" className="secondary-button" onClick={() => void showBody()}>{t.items.showBody}</button>
                : <button type="button" className="secondary-button" onClick={() => setBody(null)}>{t.items.hideBody}</button>)
              : <button type="button" className="secondary-button" aria-label={`${t.items.show} ${name}`} onClick={() => void act(() => backend.itemFieldReveal(secret.id, field.key))}>{t.items.show}</button>}
            <button type="button" className="secondary-button" aria-label={`${t.items.copy} ${name}`} onClick={() => void act(() => backend.itemFieldCopy(secret.id, field.key), `${name}: ${t.items.copied}`)}>{t.items.copy}</button>
          </div>
        </div>
      );
    }
    const value = fields[field.key];
    if (!value) return null;
    return (
      <div className="item-field" key={field.key} data-field={field.key}>
        <span className="field-label">{name}</span>
        <strong className={`field-value${field.kind === 'multiline' ? ' multiline' : ''}`}>{value}</strong>
        <div className="item-field-actions">
          {field.key === 'website' && website.toLowerCase().startsWith('https://') && <button type="button" className="secondary-button" onClick={() => void act(() => backend.openExternalUrl(website))}>{t.items.openSite}</button>}
          <button type="button" className="secondary-button" aria-label={`${t.items.copy} ${name}`} onClick={() => void act(() => backend.itemFieldCopy(secret.id, field.key), `${name}: ${t.items.copied}`)}>{t.items.copy}</button>
        </div>
      </div>
    );
  };

  return (
    <>
      <div className="detail-heading">
        <div>
          <span className="section-kicker"><TypeIcon kind={kind} size={12} /> {t.items.types[kind].name}</span>
          <h2>{secret.name}</h2>
          {secret.subtitle && <small className="detail-subtitle">{secret.subtitle}</small>}
        </div>
      </div>
      <div className="detail-fields item-fields">
        {definition?.fields.map(row)}
        {prefix && <div className="item-field"><span className="field-label">{t.items.devUseInfo}</span><strong className="field-value">{`${prefix}_USERNAME, ${prefix}_PASSWORD`}</strong></div>}
        <div><span className="field-label">{t.detail.tags}</span><div className="tag-row">{secret.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div></div>
      </div>
      {message && <div className={failed ? 'form-error' : 'form-success'} role="status">{message}</div>}
      <button className="edit-button" onClick={onEdit}>{t.detail.edit}</button>
      <button className="delete-button" onClick={onDelete}>{t.detail.delete}</button>
      <div className="detail-note"><span className="lock-small">◆</span><span>{secret.devUse ? t.items.devUseHint : t.items.personalNotice}</span></div>
    </>
  );
}
