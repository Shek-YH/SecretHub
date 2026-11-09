import { useMemo, useState } from 'react';
import type { getTranslation } from './i18n/translations';
import { backend, type ImportApplyOutcome, type ImportItemDecision, type ImportReview as ReviewData, type ImportReviewItem } from './lib/backend';

type T = ReturnType<typeof getTranslation>;

type Choice = { include: boolean; action?: string; split: boolean; picks: Record<string, number>; secrets: Record<string, boolean> };

const initialChoice = (item: ImportReviewItem): Choice => ({ include: item.dedupe.state !== 'same', split: true, picks: {}, secrets: {} });

/** How an item ends up given the user's choices: add, update or skip. Mirrors the backend rules. */
export function outcomeOf(item: ImportReviewItem, choice: Choice): 'add' | 'update' | 'skip' | 'none' {
  if (!choice.include) return 'skip';
  if (item.kind === 'api_key' && !choice.split) return 'none';
  const { state } = item.dedupe;
  if (state === 'new') return 'add';
  if (state === 'same') return 'skip';
  const action = choice.action && item.dedupe.options.includes(choice.action) ? choice.action : item.dedupe.default;
  if (action === 'merge' || action === 'overwrite' || action === 'update') return 'update';
  if (action === 'copy' || action === 'keep_both') return 'add';
  return 'skip';
}

export function summarize(review: ReviewData, choices: Record<string, Choice>): { added: number; updated: number; skipped: number } {
  const total = { added: 0, updated: 0, skipped: 0 };
  for (const item of review.items) {
    const outcome = outcomeOf(item, choices[item.id] ?? initialChoice(item));
    if (outcome === 'add') total.added += 1;
    else if (outcome === 'update') total.updated += 1;
    else if (outcome === 'skip') total.skipped += 1;
  }
  return total;
}

/**
 * Review of a local import: items grouped by source folder or file, each with its type, name,
 * variable names and secret flags, and the choices for duplicates. Nothing here is a value.
 */
