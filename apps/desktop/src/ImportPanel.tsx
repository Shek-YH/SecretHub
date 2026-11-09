import { useState, type FormEvent } from 'react';
import type { getTranslation } from './i18n/translations';
import { backend, isTauriRuntime, type ImportPreviewRow } from './lib/backend';

type T = ReturnType<typeof getTranslation>;

/** Demo/test mode only: the desktop app groups variables in the backend. */
function demoRows(format: 'env' | 'json', contents: string): ImportPreviewRow[] {
  const keys = format === 'env' ? contents.split(/\r?\n/).map((line) => line.split('=')[0].trim()).filter(Boolean) : Object.keys(JSON.parse(contents));
  return keys.map((key) => ({ label: '', generic: true, keys: [key] }));
}

export function ImportPanel({ t, onClose, onDone }: { t: T; onClose: () => void; onDone: () => void }) {
  const [format, setFormat] = useState<'env' | 'json'>('env');
  const [contents, setContents] = useState('');
  const [filePath, setFilePath] = useState('');
  const [rows, setRows] = useState<ImportPreviewRow[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');

  async function chooseFile() {
    try {
      const chosen = isTauriRuntime() ? await backend.chooseImportFile() : null;
      if (!chosen) return;
      setFilePath(chosen);
      setContents('');
      setRows(await backend.importFilePreview(chosen));
      setError('');
    } catch (reason) {
      setError(String(reason));
      setRows([]);
    }
  }

  async function previewText() {
    try {
      setFilePath('');
      setRows(isTauriRuntime() ? await backend.importPreview({ format, contents, confirmed: false }) : demoRows(format, contents));
      setError('');
    } catch (reason) {
      setError(String(reason));
      setRows([]);
    }
  }

  async function confirm() {
    setBusy(true);
    try {
      let apiKeyIds: string[] = [];
      if (isTauriRuntime()) {
        const outcome = filePath ? await backend.importFile(filePath) : await backend.importSecrets({ format, contents, confirmed: true });
        apiKeyIds = outcome.apiKeyIds;
      }
      setError('');
      setMessage(apiKeyIds.length > 0 ? t.tools.importValidating : t.actions.imported);
      // Newly created API keys are checked one by one so their status and model list are ready.
      let failed = false;
      for (const id of apiKeyIds) {
        try { await backend.validate(id); } catch { failed = true; }
      }
      if (apiKeyIds.length > 0) setMessage(failed ? t.tools.importFailed : t.tools.importValidated);
      setTimeout(onDone, 500);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  const describe = (row: ImportPreviewRow) => `${row.generic ? t.tools.importGeneric : row.label} ← ${row.keys.join(', ')}`;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.actions.import}>
      <form className="secret-form import-form" onSubmit={(event: FormEvent) => { event.preventDefault(); void previewText(); }}>
        <div className="form-heading">
          <div>
            <span className="section-kicker">IMPORT / ENCRYPTED</span>
            <h2>{t.actions.import}</h2>
          </div>
          <button type="button" className="close-button" onClick={onClose}>×</button>
        </div>
        <div className="import-file-row">
          <button type="button" className="primary-button" onClick={() => void chooseFile()}>{t.tools.importChooseFile}</button>
          {filePath && <code className="restore-file-name">{t.tools.importChosen}: {filePath}</code>}
        </div>
        <strong className="import-paste-title">{t.tools.importPasteTitle}</strong>
        <label>
          {t.actions.importFormat}
          <select value={format} onChange={(event) => setFormat(event.target.value as 'env' | 'json')}>
            <option value="env">.env</option>
            <option value="json">JSON</option>
          </select>
        </label>
        <label>
          {t.actions.importContent}
          <textarea className="import-textarea" rows={6} value={contents} onChange={(event) => { setContents(event.target.value); setFilePath(''); }} />
        </label>
        {rows.length > 0 && <div className="preview-box">{rows.map((row, index) => <div key={`${index}-${row.keys.join('|')}`}>{describe(row)}</div>)}</div>}
        {error && <div className="form-error" role="alert">{error}</div>}
        {message && <div className="form-success">{message}</div>}
        <div className="form-security"><span>◆</span>{t.security.warning}</div>
        <div className="form-actions">
          <button type="button" className="secondary-button" onClick={() => void previewText()}>{t.actions.importPreview}</button>
          <button type="button" className="primary-button" disabled={rows.length === 0 || busy} onClick={() => void confirm()}>{t.actions.confirmImport}</button>
        </div>
      </form>
    </div>
  );
}
