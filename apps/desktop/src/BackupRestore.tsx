import { useState, type FormEvent } from 'react';
import type { getTranslation } from './i18n/translations';
import { backend } from './lib/backend';

type T = ReturnType<typeof getTranslation>;

/** Maps the stable error codes of the backend to localized text; unknown errors are shown as they are. */
export function restoreErrorText(t: T, error: unknown): string {
  const message = String(error);
  const code = Object.keys(t.tools.restoreErrors).find((candidate) => message.includes(candidate));
  return code ? t.tools.restoreErrors[code] : message;
}

/**
 * Restore dialog shared by the lock screen and the settings page. The password and the chosen
 * path stay in this component; the backend reads the file itself.
 */
export function RestoreDialog({ t, onClose, onRestored }: { t: T; onClose: () => void; onRestored: () => void }) {
  const [path, setPath] = useState('');
  const [password, setPassword] = useState('');
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  async function chooseFile() {
    try {
      const chosen = await backend.chooseBackupFile();
      if (chosen) setPath(chosen);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!path || !password || !confirmed || busy) return;
    setBusy(true);
    setError('');
    try {
      await backend.restoreBackup(path, password);
      setPassword('');
      onRestored();
    } catch (reason) {
      setError(restoreErrorText(t, reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.tools.restoreTitle}>
      <form className="secret-form restore-dialog" onSubmit={(event) => void submit(event)}>
        <div className="form-heading">
          <div>
            <span className="section-kicker">BACKUP / RESTORE</span>
            <h2>{t.tools.restoreTitle}</h2>
          </div>
          <button type="button" className="close-button" onClick={onClose}>×</button>
        </div>
        <div className="restore-file-row">
          <button type="button" className="secondary-button" onClick={() => void chooseFile()}>{t.tools.chooseBackupFile}</button>
          <code className="restore-file-name">{path || t.tools.noFileChosen}</code>
        </div>
        <label>
          {t.tools.backupPassword}
          <input type="password" autoComplete="off" value={password} onChange={(event) => setPassword(event.target.value)} />
        </label>
        <div className="env-warning">◆ {t.tools.restoreNotice}</div>
        <label className="checkbox-label">
          <input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />
          {t.tools.restoreConfirm}
        </label>
        {error && <div className="form-error" role="alert">{error}</div>}
        <div className="form-actions">
          <button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button>
          <button className="primary-button" type="submit" disabled={!path || !password || !confirmed || busy}>
            {busy ? t.tools.restoring : t.tools.restoreSubmit}
          </button>
        </div>
      </form>
    </div>
  );
}
