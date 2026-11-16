import { useEffect, useState } from 'react';
import { backend, isTauriRuntime, type ConnectItem, type ConnectStatus } from './lib/backend';
import type { getTranslation } from './i18n/translations';

type Translation = ReturnType<typeof getTranslation>;

function itemLabel(t: Translation, id: string): string {
  if (id === 'claude-code') return t.aiConnect.claudeCode;
  if (id === 'claude-desktop') return t.aiConnect.claudeDesktop;
  if (id === 'cursor') return t.aiConnect.cursor;
  if (id === 'sidecar') return t.aiConnect.sidecar;
  if (id.startsWith('skill:')) return `${t.aiConnect.skills} · ${id.slice(6)}`;
  return id;
}

/** Settings block that connects the bundled MCP server and skills to the user's AI tools. */
export function AiConnectSettings({ t }: { t: Translation }) {
  const [status, setStatus] = useState<ConnectStatus | null>(null);
  const [items, setItems] = useState<ConnectItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    backend.aiConnectStatus().then((next) => { if (!cancelled) setStatus(next); }).catch((reason) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, []);
  if (!isTauriRuntime()) return <section className="catalog-settings" aria-label={t.aiConnect.title}><h3>{t.aiConnect.title}</h3><div className="model-empty">{t.aiConnect.desktopOnly}</div></section>;
  async function connect() {
    setBusy(true);
    setError('');
    try {
      setItems(await backend.aiConnect());
      setStatus(await backend.aiConnectStatus());
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  const statusText = (value: ConnectItem['status']) => value === 'connected' ? t.aiConnect.connected : value === 'skipped' ? t.aiConnect.skipped : t.aiConnect.failed;
  return <section className="catalog-settings" aria-label={t.aiConnect.title}>
    <h3>{t.aiConnect.title}</h3>
    <p className="cap-hint">{t.aiConnect.hint}</p>
    <div className="setting-row"><span>{status?.connectedVersion ? t.aiConnect.connectedAt.replace('{version}', status.connectedVersion) : t.aiConnect.notConnected}</span>{status && !status.sidecarFound && <strong>{t.aiConnect.sidecarMissing}</strong>}</div>
    {items.map((item) => <div className="setting-row" key={item.id}><span>{itemLabel(t, item.id)}</span><strong className={item.status === 'failed' ? 'form-error' : undefined}>{statusText(item.status)}{item.detail ? ` · ${item.detail}` : ''}</strong></div>)}
    <div className="settings-footer"><button className="secondary-button" disabled={busy} onClick={() => void connect()}>{busy ? t.aiConnect.connecting : t.aiConnect.button}</button>{error && <span className="form-error" role="alert">{error}</span>}</div>
  </section>;
}
