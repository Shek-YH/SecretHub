import type { getTranslation } from './i18n/translations';
import type { PendingSyncRequest } from './lib/backend';

type T = ReturnType<typeof getTranslation>;

// Banners for AI-initiated requests that Desktop runs without a confirmation dialog.
// Everything shown is a count or an id; ids render as text only.

/** Shown while the vault is locked: the sync request stays queued and runs right after unlocking. */
export function SyncRequestBanner({ request, t }: { request: PendingSyncRequest; t: T }) {
  return <section className="mcp-plan" aria-label={t.ai.syncTitle}><div><span className="section-kicker">MCP / MODEL SYNC</span><strong>{t.ai.syncTitle}</strong><p>{t.ai.syncBody}</p>{request.secretCount != null && <small>{request.secretCount} {t.nav.secrets}</small>}</div></section>;
}
