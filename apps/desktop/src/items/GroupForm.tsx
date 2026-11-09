import { useState } from 'react';
import type { GroupPublic } from '../lib/backend';
import { backend, isTauriRuntime } from '../lib/backend';
import type { SecretMetadata } from '../secretTypes';
import { ItemFormShell, SecretInput, type T } from './ItemFormShell';
import { GROUP_CATEGORIES, GROUP_TEMPLATES, MAX_GROUP_VARIABLES, VARIABLE_NAME, isKeyName, isSecretName, prefixedName, templateLabel } from './groupTemplates';

/** What the group form hands to the app; the app decides between create and update. */
export type GroupSavePayload = {
  name: string;
  category: string;
  variables: Array<{ name: string; secret: boolean; value?: string; from?: string }>;
  /** Only present when the notes were created or opened for editing; absent keeps the stored notes. */
  notes?: string;
  tags: string[];
  favorite: boolean;
  aiValues: boolean;
};

type Props = {
  t: T;
  knownTags: string[];
  initial: SecretMetadata | null;
  /** Names, secret flags and plain values of the group being edited. */
  initialPublic?: GroupPublic;
  onBack?: () => void;
  onClose: () => void;
  onSave: (payload: GroupSavePayload) => Promise<void>;
};

type Row = {
  key: number;
  name: string;
  value: string;
  secret: boolean;
  /** Template variable name this row came from; lets the prefix rename it. */
  base: string;
  edited: boolean;
  /** The secret switch was set by hand, so typing a name no longer changes it. */
  secretTouched: boolean;
  /** Stored name when the row exists in the group being edited. */
  from?: string;
  /** Edit only: empty a stored secret value on save. */
  cleared: boolean;
};

let nextKey = 1;
const newRow = (patch: Partial<Row> = {}): Row => ({ key: nextKey++, name: '', value: '', secret: false, base: '', edited: false, secretTouched: false, cleared: false, ...patch });

function rowsFromTemplate(templateKey: string, prefix: string): Row[] {
  const template = GROUP_TEMPLATES.find((item) => item.key === templateKey);
  return (template?.variables ?? []).map((variable) => newRow({ name: prefixedName(prefix, variable.name), base: variable.name, secret: variable.secret, value: variable.default ?? '' }));
}

