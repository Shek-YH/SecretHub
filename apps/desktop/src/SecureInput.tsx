import { useState, type FormEvent } from 'react';
import { backend, type InputError, type InputRequest, type InputSaved } from './lib/backend';
import { FilterableSelect } from './FilterableSelect';
import type { getTranslation } from './i18n/translations';

type T = ReturnType<typeof getTranslation>;

/** Turns whatever the backend rejected with (InputError object or plain string) into a user-facing reason. */
export function inputErrorText(t: T, request: InputRequest, error: unknown): string {
  const messages = t.secureInput.errors;
  let code = '';
  let httpStatus: number | undefined;
  let field: string | undefined;
  if (typeof error === 'string') {
    if (error.includes('auto-locked') || error.includes('locked')) code = 'locked';
    else return `${messages.unknown}: ${error}`;
  } else if (error && typeof error === 'object' && typeof (error as InputError).code === 'string') {
    ({ code, httpStatus, field } = error as InputError);
  } else return messages.unknown;
  const template = messages[code] ?? messages.unknown;
  const label = request.fields.find((item) => item.key === field)?.label ?? field ?? '';
  return template.replace('{status}', String(httpStatus ?? '')).replace('{field}', label);
}

/**
 * Form generated from an AI-supplied template. Typed values live only in this component's state
 * and are discarded when it closes; the backend verifies them and writes the vault.
 */
export function SecureInputModal({ t, request, onSaved, onCancelled }: { t: T; request: InputRequest; onSaved: (saved: InputSaved) => void; onCancelled: () => void }) {
  const [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(request.fields.map((field) => [field.key, field.value])));
  const [models, setModels] = useState<string[]>([]);
  const [error, setError] = useState('');
  const [info, setInfo] = useState('');
  const [busy, setBusy] = useState(false);
  const [fetching, setFetching] = useState(false);

  const roles = new Set(request.fields.map((field) => field.role));
  const canFetchModels = roles.has('api_key') && roles.has('base_url') && roles.has('model');
  const verifies = request.verify.type !== 'none';
  const set = (key: string, value: string) => setValues((current) => ({ ...current, [key]: value }));

  async function fetchModels() {
    setFetching(true); setError(''); setInfo('');
    try {
      const list = await backend.inputFetchModels(request.requestId, values);
      setModels(list);
      setInfo(t.secureInput.modelsFetched.replace('{n}', String(list.length)));
    } catch (reason) { setError(inputErrorText(t, request, reason)); }
    finally { setFetching(false); }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const missing = request.fields.find((field) => field.required && !values[field.key]?.trim());
    if (missing) { setError(t.secureInput.required.replace('{field}', missing.label)); return; }
    setBusy(true); setError(''); setInfo('');
    try { onSaved(await backend.inputSubmit(request.requestId, values)); }
    catch (reason) { setError(inputErrorText(t, request, reason)); setBusy(false); }
  }

  async function cancel() {
    try { await backend.inputCancel(request.requestId); } catch { /* the request is dropped on the next poll if it is gone */ }
    onCancelled();
  }

  const verifyHost = request.verify.host ?? '';
  const verifyNote = request.verify.type === 'login' ? t.secureInput.verifyLogin : request.verify.type === 'llm' ? t.secureInput.verifyLlm : '';

  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={request.title}>
    <form className="secret-form" onSubmit={(event) => void submit(event)} autoComplete="off">
      <div className="form-heading"><div><span className="section-kicker">AI REQUEST / SECURE INPUT</span><h2>{request.title}</h2></div></div>
      {request.hint && <p className="form-intro">{request.hint}</p>}
      {verifyNote && verifyHost && <div className="env-warning">◆ {verifyNote.replace('{host}', verifyHost)}</div>}
      {request.fields.map((field) => {
        const text = `${field.label}${field.required ? ' *' : ''}`;
        if (field.type === 'select') {
          const ids = Array.from(new Set([...field.options, ...(field.role === 'model' ? models : [])]));
          return <div key={field.key}><span className="field-label">{text}</span>
            <FilterableSelect ariaLabel={field.label} value={values[field.key] ?? ''} onChange={(value) => set(field.key, value)} groups={[{ options: ids.map((id) => ({ id, text: id })) }]} placeholder={t.secureInput.modelPlaceholder} />
          </div>;
        }
        return <label key={field.key}>{text}
          <input type={field.type === 'password' ? 'password' : 'text'} autoComplete={field.type === 'password' ? 'new-password' : 'off'} aria-label={field.label} value={values[field.key] ?? ''} onChange={(event) => set(field.key, event.target.value)} />
        </label>;
      })}
      {canFetchModels && <div><button type="button" className="secondary-button" disabled={fetching || busy} onClick={() => void fetchModels()}>{fetching ? t.secureInput.fetching : t.secureInput.fetchModels}</button></div>}
      {info && <div className="form-success" role="status">{info}</div>}
      {error && <div className="form-error" role="alert">{error}</div>}
      <div className="form-actions">
        <button type="button" className="secondary-button" disabled={busy} onClick={() => void cancel()}>{t.secureInput.cancel}</button>
        <button type="submit" className="primary-button" disabled={busy}>{busy ? t.secureInput.saving : verifies ? t.secureInput.saveVerify : t.secureInput.save}</button>
      </div>
    </form>
  </div>;
}

/** Shown while the vault is locked: the form opens by itself right after unlocking. */
export function InputWaitingBanner({ request, t }: { request: InputRequest; t: T }) {
  const title = t.secureInput.waitingTitle.replace('{title}', request.title);
  return <section className="mcp-plan" aria-label={title}><div><span className="section-kicker">MCP / SECURE INPUT</span><strong>{title}</strong><p>{t.secureInput.waitingBody}</p></div></section>;
}
