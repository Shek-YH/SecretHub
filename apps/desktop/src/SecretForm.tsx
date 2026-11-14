import { useId, useRef, useState, type FormEvent } from 'react';
import type { getTranslation } from './i18n/translations';
import type { BackendModel, CapabilityModelRow } from './lib/backend';
import { backend, isTauriRuntime } from './lib/backend';
import { TagField } from './TagPicker';
import { buildModelOptions } from './modelOptions';
import {
  defaultApiKeyEnvKey,
  defaultEndpointEnvKey,
  detectProviderByPrefix,
  groupProviders,
  providerKind,
  providerTemplate,
  providerTemplates,
  resolveProviderId,
  uniqueCredentialName,
  type ProviderTemplate,
} from './providers';
import { CapabilityModelsEditor, capabilityError, needsCapabilitySection, normalizeRows } from './CapabilityModels';
import { FilterableSelect, SEARCH_THRESHOLD } from './FilterableSelect';
import type { CredentialPreset, SecretMetadata, SecretRequest } from './secretTypes';

type T = ReturnType<typeof getTranslation>;

export type ExistingSecretSummary = { name: string; provider: string; archived?: boolean };

type SecretFormProps = {
  t: T;
  knownTags: string[];
  initial: SecretMetadata | null;
  preset?: CredentialPreset | null;
  /** Used to suggest unique names and to keep providers that are already in use in the common group. */
  existingSecrets?: ExistingSecretSummary[];
  syncedModels?: BackendModel[];
  capabilityRows?: CapabilityModelRow[];
  onCapture?: (providerId: string, providerName: string) => void;
  /** Goes back to the "what do you want to save" page; the link is hidden when missing. */
  onChangeType?: () => void;
  onClose: () => void;
  onSave: (request: SecretRequest) => Promise<void>;
};

const CATEGORY_OPTIONS = [
  ['ai', 'AI'],
  ['database', 'Database'],
  ['cloud', 'Cloud'],
  ['git', 'Git'],
  ['other', 'Other'],
] as const;

