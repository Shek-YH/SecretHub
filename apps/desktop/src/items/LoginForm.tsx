import { useState } from 'react';
import { ItemFormShell, SecretInput } from './ItemFormShell';
import { labelOf, validTotp, validWebsite, type ItemFormProps } from './formTypes';
import { PASSWORD_DEFAULT, PASSWORD_MAX, PASSWORD_MIN, generatePassword, validEnvPrefix } from './itemTypes';

/** Website login: URL, account, password (with generator), optional two-step secret and notes. */
export function LoginForm({ t, knownTags, initial, initialFields = {}, onBack, onClose, onSave }: ItemFormProps) {
  const editing = Boolean(initial);
  const [name, setName] = useState(initial?.name ?? '');
  const [website, setWebsite] = useState(initialFields.website ?? '');
  const [username, setUsername] = useState(initialFields.username ?? '');
  const [password, setPassword] = useState('');
  const [totp, setTotp] = useState('');
  const [notes, setNotes] = useState(initialFields.notes ?? '');
  const [cleared, setCleared] = useState<string[]>([]);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [devUse, setDevUse] = useState(initial?.devUse ?? false);
  const [prefix, setPrefix] = useState(initial?.devUse ? initial.envKey : '');
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [length, setLength] = useState(PASSWORD_DEFAULT);
  const [symbols, setSymbols] = useState(true);
  const [revealSignal, setRevealSignal] = useState(0);
  const [error, setError] = useState('');

  const toggleClear = (key: string) => setCleared((current) => (current.includes(key) ? current.filter((item) => item !== key) : [...current, key]));
  const label = (key: string) => labelOf(t, 'login', key);

  function generate() {
    setPassword(generatePassword({ length, symbols }));
    setCleared((current) => current.filter((key) => key !== 'password'));
    setRevealSignal((value) => value + 1);
  }

  async function submit() {
    setError('');
    const problem = !name.trim() || !username.trim() ? t.form.required
      : website.trim() && !validWebsite(website.trim()) ? t.items.urlInvalid
      : totp.trim() && !validTotp(totp) ? t.items.totpInvalid
      : devUse && !validEnvPrefix(prefix.trim()) ? t.items.devPrefixInvalid
      : '';
    if (problem) { setError(problem); if (totp.trim() && !validTotp(totp)) setAdvancedOpen(true); return; }
    const fields: Record<string, string> = { username: username.trim() };
    // Plain fields are always sent when editing so that emptying them clears them.
    if (website.trim() || editing) fields.website = website.trim();
    if (notes.trim() || editing) fields.notes = notes;
    if (password && !cleared.includes('password')) fields.password = password;
    if (totp.trim() && !cleared.includes('totp')) fields.totp = totp.trim();
    try {
      await onSave({ itemType: 'login', name: name.trim(), fields, clear: editing ? cleared : [], tags, favorite, devUse, envKey: devUse ? prefix.trim() : '' });
      onClose();
    } catch (reason) { setError(String(reason)); }
  }

  return (
    <ItemFormShell t={t} kind="login" editing={editing} name={name} onName={setName} tags={tags} onTags={setTags} knownTags={knownTags} favorite={favorite} onFavorite={setFavorite} error={error} onBack={onBack} onClose={onClose} onSubmit={submit}>
      <label>
        {label('website')}
        <input value={website} onChange={(event) => setWebsite(event.target.value)} placeholder="https://" autoComplete="off" spellCheck={false} />
      </label>
      <label>
        {label('username')}
        <input required value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="off" spellCheck={false} />
      </label>
      <SecretInput t={t} label={label('password')} value={password} onChange={setPassword} editing={editing} clearable cleared={cleared.includes('password')} onToggleClear={() => toggleClear('password')} showSignal={revealSignal}>
        <button type="button" className="secondary-button" onClick={generate} disabled={cleared.includes('password')}>{t.items.generate}</button>
      </SecretInput>
      <div className="password-generator" role="group" aria-label={t.items.generate}>
        <label className="generator-length">
          {t.items.genLength}: {length}
          <input type="range" min={PASSWORD_MIN} max={PASSWORD_MAX} value={length} onChange={(event) => setLength(Number(event.target.value))} />
        </label>
        <label className="checkbox-label">
          <input type="checkbox" checked={symbols} onChange={(event) => setSymbols(event.target.checked)} />
          {t.items.genSymbols}
        </label>
      </div>
      <button type="button" className="advanced-toggle" aria-expanded={advancedOpen} onClick={() => setAdvancedOpen((open) => !open)}>
        <span aria-hidden="true">{advancedOpen ? '▾' : '▸'}</span> {t.quick.advanced}
      </button>
      {advancedOpen && (
        <div className="advanced-section">
          <SecretInput t={t} label={t.items.totpAdvanced} value={totp} onChange={setTotp} editing={editing} clearable cleared={cleared.includes('totp')} onToggleClear={() => toggleClear('totp')} />
          <label>
            {label('notes')}
            <textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={3} />
          </label>
        </div>
      )}
      <label className="checkbox-label">
        <input type="checkbox" checked={devUse} onChange={(event) => setDevUse(event.target.checked)} />
        {t.items.devUse}
      </label>
      {devUse && (
        <div className="dev-use-section">
          <p className="cap-hint">{t.items.devUseHint}</p>
          <label>
            {t.items.devPrefix}
            <input value={prefix} onChange={(event) => setPrefix(event.target.value.toUpperCase())} placeholder="DOUYIN_TEST" autoComplete="off" spellCheck={false} />
          </label>
          <p className="cap-hint">{t.items.devPrefixHint.replace(/\{prefix\}/g, prefix.trim() || 'PREFIX')}</p>
        </div>
      )}
    </ItemFormShell>
  );
}
