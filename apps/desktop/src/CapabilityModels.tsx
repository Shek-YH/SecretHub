import { useRef, useState } from 'react';
import type { BackendModel, CapabilityModelRow } from './lib/backend';
import type { getTranslation } from './i18n/translations';

type T = ReturnType<typeof getTranslation>;

// Same order and defaults as crates/storage (CAPABILITY_MODEL_ORDER / default_capability_env_key).
export const CAPABILITY_ORDER = ['code', 'reasoning', 'vision', 'image_gen', 'video_gen', 'tts', 'asr', 'embedding', 'rerank'] as const;
export const DEFAULT_CAPABILITY_ENV: Record<string, string> = { code: 'CODE_MODEL', reasoning: 'REASONING_MODEL', vision: 'VISION_MODEL', image_gen: 'IMAGE_MODEL', video_gen: 'VIDEO_MODEL', tts: 'TTS_MODEL', asr: 'ASR_MODEL', embedding: 'EMBEDDING_MODEL', rerank: 'RERANK_MODEL' };
const ENV_NAME = /^[A-Z_][A-Z0-9_]*$/;
const MAX_OPTIONS = 500;

export const capabilityLabel = (t: T, capability: string) => t.cap.labels[capability] ?? capability;

/** The section shows for aggregators, or when the key's synced models span at least two capabilities. */
export function needsCapabilitySection(kind: string | undefined, models: BackendModel[]): boolean {
  return kind === 'aggregator' || new Set(models.flatMap((model) => model.capabilities)).size >= 2;
}

/** Empty env names are allowed (row is not exported); names must be valid and unique, also against `reserved`. */
export function validateCapabilityRows(rows: CapabilityModelRow[], reserved: string[]): 'invalid' | 'duplicate' | null {
  const used = new Set(reserved.map((name) => name.trim()).filter(Boolean));
  for (const row of rows) {
    const env = row.env_key.trim();
    if (!row.model_id.trim() || !env) continue;
    if (!ENV_NAME.test(env)) return 'invalid';
    if (used.has(env)) return 'duplicate';
    used.add(env);
  }
  return null;
}

/** Drops cleared rows and sorts them in display order. */
export function normalizeRows(rows: CapabilityModelRow[]): CapabilityModelRow[] {
  const order = (capability: string) => CAPABILITY_ORDER.indexOf(capability as (typeof CAPABILITY_ORDER)[number]);
  return rows.filter((row) => row.model_id.trim()).map((row) => ({ ...row, model_id: row.model_id.trim(), env_key: row.env_key.trim().toUpperCase() })).sort((left, right) => order(left.capability) - order(right.capability));
}

export function capabilityError(t: T, rows: CapabilityModelRow[], reserved: string[]): string {
  const problem = validateCapabilityRows(rows, reserved);
  return problem === 'invalid' ? t.cap.invalidEnv : problem === 'duplicate' ? t.cap.duplicateEnv : '';
}

/** One row per capability: model dropdown (candidates come from the key's synced models) plus an editable variable name. */
export function CapabilityModelsEditor({ t, models, value, onChange, primaryModelId }: { t: T; models: BackendModel[]; value: CapabilityModelRow[]; onChange: (rows: CapabilityModelRow[]) => void; primaryModelId?: string }) {
  const touched = useRef(new Set<string>());
  const rowOf = (capability: string) => value.find((row) => row.capability === capability);
  function update(capability: string, patch: Partial<CapabilityModelRow>) {
    const rest = value.filter((row) => row.capability !== capability);
    const next = { capability, model_id: '', env_key: '', ...rowOf(capability), ...patch };
    onChange([...rest, next]);
  }
  function chooseModel(capability: string, modelId: string) {
    if (!modelId) { touched.current.delete(capability); update(capability, { model_id: '', env_key: '' }); return; }
    const current = rowOf(capability);
    update(capability, { model_id: modelId, env_key: !touched.current.has(capability) && !current?.env_key ? DEFAULT_CAPABILITY_ENV[capability] ?? '' : current?.env_key ?? '' });
  }
  return <div className="cap-editor">
    <div className="cap-row cap-row-chat"><span className="cap-name">{t.cap.chatRow}</span><code className="cap-primary">{primaryModelId || '—'}</code></div>
    {CAPABILITY_ORDER.map((capability) => {
      const label = capabilityLabel(t, capability);
      const row = rowOf(capability);
      const candidates = models.filter((model) => model.capabilities.includes(capability)).map((model) => model.model_id).slice(0, MAX_OPTIONS);
      const options = row?.model_id && !candidates.includes(row.model_id) ? [row.model_id, ...candidates] : candidates;
      return <div className="cap-row" key={capability}>
        <span className="cap-name">{label}</span>
        {options.length === 0 ? <span className="cap-empty">{t.cap.noModels}</span> : <select aria-label={`${label} ${t.cap.modelAria}`} value={row?.model_id ?? ''} onChange={(event) => chooseModel(capability, event.target.value)}><option value="">{t.cap.notSet}</option>{options.map((id) => <option value={id} key={id}>{id}</option>)}</select>}
        <input aria-label={`${label} ${t.cap.envAria}`} value={row?.env_key ?? ''} disabled={!row?.model_id} placeholder={DEFAULT_CAPABILITY_ENV[capability]} onChange={(event) => { touched.current.add(capability); update(capability, { env_key: event.target.value.toUpperCase() }); }} />
      </div>;
    })}
  </div>;
}

/** Detail-page version: own draft plus an explicit save button. */
export function CapabilityModelsPanel({ t, models, rows, primaryModelId, reservedEnvKeys, onSave }: { t: T; models: BackendModel[]; rows: CapabilityModelRow[]; primaryModelId?: string; reservedEnvKeys: string[]; onSave: (rows: CapabilityModelRow[]) => Promise<void> }) {
  const [draft, setDraft] = useState<CapabilityModelRow[]>(rows);
  const [syncedFrom, setSyncedFrom] = useState(rows);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  // Re-seed the draft when the stored rows change (another key was selected or a default was set elsewhere).
  if (syncedFrom !== rows) { setSyncedFrom(rows); setDraft(rows); setMessage(''); setError(''); }
  async function save() {
    const next = normalizeRows(draft);
    const problem = capabilityError(t, next, reservedEnvKeys);
    if (problem) { setError(problem); setMessage(''); return; }
    try { await onSave(next); setError(''); setMessage(t.cap.saved); } catch (reason) { setError(String(reason)); setMessage(''); }
  }
  return <section className="cap-section" aria-label={t.cap.title}>
    <div className="model-sync-head"><strong>{t.cap.title}</strong><button type="button" className="secondary-button" onClick={() => void save()}>{t.cap.save}</button></div>
    <p className="cap-hint">{t.cap.hint}</p>
    <CapabilityModelsEditor t={t} models={models} value={draft} onChange={setDraft} primaryModelId={primaryModelId} />
    {message && <div className="form-success">{message}</div>}
    {error && <div className="form-error" role="alert">{error}</div>}
  </section>;
}
