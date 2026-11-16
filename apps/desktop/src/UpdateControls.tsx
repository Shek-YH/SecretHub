import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { backend, isTauriRuntime, type UpdateCheckResult } from './lib/backend';
import type { getTranslation } from './i18n/translations';

type Translation = ReturnType<typeof getTranslation>;

/** Top-bar "Update" button: shown once a new version has been downloaded silently in the background. */
export function UpdateButton({ t }: { t: Translation }) {
  const [version, setVersion] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    backend.updateStatus().then((status) => { if (!cancelled && status.readyVersion) setVersion(status.readyVersion); }).catch(() => undefined);
    listen<string>('update-ready', (event) => { if (!cancelled) setVersion(event.payload); }).then((fn) => { if (cancelled) fn(); else unlisten = fn; }).catch(() => undefined);
    return () => { cancelled = true; unlisten?.(); };
  }, []);
  if (!version) return null;
  async function install() {
    setInstalling(true);
    try { await backend.updateInstall(); } catch { setInstalling(false); }
  }
  return <button className="primary-button update-button" disabled={installing} onClick={() => void install()} title={t.update.buttonTitle.replace('{version}', version)}>{installing ? t.update.installing : t.update.button}</button>;
}

function resultText(t: Translation, result: UpdateCheckResult): string {
  switch (result.result) {
    case 'up_to_date': return t.update.upToDate;
    case 'ready': return t.update.ready.replace('{version}', result.version ?? '');
    case 'busy': return t.update.downloading;
    default: return result.detail ? `${t.update.failed}: ${result.detail}` : t.update.failed;
  }
}

/** Settings block with the manual "check for updates" entry. */
export function UpdateSettings({ t }: { t: Translation }) {
  const [current, setCurrent] = useState('');
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState('');
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    backend.updateStatus().then((status) => { if (!cancelled) setCurrent(status.currentVersion); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, []);
  if (!isTauriRuntime()) return <section className="catalog-settings" aria-label={t.update.title}><h3>{t.update.title}</h3><div className="model-empty">{t.update.desktopOnly}</div></section>;
  async function check() {
    setChecking(true);
    setMessage(t.update.checking);
    try { setMessage(resultText(t, await backend.updateCheckNow())); }
    catch (reason) { setMessage(`${t.update.failed}: ${String(reason)}`); }
    finally { setChecking(false); }
  }
  return <section className="catalog-settings" aria-label={t.update.title}>
    <h3>{t.update.title}</h3>
    <div className="setting-row"><span>{t.update.currentVersion}</span><strong>{current || '—'}</strong></div>
    <div className="settings-footer"><button className="secondary-button" disabled={checking} onClick={() => void check()}>{t.update.checkNow}</button>{message && <span className="form-success" role="status">{message}</span>}</div>
  </section>;
}
