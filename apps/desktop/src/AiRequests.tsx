import type { getTranslation } from './i18n/translations';
import type { CaptureRequest, PendingSyncRequest } from './lib/backend';

type T = ReturnType<typeof getTranslation>;

// Banners for AI-initiated requests that Desktop runs without a confirmation dialog.
// Everything shown is a count or an id; ids render as text only.

/** An AI asks for a key capture; nothing is read from the clipboard until the user clicks Start. */
export function CaptureRequestBanner({ request, t, onStart, onDecline }: { request: CaptureRequest; t: T; onStart: () => void; onDecline: () => void }) {
  return <section className="mcp-plan" aria-label={t.capture.title}><div><span className="section-kicker">MCP / KEY CAPTURE</span><strong>{t.capture.requestTitle.replace('{provider}', request.providerName)}</strong><p>{t.capture.requestBody}</p></div><div className="mcp-plan-actions"><button className="primary-button" onClick={onStart}>{t.capture.start}</button><button className="secondary-button" onClick={onDecline}>{t.capture.decline}</button></div></section>;
}

/** Shown while the vault is locked: the sync request stays queued and runs right after unlocking. */
export function SyncRequestBanner({ request, t }: { request: PendingSyncRequest; t: T }) {
  return <section className="mcp-plan" aria-label={t.ai.syncTitle}><div><span className="section-kicker">MCP / MODEL SYNC</span><strong>{t.ai.syncTitle}</strong><p>{t.ai.syncBody}</p>{request.secretCount != null && <small>{request.secretCount} {t.nav.secrets}</small>}</div></section>;
}
