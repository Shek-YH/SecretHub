import { useState, type FormEvent } from 'react';
import type { getTranslation } from '../i18n/translations';
import { backend, isTauriRuntime } from '../lib/backend';

const MIN_LENGTH = 9;
const RECOMMENDED_LENGTH = 12;

/** Settings > Security > Change master password. The backend re-encrypts everything in one transaction. */
export function ChangePasswordDialog({ t, onClose }: { t: ReturnType<typeof getTranslation>; onClose: () => void }) {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!current) { setError(t.form.required); return; }
    if (next.length < MIN_LENGTH) { setError(t.auth.minLength); return; }
    if (next !== confirmation) { setError(t.auth.mismatch); return; }
    setBusy(true);
    try {
      if (isTauriRuntime()) await backend.changePassword(current, next);
      setCurrent(''); setNext(''); setConfirmation('');
      setError('');
      setDone(true);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.items.changePassword}>
      <form className="secret-form change-password-form" onSubmit={(event) => void submit(event)} noValidate>
        <div className="form-heading">
          <div>
            <span className="section-kicker">SECURITY / MASTER PASSWORD</span>
            <h2>{t.items.changePassword}</h2>
            <p className="form-intro">{t.items.passwordHint}</p>
          </div>
          <button type="button" className="close-button" onClick={onClose} aria-label={t.form.cancel}>×</button>
        </div>
        {done ? (
          <>
            <div className="form-success" role="status">{t.items.passwordChanged}</div>
            <div className="form-actions"><button type="button" className="primary-button" onClick={onClose}>{t.items.close}</button></div>
          </>
        ) : (
          <>
            <label>{t.items.currentPassword}<input autoFocus type="password" value={current} onChange={(event) => setCurrent(event.target.value)} autoComplete="current-password" /></label>
            <label>{t.items.newPassword}<input type="password" value={next} onChange={(event) => setNext(event.target.value)} autoComplete="new-password" /></label>
            {next.length >= MIN_LENGTH && next.length < RECOMMENDED_LENGTH && <p className="cap-hint" role="status">{t.items.recommendLonger}</p>}
            <label>{t.items.confirmNewPassword}<input type="password" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} autoComplete="new-password" /></label>
            {error && <div className="form-error" role="alert">{error}</div>}
            <div className="form-actions">
              <button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button>
              <button className="primary-button" type="submit" disabled={busy}>{busy ? t.items.changing : t.items.changeSubmit}</button>
            </div>
          </>
        )}
      </form>
    </div>
  );
}
