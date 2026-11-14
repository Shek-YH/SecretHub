import { useEffect, useMemo, useState } from 'react';
import { backend, type CapabilitySuggestionBatch, type PendingPlan, type PlanPreview, type PlanPreviewEntry } from './lib/backend';
import type { getTranslation } from './i18n/translations';

type T = ReturnType<typeof getTranslation>;

// Everything below renders untrusted strings (keys, model ids, paths, secret names, URLs) as plain text only.

function purposeLabel(entry: PlanPreviewEntry, t: T) {
  if (entry.kind === 'api_key') return t.review.purposeKey;
  if (entry.kind === 'base_url') return t.review.purposeBaseUrl;
  if (entry.kind === 'model') return entry.capability ? `${t.review.purposeModel} · ${entry.capability}` : t.review.purposeModel;
  if (entry.kind === 'group_var') return t.review.purposeGroup;
  return t.review.purposeLegacy;
}

function warningLabel(code: string, t: T) {
  if (code === 'model_not_in_synced_list') return t.review.warningModelNotInList;
  if (code === 'models_not_synced') return t.review.warningModelsNotSynced;
  return code;
}

export function PlanReviewModal({ plan, t, onClose, onDone }: { plan: PendingPlan; t: T; onClose: () => void; onDone: () => void }) {
  const [preview, setPreview] = useState<PlanPreview | null>(null);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [result, setResult] = useState<{ written: string[]; skipped: string[] } | null>(null);
  useEffect(() => {
    let cancelled = false;
    backend.planPreview(plan.request_id).then((value) => { if (!cancelled) setPreview(value); }).catch((reason) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, [plan.request_id]);

  const entries = preview?.entries ?? [];
  const errors = preview?.errors ?? [];
  // Only conflicting rows can be skipped, so a stale key can never leak into skip_keys.
  const skipKeys = useMemo(() => entries.filter((entry) => entry.conflict && skipped.has(entry.key)).map((entry) => entry.key), [entries, skipped]);

  function setAction(key: string, skip: boolean) {
    setSkipped((current) => { const next = new Set(current); if (skip) next.add(key); else next.delete(key); return next; });
  }

  async function approve() {
    setBusy(true); setError('');
    try {
      await backend.confirmPlan(plan.request_id, true, skipKeys);
      setResult({ written: entries.map((entry) => entry.key).filter((key) => !skipKeys.includes(key)), skipped: skipKeys });
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function decline() {
    setBusy(true); setError('');
    try { await backend.confirmPlan(plan.request_id, false); onDone(); }
    catch (reason) { setError(String(reason)); setBusy(false); }
  }

  if (result) {
    return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.review.title}><div className="secret-form plan-review-modal"><div className="form-heading"><div><span className="section-kicker">MCP / {t.review.done.toUpperCase()}</span><h2>{t.review.done}</h2></div></div><div className="form-success">{t.review.written}: {result.written.join(', ') || '—'}</div>{result.skipped.length > 0 && <div className="conflict-box">{t.review.skipped}: {result.skipped.join(', ')}</div>}<div className="form-actions"><button type="button" className="primary-button" onClick={onDone}>{t.review.close}</button></div></div></div>;
  }

  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.review.title}><div className="secret-form plan-review-modal">
    <div className="form-heading"><div><span className="section-kicker">MCP / CONFIRMATION REQUIRED</span><h2>{t.review.title}</h2><p className="form-intro">{t.review.project}: <code>{plan.project_path}</code></p></div><button type="button" className="close-button" aria-label={t.review.close} onClick={onClose}>×</button></div>
    {!preview && !error && <div className="model-empty">{t.review.loading}</div>}
    {errors.length > 0 && <div className="form-error" role="alert">{t.review.errors}{errors.map((item) => `${item.code}: ${item.message}`).join('; ')}</div>}
    {preview && errors.length === 0 && entries.length === 0 && <div className="model-empty">{t.review.empty}</div>}
    {entries.length > 0 && <table className="plan-review-table"><thead><tr><th>{t.review.variable}</th><th>{t.review.source}</th><th>{t.review.value}</th><th>{t.review.status}</th><th>{t.review.action}</th></tr></thead><tbody>{entries.map((entry) => {
      const skip = entry.conflict && skipped.has(entry.key);
      return <tr key={entry.key} className={skip ? 'plan-row-skipped' : undefined}>
        <td><code>{entry.key}</code></td>
        <td><strong>{entry.secretName || '—'}</strong><small>{purposeLabel(entry, t)}</small></td>
        <td><code>{entry.display}</code></td>
        <td><span>{entry.conflict ? t.review.statusOverwrite : t.review.statusNew}</span>{entry.warnings.map((code) => <small className="plan-warning" key={code}>{warningLabel(code, t)}</small>)}</td>
        <td>{entry.conflict ? <select aria-label={`${t.review.action} ${entry.key}`} value={skip ? 'skip' : 'overwrite'} onChange={(event) => setAction(entry.key, event.target.value === 'skip')}><option value="overwrite">{t.review.actionOverwrite}</option><option value="skip">{t.review.actionSkip}</option></select> : null}</td>
      </tr>;
    })}</tbody></table>}
    {error && <div className="form-error" role="alert">{t.review.failed}: {error}</div>}
    <div className="form-security"><span>◆</span>{t.security.warning}</div>
    <div className="form-actions"><button type="button" className="secondary-button" disabled={busy} onClick={() => void decline()}>{t.review.decline}</button><button type="button" className="primary-button" disabled={busy || !preview || errors.length > 0 || entries.length === 0} onClick={() => void approve()}>{t.review.approve}</button></div>
  </div></div>;
}

export function CapabilitySuggestionBanner({ batch, t, onReview }: { batch: CapabilitySuggestionBatch; t: T; onReview: () => void }) {
  return <section className="mcp-plan" aria-label={t.capsReview.title}><div><span className="section-kicker">MCP / CAPABILITY SUGGESTIONS</span><strong>{t.capsReview.notice.replace('{n}', String(batch.suggestions.length))}</strong><small>{batch.providerId}</small></div><div className="mcp-plan-actions"><button className="primary-button" onClick={onReview}>{t.capsReview.open}</button></div></section>;
}

export function CapabilitySuggestionModal({ batch, t, onClose, onDone }: { batch: CapabilitySuggestionBatch; t: T; onClose: () => void; onDone: () => void }) {
  const [checked, setChecked] = useState<Set<string>>(() => new Set(batch.suggestions.map((item) => item.modelId)));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  function toggle(modelId: string) {
    setChecked((current) => { const next = new Set(current); if (next.has(modelId)) next.delete(modelId); else next.add(modelId); return next; });
  }
  async function resolve(accepted: string[]) {
    setBusy(true); setError('');
    try { await backend.resolveCapabilitySuggestions(batch.requestId, accepted); onDone(); }
    catch (reason) { setError(String(reason)); setBusy(false); }
  }
  async function openEvidence(url: string) {
    try { await backend.openExternalUrl(url); } catch (reason) { setError(String(reason)); }
  }
  const accepted = batch.suggestions.map((item) => item.modelId).filter((modelId) => checked.has(modelId));
  const tags = (items: string[]) => items.length === 0 ? <small>{t.capsReview.none}</small> : items.map((cap) => <span className="tag" key={cap}>{cap}</span>);
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.capsReview.title}><div className="secret-form plan-review-modal">
    <div className="form-heading"><div><span className="section-kicker">MCP / CAPABILITY SUGGESTIONS</span><h2>{t.capsReview.title}</h2><p className="form-intro">{t.capsReview.provider}: <code>{batch.providerId}</code></p></div><button type="button" className="close-button" aria-label={t.capsReview.close} onClick={onClose}>×</button></div>
    <table className="plan-review-table"><thead><tr><th /><th>{t.capsReview.model}</th><th>{t.capsReview.suggested}</th><th>{t.capsReview.current}</th><th>{t.capsReview.evidence}</th></tr></thead><tbody>{batch.suggestions.map((item) => <tr key={item.modelId}>
      <td><input type="checkbox" aria-label={item.modelId} checked={checked.has(item.modelId)} onChange={() => toggle(item.modelId)} /></td>
      <td><code>{item.modelId}</code></td>
      <td>{tags(item.capabilities)}</td>
      <td>{item.current ? <>{tags(item.current.capabilities)}<small className="model-source">{item.current.source}</small></> : <small>—</small>}</td>
      <td>{item.evidenceUrl ? <><code className="plan-evidence">{item.evidenceUrl}</code>{item.evidenceUrl.startsWith('https://') && <button type="button" className="secondary-button" aria-label={`${t.capsReview.openEvidence} ${item.modelId}`} onClick={() => void openEvidence(item.evidenceUrl as string)}>{t.capsReview.openEvidence}</button>}</> : <small>—</small>}</td>
    </tr>)}</tbody></table>
    {error && <div className="form-error" role="alert">{t.capsReview.failed}: {error}</div>}
    <div className="form-actions"><button type="button" className="secondary-button" disabled={busy} onClick={() => void resolve([])}>{t.capsReview.rejectAll}</button><button type="button" className="primary-button" disabled={busy || accepted.length === 0} onClick={() => void resolve(accepted)}>{t.capsReview.accept}</button></div>
  </div></div>;
}
