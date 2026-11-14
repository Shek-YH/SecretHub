import { useState } from 'react';
import type { getTranslation } from '../i18n/translations';
import type { SecretMetadata, SecretRequest } from '../secretTypes';
import { ItemFormShell, SecretInput } from './ItemFormShell';
import { labelOf } from './formTypes';

type Props = {
  t: ReturnType<typeof getTranslation>;
  knownTags: string[];
  initial: SecretMetadata | null;
  onBack?: () => void;
  onClose: () => void;
  onSave: (request: SecretRequest) => Promise<void>;
};

const ENV_NAME = /^[A-Z_][A-Z0-9_]*$/;

/** Developer secret (token, connection string, SSH key): a raw value plus an optional variable name. */
export function DevSecretForm({ t, knownTags, initial, onBack, onClose, onSave }: Props) {
  const editing = Boolean(initial);
  const [name, setName] = useState(initial?.name ?? '');
  const [value, setValue] = useState('');
  const [envKey, setEnvKey] = useState(initial?.envKey ?? '');
  const [description, setDescription] = useState(initial?.description ?? '');
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [error, setError] = useState('');
  const label = (key: string) => labelOf(t, 'dev_secret', key);

  async function submit() {
    setError('');
    const key = envKey.trim().toUpperCase();
    if (!name.trim() || (!editing && !value.trim())) { setError(t.form.required); return; }
    if (key && !ENV_NAME.test(key)) { setError(t.items.envInvalid); return; }
    try {
      await onSave({
        name: name.trim(),
        provider_id: initial?.provider || 'generic',
        env_key: key,
        description: description.trim(),
        tags,
        value,
        value_type: 'dev_secret',
        category: initial?.category ?? 'other',
        scope: initial?.scope ?? 'global',
        favorite,
        archived: initial?.archived ?? false,
        model_id: initial?.modelId ?? '',
        model_env_key: initial?.modelEnvKey ?? '',
        endpoint_url: initial?.endpointUrl ?? '',
        endpoint_env_key: initial?.endpointEnvKey ?? '',
      });
      onClose();
    } catch (reason) { setError(String(reason)); }
  }

  return (
    <ItemFormShell t={t} kind="dev_secret" editing={editing} name={name} onName={setName} tags={tags} onTags={setTags} knownTags={knownTags} favorite={favorite} onFavorite={setFavorite} error={error} onBack={onBack} onClose={onClose} onSubmit={submit}>
      <SecretInput t={t} label={label('value')} value={value} onChange={setValue} editing={editing} multiline />
      <label>
        {label('env_key')}
        <input value={envKey} onChange={(event) => setEnvKey(event.target.value.toUpperCase())} placeholder="DATABASE_URL" autoComplete="off" spellCheck={false} />
      </label>
      <label>
        {label('description')}
        <input value={description} onChange={(event) => setDescription(event.target.value)} autoComplete="off" />
      </label>
    </ItemFormShell>
  );
}
