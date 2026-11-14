import { useState, type FormEvent } from 'react';
import type { getTranslation } from './i18n/translations';
import { backend, isTauriRuntime, type ImportApplyOutcome, type ImportPreviewRow, type ImportReview } from './lib/backend';
import { ImportReviewModal } from './ImportReview';

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
  const [review, setReview] = useState<ImportReview | null>(null);

  /** Folder import: the backend reads the folder locally and returns a value-free review. */
  async function chooseFolder() {
    try {
      const folder = isTauriRuntime() ? await backend.chooseFolder() : null;
      if (!folder) return;
      setMessage(t.importReview.scanning);
      setReview(await backend.importScan([folder]));
      setMessage('');
      setError('');
    } catch (reason) {
      setMessage('');
      setError(String(reason));
    }
  }

  /** Files for the review flow: password exports, the bank card template, keys. */
  async function chooseFiles() {
    try {
      const files = isTauriRuntime() ? await backend.importChooseFiles() : null;
      if (!files || files.length === 0) return;
      setMessage(t.importReview.scanning);
      setReview(await backend.importScan(files));
      setMessage('');
      setError('');
    } catch (reason) {
      setMessage('');
      setError(String(reason));
    }
  }

  async function saveTemplate() {
    try {
      const saved = isTauriRuntime() ? await backend.importSaveBankTemplate() : null;
      if (saved) { setMessage(t.importReview.templateSaved); setError(''); }
    } catch (reason) { setError(String(reason)); }
  }

  /** Smart paste: recognized locally, then reviewed like any other import. */
  async function smartPaste() {
    try {
      if (!isTauriRuntime()) return;
      setFilePath('');
      setMessage(t.importReview.scanning);
      setReview(await backend.importScanText(contents));
      setMessage('');
      setError('');
    } catch (reason) {
      setMessage('');
      setError(String(reason));
    }
  }

  async function reviewed(outcome: ImportApplyOutcome) {
    setReview(null);
    // Newly created API keys are checked one by one so their status and model list are ready.
    for (const id of outcome.apiKeyIds) {
      try { await backend.validate(id); } catch { /* the key stays unchecked; the list shows it */ }
    }
    onDone();
  }

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
          <button type="button" className="secondary-button" onClick={() => void chooseFolder()}>{t.importReview.chooseFolder}</button>
          <button type="button" className="secondary-button" onClick={() => void chooseFiles()}>{t.importReview.chooseFiles}</button>
          <button type="button" className="secondary-button" onClick={() => void saveTemplate()}>{t.importReview.bankTemplate}</button>
          {filePath && <code className="restore-file-name">{t.tools.importChosen}: {filePath}</code>}
        </div>
        <p className="cap-hint">{t.importReview.bankTemplateHint}</p>
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
          <button type="button" className="secondary-button" disabled={!contents.trim()} title={t.importReview.smartPasteHint} onClick={() => void smartPaste()}>{t.importReview.smartPaste}</button>
          <button type="button" className="secondary-button" onClick={() => void previewText()}>{t.actions.importPreview}</button>
          <button type="button" className="primary-button" disabled={rows.length === 0 || busy} onClick={() => void confirm()}>{t.actions.confirmImport}</button>
        </div>
      </form>
      {review && <ImportReviewModal t={t} review={review} onClose={() => setReview(null)} onDone={(outcome) => void reviewed(outcome)} />}
    </div>
  );
}
