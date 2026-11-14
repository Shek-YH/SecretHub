import { useEffect, useId, useState, type FormEvent, type ReactNode } from 'react';
import type { getTranslation } from '../i18n/translations';
import { TagField } from '../TagPicker';
import { TypeIcon } from './TypeIcon';
import type { ItemKind } from './itemTypes';

export type T = ReturnType<typeof getTranslation>;

/** `{type}` placeholder in the localized add/edit headings. */
export function formTitle(t: T, kind: ItemKind, editing: boolean): string {
  return (editing ? t.items.editTitle : t.items.addTitle).replace('{type}', t.items.types[kind].name);
}

type ShellProps = {
  t: T;
  kind: ItemKind;
  editing: boolean;
  nameLabel?: string;
  name: string;
  onName: (value: string) => void;
  tags: string[];
  onTags: (tags: string[]) => void;
  knownTags: string[];
  favorite: boolean;
  onFavorite: (value: boolean) => void;
  error: string;
  /** Returns to the type picker (only offered while adding). */
  onBack?: () => void;
  onClose: () => void;
  onSubmit: () => void | Promise<void>;
  children: ReactNode;
};

/** Modal frame shared by the typed item forms: heading, name, type fields, tags, favorite, save. */
export function ItemFormShell({ t, kind, editing, nameLabel, name, onName, tags, onTags, knownTags, favorite, onFavorite, error, onBack, onClose, onSubmit, children }: ShellProps) {
  const [busy, setBusy] = useState(false);
  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    try { await onSubmit(); } finally { setBusy(false); }
  }
  const title = formTitle(t, kind, editing);
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={title}>
      <form className="secret-form quick-form item-form" onSubmit={(event) => void submit(event)} noValidate>
        <div className="form-heading">
          <div>
            <span className="section-kicker"><TypeIcon kind={kind} size={12} /> {t.items.types[kind].name}</span>
            <h2>{title}</h2>
            <p className="form-intro">{t.items.types[kind].hint}</p>
          </div>
          <div className="form-heading-actions">
            {!editing && onBack && <button type="button" className="back-button" onClick={onBack}>{t.form.back}</button>}
            <button type="button" className="close-button" onClick={onClose} aria-label={t.form.cancel}>×</button>
          </div>
        </div>
        <label>
          {nameLabel ?? t.form.name}
          <input required value={name} onChange={(event) => onName(event.target.value)} placeholder={t.form.required} />
        </label>
        {children}
        <label className="checkbox-label">
          <input type="checkbox" checked={favorite} onChange={(event) => onFavorite(event.target.checked)} />
          {t.form.favorite}
        </label>
        <TagField t={t} value={tags} onChange={onTags} knownTags={knownTags} />
        {error && <div className="form-error" role="alert">{error}</div>}
        <div className="form-security"><span>◆</span>{t.security.encrypted}</div>
        <div className="form-actions">
          <button type="button" className="secondary-button" onClick={onClose}>{t.form.cancel}</button>
          <button className="primary-button" type="submit" disabled={busy}>{t.form.save}</button>
        </div>
      </form>
    </div>
  );
}

type SecretInputProps = {
  t: T;
  label: string;
  value: string;
  onChange: (value: string) => void;
  editing?: boolean;
  /** Optional secrets of a stored item can be emptied with a button instead of typing. */
  clearable?: boolean;
  cleared?: boolean;
  onToggleClear?: () => void;
  multiline?: boolean;
  inputMode?: 'numeric';
  maxLength?: number;
  placeholder?: string;
  /** Bumping this number reveals the value (used after generating a password). */
  showSignal?: number;
  hint?: ReactNode;
  children?: ReactNode;
};

/** Masked input for a secret field with a show/hide toggle and, when editing, a clear button. */
export function SecretInput({ t, label, value, onChange, editing, clearable, cleared, onToggleClear, multiline, inputMode, maxLength, placeholder, showSignal, hint, children }: SecretInputProps) {
  const [shown, setShown] = useState(false);
  useEffect(() => { if (showSignal) setShown(true); }, [showSignal]);
  const id = useId();
  const disabled = Boolean(cleared);
  const common = {
    id,
    value,
    disabled,
    autoComplete: 'off' as const,
    spellCheck: false,
    placeholder: editing ? t.items.keepUnchanged : placeholder,
    onChange: (event: { target: { value: string } }) => onChange(event.target.value),
  };
  return (
    <div className="secret-input">
      <label htmlFor={id}>{label}</label>
      <div className="key-input-row">
        {multiline
          ? <textarea {...common} rows={4} maxLength={maxLength} className={shown ? '' : 'masked-textarea'} />
          : <input {...common} type={shown ? 'text' : 'password'} inputMode={inputMode} maxLength={maxLength} />}
        <button type="button" className="secondary-button" aria-pressed={shown} aria-label={`${shown ? t.quick.hideKey : t.quick.showKey} ${label}`} onClick={() => setShown((current) => !current)}>{shown ? t.quick.hideKey : t.quick.showKey}</button>
        {editing && clearable && <button type="button" className="secondary-button" aria-pressed={disabled} aria-label={`${disabled ? t.items.undoClear : t.items.clearField} ${label}`} onClick={onToggleClear}>{disabled ? t.items.undoClear : t.items.clearField}</button>}
        {children}
      </div>
      {disabled && <p className="cap-hint">{t.items.clearedHint}</p>}
      {hint}
    </div>
  );
}