/** Config group: a named set of variables where each one is plain or secret. */
export function GroupForm({ t, knownTags, initial, initialPublic, onBack, onClose, onSave }: Props) {
  const editing = Boolean(initial);
  const [name, setName] = useState(initial?.name ?? '');
  const [templateKey, setTemplateKey] = useState(editing ? '' : 'server');
  const [prefix, setPrefix] = useState('');
  const [rows, setRows] = useState<Row[]>(() => editing
    ? (initialPublic?.variables ?? []).map((variable) => newRow({ name: variable.name, value: variable.value ?? '', secret: variable.secret, from: variable.name, edited: true }))
    : rowsFromTemplate('server', ''));
  const [notes, setNotes] = useState('');
  const [notesOpen, setNotesOpen] = useState(!editing);
  const [aiValues, setAiValues] = useState(initial?.aiValues ?? true);
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [error, setError] = useState('');
  const category = editing
    ? (GROUP_CATEGORIES.includes(initial?.category ?? '') ? initial!.category! : 'other')
    : (GROUP_TEMPLATES.find((item) => item.key === templateKey)?.category ?? 'other');

  const patch = (key: number, change: Partial<Row>) => setRows((current) => current.map((row) => (row.key === key ? { ...row, ...change } : row)));

  function chooseTemplate(key: string) {
    setTemplateKey(key);
    setRows(rowsFromTemplate(key, prefix));
  }

  function changePrefix(value: string) {
    setPrefix(value);
    setRows((current) => current.map((row) => (row.base && !row.edited ? { ...row, name: prefixedName(value, row.base) } : row)));
  }

  function changeName(row: Row, value: string) {
    // A typed name picks the secret default only for new free-form rows nobody toggled by hand.
    const guess = !row.secretTouched && !row.from && !row.base ? { secret: isSecretName(value) } : {};
    patch(row.key, { name: value, edited: true, ...guess });
  }

  async function openNotes() {
    if (!initial || !isTauriRuntime()) { setNotesOpen(true); return; }
    try { setNotes(await backend.groupNotes(initial.id)); setNotesOpen(true); }
    catch (reason) { setError(`${t.items.loadFailed}: ${String(reason)}`); }
  }

  async function submit() {
    setError('');
    if (!name.trim()) { setError(t.form.required); return; }
    if (prefix.trim() && !VARIABLE_NAME.test(prefix.trim())) { setError(t.groups.prefixInvalid); return; }
    // New groups skip template rows nobody filled in; an edit keeps every row.
    const filled = rows.filter((row) => row.name.trim() && (editing || row.value !== ''));
    if (filled.length > MAX_GROUP_VARIABLES) { setError(t.groups.tooMany); return; }
    const names = new Set<string>();
    for (const row of filled) {
      const variable = row.name.trim();
      if (!VARIABLE_NAME.test(variable)) { setError(`${t.groups.nameInvalid}: ${variable}`); return; }
      if (names.has(variable)) { setError(`${t.groups.nameDuplicate}: ${variable}`); return; }
      names.add(variable);
    }
    const variables = filled.map((row) => {
      const variable = row.name.trim();
      if (!editing) return { name: variable, secret: row.secret, value: row.value };
      const from = row.from && row.from !== variable ? row.from : undefined;
      // A stored secret keeps its value unless something was typed or the clear button was used.
      const value = row.secret ? (row.cleared ? '' : row.value !== '' ? row.value : undefined) : row.value;
      return { name: variable, secret: row.secret, ...(value !== undefined ? { value } : {}), ...(from ? { from } : {}) };
    });
    try {
      await onSave({ name: name.trim(), category, variables, ...(notesOpen ? { notes } : {}), tags, favorite, aiValues });
      onClose();
    } catch (reason) { setError(String(reason)); }
  }

  return (
    <ItemFormShell t={t} kind="env_group" editing={editing} name={name} onName={setName} tags={tags} onTags={setTags} knownTags={knownTags} favorite={favorite} onFavorite={setFavorite} error={error} onBack={onBack} onClose={onClose} onSubmit={submit}>
      {!editing && (
        <div className="group-template-row">
          <label>
            {t.groups.templateLabel}
            <select value={templateKey} onChange={(event) => chooseTemplate(event.target.value)}>
              {GROUP_TEMPLATES.map((template) => <option key={template.key} value={template.key}>{templateLabel(template, t.items.labelLocale)}</option>)}
            </select>
          </label>
          <label>
            {t.groups.prefix}
            <input value={prefix} onChange={(event) => changePrefix(event.target.value)} placeholder="YECAOYUN" autoComplete="off" spellCheck={false} />
          </label>
          <p className="cap-hint">{t.groups.prefixHint}</p>
        </div>
      )}
      <fieldset className="group-variables">
        <legend>{t.groups.variables}</legend>
        {rows.map((row) => {
          const label = row.name || t.groups.varName;
          return (
            <div className="group-variable-row" key={row.key} data-secret={row.secret ? 'true' : 'false'}>
              <input className="group-var-name" aria-label={`${t.groups.varName} ${label}`} value={row.name} onChange={(event) => changeName(row, event.target.value)} placeholder="NAME" autoComplete="off" spellCheck={false} />
              {row.secret
                ? <SecretInput t={t} label={`${t.groups.varValue} ${label}`} value={row.value} onChange={(value) => patch(row.key, { value })} editing={Boolean(row.from)} clearable={Boolean(row.from)} cleared={row.cleared} onToggleClear={() => patch(row.key, { cleared: !row.cleared, value: '' })} multiline={isKeyName(row.name)} />
                : (isKeyName(row.name)
                  ? <textarea aria-label={`${t.groups.varValue} ${label}`} value={row.value} rows={4} spellCheck={false} onChange={(event) => patch(row.key, { value: event.target.value })} />
                  : <input aria-label={`${t.groups.varValue} ${label}`} value={row.value} autoComplete="off" spellCheck={false} onChange={(event) => patch(row.key, { value: event.target.value })} />)}
              <label className="checkbox-label group-secret-switch">
                <input type="checkbox" checked={row.secret} aria-label={`${t.groups.secretSwitch} ${label}`} onChange={(event) => patch(row.key, { secret: event.target.checked, secretTouched: true, cleared: false })} />
                {t.groups.secretSwitch}
              </label>
              <button type="button" className="secondary-button" aria-label={`${t.groups.removeVariable} ${label}`} onClick={() => setRows((current) => current.filter((item) => item.key !== row.key))}>×</button>
            </div>
          );
        })}
        <button type="button" className="secondary-button" onClick={() => setRows((current) => [...current, newRow()])}>+ {t.groups.addVariable}</button>
      </fieldset>
      <p className="cap-hint">{t.groups.aiValuesHint}</p>
      <label className="checkbox-label">
        <input type="checkbox" checked={aiValues} onChange={(event) => setAiValues(event.target.checked)} />
        {t.groups.aiValues}
      </label>
      {notesOpen
        ? <label>{t.groups.notes}<textarea value={notes} rows={5} maxLength={100000} spellCheck={false} onChange={(event) => setNotes(event.target.value)} /></label>
        : <div className="group-notes-closed"><p className="cap-hint">{t.groups.notesKept}</p><button type="button" className="secondary-button" onClick={() => void openNotes()}>{t.groups.editNotes}</button></div>}
    </ItemFormShell>
  );
}
