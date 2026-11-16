import { useEffect, useState } from 'react';
import { backend, isTauriRuntime, type CatalogCheckResult, type CatalogStatus } from './lib/backend';
import type { getTranslation, Locale } from './i18n/translations';

type Translation = ReturnType<typeof getTranslation>;

function resultText(t: Translation, result: CatalogCheckResult['result'], detail: string): string {
  switch (result) {
    case 'updated': return t.catalog.updated;
    case 'up_to_date': return t.catalog.upToDate;
    case 'not_configured': return t.catalog.notConfigured;
    case 'failed': return detail ? `${t.catalog.failed}: ${detail}` : t.catalog.failed;
    default: return '';
  }
}

/** Settings block for the remote model catalog. All strings from the backend render as plain text. */
export function CatalogSettings({ t, locale, onUpdated }: { t: Translation; locale: Locale; onUpdated?: () => void }) {
  const [status, setStatus] = useState<CatalogStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    backend.catalogStatus().then((next) => { if (!cancelled) setStatus(next); }).catch((reason) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, []);
  if (!isTauriRuntime()) return <section className="catalog-settings" aria-label={t.catalog.title}><h3>{t.catalog.title}</h3><div className="model-empty">{t.catalog.desktopOnly}</div></section>;
  async function toggle(enabled: boolean) {
    try { setStatus(await backend.catalogSetAutoUpdate(enabled)); setError(''); } catch (reason) { setError(String(reason)); }
  }
  async function checkNow() {
    setChecking(true);
    setMessage('');
    try {
      const outcome = await backend.catalogCheckNow();
      setStatus(outcome.status);
      setMessage(resultText(t, outcome.result, outcome.detail));
      setError('');
      if (outcome.result === 'updated') onUpdated?.();
    } catch (reason) { setError(String(reason)); }
    finally { setChecking(false); }
  }
  const published = status?.publishedAt ? new Date(status.publishedAt).toLocaleString(locale) : '—';
  const last = status?.lastCheck;
  return <section className="catalog-settings" aria-label={t.catalog.title}>
    <h3>{t.catalog.title}</h3>
    <div className="setting-row"><span>{t.catalog.serial}</span><strong>{status ? status.serial : '—'}</strong></div>
    <div className="setting-row"><span>{t.catalog.publishedAt}</span><strong>{published}</strong></div>
    <div className="setting-row"><span>{t.catalog.source}</span><strong>{status?.source === 'remote' ? t.catalog.sourceRemote : t.catalog.sourceBundled}</strong></div>
    <div className="setting-row"><span>{t.catalog.url}</span><strong>{status?.urlConfigured ? t.catalog.urlConfigured : t.catalog.urlMissing}</strong></div>
    <label className="checkbox-label"><input type="checkbox" checked={status?.autoUpdate ?? true} disabled={!status} onChange={(event) => void toggle(event.target.checked)} />{t.catalog.autoUpdate}</label>
    {last && <div className="setting-row"><span>{t.catalog.lastCheck}</span><strong>{new Date(last.at * 1000).toLocaleString(locale)} · {resultText(t, last.result, last.detail)}</strong></div>}
    <div className="settings-footer"><button className="secondary-button" disabled={checking} onClick={() => void checkNow()}>{checking ? t.catalog.checking : t.catalog.checkNow}</button>{message && <span className="form-success" role="status">{message}</span>}{error && <span className="form-error" role="alert">{error}</span>}</div>
  </section>;
}