export function ImportReviewModal({ t, review, onClose, onDone }: { t: T; review: ReviewData; onClose: () => void; onDone: (outcome: ImportApplyOutcome) => void }) {
  const [choices, setChoices] = useState<Record<string, Choice>>(() => Object.fromEntries(review.items.map((item) => [item.id, initialChoice(item)])));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const sources = useMemo(() => Array.from(new Set(review.items.map((item) => item.source))), [review.items]);
  const totals = summarize(review, choices);
  const text = t.importReview;
  const fill = (template: string, values: Record<string, string | number>) => Object.entries(values).reduce((result, [key, value]) => result.replace(`{${key}}`, String(value)), template);

  const patch = (id: string, change: Partial<Choice>) => setChoices((current) => ({ ...current, [id]: { ...current[id], ...change } }));

  async function confirm() {
    setBusy(true);
    setError('');
    const items: ImportItemDecision[] = review.items.map((item) => {
      const choice = choices[item.id];
      const decision: ImportItemDecision = { id: item.id, include: choice.include };
      if (choice.action) decision.action = choice.action;
      if (item.kind === 'api_key') decision.split = choice.split;
      if (Object.keys(choice.picks).length > 0) decision.picks = choice.picks;
      if (Object.keys(choice.secrets).length > 0) decision.secrets = choice.secrets;
      return decision;
    });
    try {
      const outcome = await backend.importApply(review.sessionId, { items });
      setMessage(fill(text.done, { a: outcome.added.length, u: outcome.updated.length, s: outcome.skipped.length }));
      window.setTimeout(() => onDone(outcome), 600);
    } catch (reason) {
      setError(String(reason));
      setBusy(false);
    }
  }

  async function cancel() {
    // The scan is only kept in memory; closing drops it. An AI request stays pending for later.
    try { await backend.importCancel(review.sessionId); } catch { /* the session may already be gone */ }
    onClose();
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={text.title}>
      <div className="secret-form import-review">
        <div className="form-heading">
          <div><span className="section-kicker">IMPORT / REVIEW</span><h2>{text.title}</h2><p className="form-intro">{text.intro}</p></div>
          <button type="button" className="close-button" onClick={() => void cancel()} aria-label={t.form.cancel}>×</button>
        </div>
        {review.items.length === 0 && <p className="cap-hint">{text.empty}</p>}
        {sources.map((source) => (
          <section className="import-source" key={source} aria-label={source}>
            <h3>{source}</h3>
            {review.items.filter((item) => item.source === source).map((item) => {
              const choice = choices[item.id];
              const parent = item.splitFrom ? review.items.find((other) => other.id === item.splitFrom) : undefined;
              return (
                <div className="import-item" key={item.id} data-item={item.id} data-kind={item.kind}>
                  <div className="import-item-head">
                    <label className="checkbox-label">
                      <input type="checkbox" checked={choice.include} aria-label={`${text.include} ${item.name}`} onChange={(event) => patch(item.id, { include: event.target.checked })} />
                      <strong>{item.name}</strong>
                    </label>
                    <span className="tag">{text.kinds[item.kind] ?? item.kind}</span>
                    {item.provider && <span className="tag">{item.provider}</span>}
                    <span className={`tag import-state import-state-${item.dedupe.state}`}>{text.states[item.dedupe.state]}</span>
                    {item.hasNotes && <span className="tag">{text.hasNotes}</span>}
                  </div>
                  {parent && <small>{fill(text.splitFrom, { name: parent.name })}</small>}
                  {item.kind === 'api_key' && (
                    <select aria-label={`${item.name} ${text.splitOption}`} value={choice.split ? 'split' : 'keep'} onChange={(event) => patch(item.id, { split: event.target.value === 'split' })}>
                      <option value="split">{text.splitOption}</option>
                      <option value="keep">{text.keepOption}</option>
                    </select>
                  )}
                  {item.dedupe.state === 'differs' && (
                    <>
                      <select aria-label={`${item.name} ${text.states.differs}`} value={choice.action ?? item.dedupe.default} onChange={(event) => patch(item.id, { action: event.target.value })}>
                        {item.dedupe.options.map((option) => <option key={option} value={option}>{text.actions[option] ?? option}</option>)}
                      </select>
                      {item.dedupe.added.length > 0 && <small>{text.added}: {item.dedupe.added.join(', ')}</small>}
                      {item.dedupe.changed.length > 0 && <small>{text.changed}: {item.dedupe.changed.join(', ')}</small>}
                      {item.dedupe.missing.length > 0 && <small>{text.missing}: {item.dedupe.missing.join(', ')}</small>}
                    </>
                  )}
                  <ul className="import-variables">
                    {item.variables.map((variable) => {
                      const secret = choice.secrets[variable.name] ?? variable.secret;
                      return (
                        <li key={variable.name} data-variable={variable.name}>
                          <code>{variable.name}</code>
                          {item.kind === 'group' && (
                            <label className="checkbox-label">
                              <input type="checkbox" checked={secret} aria-label={`${text.secret} ${variable.name}`} onChange={(event) => patch(item.id, { secrets: { ...choice.secrets, [variable.name]: event.target.checked } })} />
                              {secret ? text.secret : text.plain}
                            </label>
                          )}
                          {item.kind !== 'group' && <small>{variable.secret ? text.secret : text.plain}</small>}
                          {variable.conflict && <span className="tag import-conflict">{text.conflict}</span>}
                          {variable.values > 1 && (
                            <label>
                              <small>{fill(text.duplicate, { n: variable.values })} · {text.keepWhich}</small>
                              <select aria-label={`${text.keepWhich} ${variable.name}`} value={choice.picks[variable.name] ?? variable.pick} onChange={(event) => patch(item.id, { picks: { ...choice.picks, [variable.name]: Number(event.target.value) } })}>
                                {Array.from({ length: variable.values }, (_, index) => <option key={index} value={index}>{fill(text.nth, { n: index + 1 })}</option>)}
                              </select>
                            </label>
                          )}
                          {variable.apiKeyItem && <small>→ {review.items.find((other) => other.id === variable.apiKeyItem)?.name}</small>}
                        </li>
                      );
                    })}
                  </ul>
                </div>
              );
            })}
          </section>
        ))}
        {review.skipped.length > 0 && (
          <section className="import-skipped" aria-label={text.skippedTitle}>
            <h3>{text.skippedTitle}</h3>
            <ul>{review.skipped.map((skipped, index) => <li key={`${skipped.name}-${index}`}><code>{skipped.name}</code> <small>{text.reasons[skipped.reason] ?? skipped.reason}</small></li>)}</ul>
          </section>
        )}
        {error && <div className="form-error" role="alert">{error}</div>}
        {message && <div className="form-success" role="status">{message}</div>}
        <div className="form-actions">
          <span className="import-summary" aria-live="polite">{fill(text.summary, { a: totals.added, u: totals.updated, s: totals.skipped })}</span>
          <button type="button" className="secondary-button" onClick={() => void cancel()}>{t.form.cancel}</button>
          <button type="button" className="primary-button" disabled={busy || review.items.length === 0} onClick={() => void confirm()}>{busy ? text.importing : text.confirm}</button>
        </div>
      </div>
    </div>
  );
}
