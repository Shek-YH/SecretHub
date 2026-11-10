import type { getTranslation } from './i18n/translations';
import type { AiImportNotice, CaptureRequest, MediaRequest, PendingImportRequest, PendingOrganizeRequest, PendingSyncRequest } from './lib/backend';
import { formatBytes } from './mediaFormat';

type T = ReturnType<typeof getTranslation>;

// Banners for AI-initiated requests that Desktop runs without a confirmation dialog.
// Everything shown is a count or an id; ids render as text only.

/** An AI asks to add a file to the media library; the full path and size are shown before anything is copied. */
export function MediaRequestBanner({ request, t, onAccept, onDecline }: { request: MediaRequest; t: T; onAccept: () => void; onDecline: () => void }) {
  return <section className="mcp-plan" aria-label={t.media.requestTitle}><div><span className="section-kicker">MCP / MEDIA</span><strong>{t.media.requestTitle}</strong><p>{t.media.requestBody}</p><div className="media-request-file"><strong>{request.filename}</strong>{request.sizeBytes != null && <small>{t.media.requestSize}: {formatBytes(request.sizeBytes)}</small>}<small>{t.media.requestPath}: <code>{request.path}</code></small></div>{request.description && <small>{request.description}</small>}{request.tags.length > 0 && <div className="tag-row">{request.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div>}{request.problem && <div className="form-error" role="alert">{t.media.problems[request.problem] ?? request.problem}</div>}</div><div className="mcp-plan-actions"><button className="primary-button" disabled={Boolean(request.problem)} onClick={onAccept}>{t.media.accept}</button><button className="secondary-button" onClick={onDecline}>{t.media.decline}</button></div></section>;
}

/** An AI asks for a key capture; nothing is read from the clipboard until the user clicks Start. */
export function CaptureRequestBanner({ request, t, onStart, onDecline }: { request: CaptureRequest; t: T; onStart: () => void; onDecline: () => void }) {
  return <section className="mcp-plan" aria-label={t.capture.title}><div><span className="section-kicker">MCP / KEY CAPTURE</span><strong>{t.capture.requestTitle.replace('{provider}', request.providerName)}</strong><p>{t.capture.requestBody}</p></div><div className="mcp-plan-actions"><button className="primary-button" onClick={onStart}>{t.capture.start}</button><button className="secondary-button" onClick={onDecline}>{t.capture.decline}</button></div></section>;
}

/** Shown while the vault is locked: the sync request stays queued and runs right after unlocking. */
export function SyncRequestBanner({ request, t }: { request: PendingSyncRequest; t: T }) {
  return <section className="mcp-plan" aria-label={t.ai.syncTitle}><div><span className="section-kicker">MCP / MODEL SYNC</span><strong>{t.ai.syncTitle}</strong><p>{t.ai.syncBody}</p>{request.secretCount != null && <small>{request.secretCount} {t.nav.secrets}</small>}</div></section>;
}

/** Shown while the vault is locked: an AI import plan stays queued and is written right after unlocking. */
export function OrganizeWaitingBanner({ request, t }: { request: PendingOrganizeRequest; t: T }) {
  return <section className="mcp-plan" aria-label={t.ai.organizeWaitingTitle}><div><span className="section-kicker">MCP / IMPORT</span><strong>{t.ai.organizeWaitingTitle}</strong><p>{t.ai.organizeWaitingBody}</p><small>{request.itemCount} {t.nav.secrets}</small></div></section>;
}

/** An AI import was written automatically: counts only, with a way to look at it or to undo exactly that batch. */
export function AiImportNoticeBanner({ notice, t, onView, onUndo, onDismiss }: { notice: AiImportNotice; t: T; onView: () => void; onUndo: () => void; onDismiss: () => void }) {
  const total = notice.createdGroups + notice.addedVars + notice.savedSeparately + notice.otherItems;
  const title = t.ai.noticeTitle.replace('{total}', String(total)).replace('{groups}', String(notice.createdGroups)).replace('{vars}', String(notice.addedVars)).replace('{separate}', String(notice.savedSeparately));
  return <section className="mcp-plan" aria-label={title}><div><span className="section-kicker">MCP / IMPORT</span><strong>{title}</strong></div><div className="mcp-plan-actions"><button className="primary-button" onClick={onView}>{t.ai.noticeView}</button><button className="secondary-button" onClick={onUndo}>{t.ai.noticeUndo}</button><button className="secondary-button" onClick={onDismiss}>{t.ai.noticeDismiss}</button></div></section>;
}

/** An AI asks to import files by path only; nothing is read until the user clicks View. */
export function ImportRequestBanner({ request, t, onView, onDecline }: { request: PendingImportRequest; t: T; onView: () => void; onDecline: () => void }) {
  return <section className="mcp-plan" aria-label={t.importReview.aiRequest.replace('{paths}', '')}><div><span className="section-kicker">MCP / IMPORT</span><strong>{t.importReview.aiRequest.replace('{paths}', '')}</strong><div className="import-request-paths">{request.paths.map((path) => <code key={path}>{path}</code>)}</div><p>{t.importReview.aiBody}</p></div><div className="mcp-plan-actions"><button className="primary-button" onClick={onView}>{t.importReview.view}</button><button className="secondary-button" onClick={onDecline}>{t.importReview.decline}</button></div></section>;
}