export function SecretForm({
  t,
  knownTags,
  initial,
  preset,
  existingSecrets = [],
  syncedModels = [],
  capabilityRows = [],
  onCapture,
  onChangeType,
  onClose,
  onSave,
}: SecretFormProps) {
  const editing = Boolean(initial);
  const existingNames = existingSecrets.map((item) => item.name);
  const usedProviderIds = existingSecrets
    .filter((item) => !item.archived)
    .map((item) => resolveProviderId(item.provider))
    .filter((id): id is string => Boolean(id));
  const initialProvider = initial
    ? providerTemplates.find((item) => item.id === initial.provider || item.name === initial.provider)?.id ?? 'custom'
    : preset?.provider ?? '';

  const [provider, setProvider] = useState(initialProvider);
  const [moreOpen, setMoreOpen] = useState(false);
  const [providerQuery, setProviderQuery] = useState('');
  const [name, setName] = useState(initial?.name ?? (preset ? uniqueCredentialName(preset.name, existingNames) : ''));
  const [envKey, setEnvKey] = useState(initial?.envKey ?? preset?.envKey ?? '');
  const [value, setValue] = useState('');
  const [showValue, setShowValue] = useState(false);
  const [category, setCategory] = useState(initial?.category ?? (preset && preset.provider !== 'custom' ? 'ai' : 'other'));
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [notes, setNotes] = useState(initial?.description ?? '');
  const [modelId, setModelId] = useState(initial?.modelId ?? preset?.modelId ?? '');
  const [modelEnvKey, setModelEnvKey] = useState(initial?.modelEnvKey ?? preset?.modelEnvKey ?? '');
  const [endpointUrl, setEndpointUrl] = useState(initial?.endpointUrl ?? preset?.endpointUrl ?? '');
  const [endpointEnvKey, setEndpointEnvKey] = useState(initial?.endpointEnvKey ?? preset?.endpointEnvKey ?? '');
  const [capDraft, setCapDraft] = useState<CapabilityModelRow[]>(capabilityRows);
  const [advancedOpen, setAdvancedOpen] = useState(editing);
  const [error, setError] = useState('');
  const keyInputId = useId();

  // Fields the user edited by hand are never overwritten by provider defaults.
  const touched = useRef(new Set<string>(editing ? ['name', 'envKey'] : []));
  // Once the user picks a provider (or one was preselected) prefix detection stops switching it.
  const manualProvider = useRef(Boolean(preset) || editing);

  const template: ProviderTemplate = providerTemplate(provider);
  const aggregator = providerKind(provider) === 'aggregator';
  const showCaps = editing && needsCapabilitySection(providerKind(provider), syncedModels);
  const modelGroups = buildModelOptions(template.models, template.id === 'custom' ? [] : syncedModels, template.defaultModel);
  const modelOptions = [...modelGroups.synced, ...modelGroups.recommended];
  const isCustomModel = template.id === 'custom' || !modelOptions.some((model) => model.id === modelId);
  const commonProviders = groupProviders(providerTemplates, usedProviderIds).common;
  const selectedExtra = provider && !commonProviders.some((item) => item.id === provider) ? providerTemplate(provider) : null;
  const panelGroups = groupProviders(providerTemplates, usedProviderIds, providerQuery);
  const panelProviders = panelGroups.searching ? [...panelGroups.common, ...panelGroups.more] : panelGroups.more;
  const detection = detectProviderByPrefix(value, providerTemplates);

  function edited<V>(field: string, setter: (next: V) => void) {
    return (next: V) => {
      touched.current.add(field);
      setter(next);
    };
  }

  function applyTemplate(next: ProviderTemplate) {
    const firstModel = next.models.find((item) => item.id === next.defaultModel) ?? next.models[0];
    const fields = touched.current;
    setProvider(next.id);
    if (!fields.has('name')) setName(uniqueCredentialName(next.name, existingNames));
    if (!fields.has('envKey')) setEnvKey(defaultApiKeyEnvKey(next.id));
    if (!fields.has('modelId')) setModelId(firstModel.id);
    if (!fields.has('modelEnvKey')) setModelEnvKey(firstModel.envKey);
    if (!fields.has('endpointUrl')) setEndpointUrl(next.baseUrl);
    if (!fields.has('endpointEnvKey')) setEndpointEnvKey(defaultEndpointEnvKey(next.id));
    if (!fields.has('category')) setCategory(next.id === 'custom' ? 'other' : 'ai');
  }

  function chooseProvider(next: ProviderTemplate) {
    manualProvider.current = true;
    applyTemplate(next);
    setMoreOpen(false);
    setProviderQuery('');
  }

  function changeValue(next: string) {
    setValue(next);
    if (editing || manualProvider.current) return;
    const { match } = detectProviderByPrefix(next, providerTemplates);
    if (match && match.id !== provider) applyTemplate(match);
  }

  function pickModel(next: string) {
    touched.current.add('modelId');
    setModelId(next);
    const found = modelOptions.find((item) => item.id === next);
    if (found) {
      touched.current.add('modelEnvKey');
      setModelEnvKey(found.envKey);
    }
  }

  async function openExternal(url: string) {
    try {
      if (isTauriRuntime()) await backend.openExternalUrl(url);
      else window.open(url, '_blank', 'noopener,noreferrer');
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    try {
      if (!provider) throw new Error(t.quick.pickProvider);
      if (!name.trim()) throw new Error(t.form.required);
      if (!editing && !value.trim()) throw new Error(t.quick.keyCleared);
      if (template.id === 'custom' && !endpointUrl.trim()) {
        setAdvancedOpen(true);
        throw new Error(t.quick.customNeedsUrl);
      }
      if ((!modelId.trim() && !aggregator) || !endpointUrl.trim()) {
        setAdvancedOpen(true);
        throw new Error(t.form.apiKeyFieldsRequired);
      }
      let capabilityModels: CapabilityModelRow[] | undefined;
      if (showCaps) {
        capabilityModels = normalizeRows(capDraft);
        const problem = capabilityError(t, capabilityModels, [envKey.trim().toUpperCase(), modelEnvKey.trim().toUpperCase(), endpointEnvKey.trim().toUpperCase()]);
        if (problem) throw new Error(problem);
      }
      await onSave({
        ...(capabilityModels ? { capability_models: capabilityModels } : {}),
        name,
        provider_id: provider || 'custom',
        env_key: envKey.trim().toUpperCase(),
        description: notes,
        tags,
        value,
        value_type: 'api_key',
        category,
        scope: initial?.scope ?? 'global',
        favorite,
        archived: initial?.archived ?? false,
        model_id: modelId.trim(),
        model_env_key: modelEnvKey.trim().toUpperCase(),
        endpoint_url: endpointUrl.trim(),
        endpoint_env_key: (endpointEnvKey.trim() || defaultEndpointEnvKey(provider)).toUpperCase(),
      });
      onClose();
    } catch (reason) {
      setError(String(reason));
    }
  }

  const optionLabel = (model: { id: string; label: string; capabilities: string[] }) =>
    `${model.label} · ${model.id}${model.capabilities.length ? ` [${model.capabilities.join(', ')}]` : ''}`;

  let detectionMessage = '';
  if (!editing && value.trim()) {
    if (detection.match && detection.match.id === provider) detectionMessage = t.quick.detected.replace('{name}', detection.match.name);
    else if (detection.ambiguous && !manualProvider.current) detectionMessage = t.quick.ambiguous;
  }

  const providerSection = editing ? (
    <label>
      {t.form.provider}
      <select value={provider} onChange={(event) => applyTemplate(providerTemplate(event.target.value))}>
        <optgroup label={t.quick.commonGroup}>
          {commonProviders.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}
        </optgroup>
        <optgroup label={t.quick.moreGroup}>
          {groupProviders(providerTemplates, usedProviderIds).more.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}
        </optgroup>
      </select>
    </label>
  ) : (
    <div className="provider-chip-section">
      <span className="field-label">{t.quick.provider}</span>
      <div className="provider-chips">
        {commonProviders.map((item) => (
          <button type="button" key={item.id} className={`provider-chip${item.id === provider ? ' active' : ''}`} aria-pressed={item.id === provider} onClick={() => chooseProvider(item)}>
            {item.name}
          </button>
        ))}
        {selectedExtra && (
          <button type="button" className="provider-chip active" aria-pressed onClick={() => chooseProvider(selectedExtra)}>{selectedExtra.name}</button>
        )}
        <button type="button" className="provider-chip provider-chip-more" aria-expanded={moreOpen} onClick={() => setMoreOpen((open) => !open)}>
          {moreOpen ? t.quick.lessButton : t.quick.moreButton}
        </button>
      </div>
      {moreOpen && (
        <div className="provider-more-panel" role="group" aria-label={t.quick.allProviders}>
          <input aria-label={t.form.providerSearch} placeholder={t.form.providerSearch} value={providerQuery} onChange={(event) => setProviderQuery(event.target.value)} />
          {panelProviders.length === 0 ? (
            <div className="empty-state provider-empty"><strong>{t.form.providerSearchEmpty}</strong></div>
          ) : (
            <div className="provider-chips">
              {panelProviders.map((item) => (
                <button type="button" key={item.id} className={`provider-chip${item.id === provider ? ' active' : ''}`} aria-pressed={item.id === provider} onClick={() => chooseProvider(item)}>
                  {item.name}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );

  const modelControl = modelOptions.length > SEARCH_THRESHOLD ? (
    <label>
      {t.form.model}
      <FilterableSelect
        ariaLabel={t.form.model}
        value={modelId}
        onChange={pickModel}
        placeholder={t.quick.modelSearchPlaceholder}
        truncatedLabel={t.quick.modelTruncated}
        noMatchLabel={t.quick.modelNoMatch}
        groups={[
          ...(modelGroups.synced.length > 0 ? [{ label: t.form.syncedModelsGroup, options: modelGroups.synced.map((model) => ({ id: model.id, text: optionLabel(model) })) }] : []),
          { label: modelGroups.synced.length > 0 ? t.form.recommendedModelsGroup : undefined, options: modelGroups.recommended.map((model) => ({ id: model.id, text: optionLabel(model) })) },
        ]}
      />
    </label>
  ) : (
    <label>
      {t.form.model}
      <select
        value={isCustomModel ? '__custom__' : modelId}
        onChange={(event) => {
          const next = event.target.value;
          touched.current.add('modelId');
          touched.current.add('modelEnvKey');
          if (next === '__custom__') {
            setModelId('');
            setModelEnvKey('MODEL_NAME');
          } else {
            pickModel(next);
          }
        }}
      >
        {modelGroups.synced.length > 0 ? (
          <>
            <optgroup label={t.form.syncedModelsGroup}>
              {modelGroups.synced.map((model) => <option value={model.id} key={model.id}>{optionLabel(model)}</option>)}
            </optgroup>
            {modelGroups.recommended.length > 0 && (
              <optgroup label={t.form.recommendedModelsGroup}>
                {modelGroups.recommended.map((model) => <option value={model.id} key={model.id}>{optionLabel(model)}</option>)}
              </optgroup>
            )}
          </>
        ) : (
          modelGroups.recommended.map((model) => <option value={model.id} key={model.id}>{optionLabel(model)}</option>)
        )}
        <option value="__custom__">{t.form.customModel}</option>
      </select>
      {isCustomModel && <input value={modelId} onChange={(event) => edited('modelId', setModelId)(event.target.value)} placeholder="your-model-id" />}
    </label>
  );

  const metaFields = (
    <>
      <label>
        {t.form.category}
        <select value={category} onChange={(event) => edited('category', setCategory)(event.target.value)}>
          {CATEGORY_OPTIONS.map(([id, label]) => <option value={id} key={id}>{label}</option>)}
        </select>
      </label>
      <label className="checkbox-label">
        <input type="checkbox" checked={favorite} onChange={(event) => setFavorite(event.target.checked)} />
        {t.form.favorite}
      </label>
      <TagField t={t} value={tags} onChange={setTags} knownTags={knownTags} />
      <label>
        {t.form.notes}
        <textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={3} />
      </label>
    </>
  );

  const envKeyField = (
    <label>
      {t.form.envKey}
      <span className="optional-label">{t.form.optional}</span>
      <input value={envKey} onChange={(event) => edited('envKey', setEnvKey)(event.target.value)} placeholder="OPENAI_API_KEY" />
    </label>
  );

  const advanced = (
    <>
      {envKeyField}
      <div className="form-two-col">
        {modelControl}
        <label>
          {t.form.modelEnvKey}
          <input value={modelEnvKey} onChange={(event) => edited('modelEnvKey', setModelEnvKey)(event.target.value)} placeholder="OPENAI_MODEL" />
        </label>
      </div>
      <div className="form-two-col">
        <label>
          {t.form.endpointUrl}
          <input value={endpointUrl} onChange={(event) => edited('endpointUrl', setEndpointUrl)(event.target.value)} placeholder="https://api.example.com/v1" />
        </label>
        <label>
          {t.form.endpointEnvKey}
          <input value={endpointEnvKey} onChange={(event) => edited('endpointEnvKey', setEndpointEnvKey)(event.target.value)} placeholder="OPENAI_BASE_URL" />
        </label>
      </div>
      {showCaps && (
        <div className="cap-section">
          <strong>{t.cap.title}</strong>
          <p className="cap-hint">{t.cap.hint}</p>
          <CapabilityModelsEditor t={t} models={syncedModels} value={capDraft} onChange={setCapDraft} primaryModelId={modelId} />
        </div>
      )}
      {metaFields}
    </>
  );

  const defaultModelLine = editing || !provider ? '' : aggregator ? t.cap.newKeyHint : modelId.trim() ? t.quick.defaultModelLine.replace('{model}', modelId.trim()) : '';
  const kicker = `API KEY / ${provider ? template.name.toUpperCase() : 'NEW'}`;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={editing ? t.form.editTitle : t.form.addTitle}>
      <form className="secret-form quick-form" onSubmit={submit} noValidate>
        <div className="form-heading">
          <div>
            <span className="section-kicker">{kicker}</span>
            <h2>{editing ? t.form.editTitle : t.form.addTitle}</h2>
            <p className="form-intro">{t.form.presetIntro}</p>
          </div>
          <div className="form-heading-actions">
            <button type="button" className="close-button" onClick={onClose}>×</button>
          </div>
        </div>

        {providerSection}

        <div className="key-field">
          <label htmlFor={keyInputId}>{editing ? t.quick.keyLabelEdit : t.quick.pasteKey}</label>
          <div className="key-input-row">
            <input
              id={keyInputId}
              required={!editing}
              value={value}
              onChange={(event) => changeValue(event.target.value)}
              type={showValue ? 'text' : 'password'}
              autoComplete="off"
              spellCheck={false}
              placeholder={editing ? t.form.keepExistingValue : undefined}
            />
            <button type="button" className="secondary-button" aria-pressed={showValue} onClick={() => setShowValue((shown) => !shown)}>
              {showValue ? t.quick.hideKey : t.quick.showKey}
            </button>
          </div>
          {detectionMessage && <p className="cap-hint detect-hint" role="status">{detectionMessage}</p>}
          {provider && template.keyUrl && (
            <div className="provider-link-row">
              {onCapture && !editing && (
                <button type="button" className="secondary-button capture-button" onClick={() => onCapture(template.id, template.name)}>{t.quick.getFromSite}</button>
              )}
              <a href={template.keyUrl} target="_blank" rel="noreferrer" onClick={(event) => { event.preventDefault(); void openExternal(template.keyUrl); }}>{t.form.officialKeyLink}</a>
            </div>
          )}
        </div>

        <label>
          {t.form.name}
          <input required value={name} onChange={(event) => edited('name', setName)(event.target.value)} placeholder={t.form.required} />
        </label>

        {defaultModelLine && <p className="cap-hint default-model-line">{defaultModelLine}</p>}

        <button type="button" className="advanced-toggle" aria-expanded={advancedOpen} onClick={() => setAdvancedOpen((open) => !open)}>
          <span aria-hidden="true">{advancedOpen ? '▾' : '▸'}</span> {t.quick.advanced}
        </button>
        {advancedOpen && <div className="advanced-section">{advanced}</div>}

        {error && <div className="form-error" role="alert">{error}</div>}
        <div className="form-security"><span>◆</span>{t.security.encrypted}</div>
        <div className="form-actions">
          <button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button>
          <button className="primary-button" type="submit">{t.form.save}</button>
        </div>
        {!editing && onChangeType && (
          <button type="button" className="inline-link other-types-link" onClick={onChangeType}>{t.quick.otherTypes}</button>
        )}
      </form>
    </div>
  );
}
