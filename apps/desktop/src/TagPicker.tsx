import { useState } from 'react';
import { createPortal } from 'react-dom';
import { backend, isTauriRuntime } from './lib/backend';
import type { getTranslation } from './i18n/translations';

type Translation = ReturnType<typeof getTranslation>;

const sameTag = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** Dedupes case-insensitively (first spelling wins) and sorts case-insensitively. */
function mergeTags(...lists: string[][]): string[] {
  const merged: string[] = [];
  for (const tag of lists.flat()) if (tag && !merged.some((item) => sameTag(item, tag))) merged.push(tag);
  return merged.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
}

function validTagName(name: string): boolean {
  return name.length >= 1 && name.length <= 32 && !name.includes(',') && !/[\u0000-\u001f\u007f]/.test(name);
}

/** Selected-tag chips plus a modal picker over the tag library; tag names are rendered as text only. */
export function TagField({ t, value, onChange, knownTags }: { t: Translation; value: string[]; onChange: (tags: string[]) => void; knownTags: string[] }) {
  const [open, setOpen] = useState(false);
  const [library, setLibrary] = useState<string[]>([]);
  const [localTags, setLocalTags] = useState<string[]>([]);
  const [draft, setDraft] = useState<string[]>([]);
  const [newName, setNewName] = useState('');
  const [managing, setManaging] = useState(false);
  const [error, setError] = useState('');

  async function openPicker() {
    setDraft(value);
    setNewName('');
    setManaging(false);
    setError('');
    setOpen(true);
    if (isTauriRuntime()) {
      try { setLibrary(await backend.tagList()); } catch (reason) { setError(String(reason)); }
    }
  }

  const options = mergeTags(isTauriRuntime() ? library : [...knownTags, ...localTags], draft);
  const isSelected = (tag: string) => draft.some((item) => sameTag(item, tag));
  function toggle(tag: string) { setDraft((current) => (current.some((item) => sameTag(item, tag)) ? current.filter((item) => !sameTag(item, tag)) : [...current, tag])); }

  async function addTag() {
    const name = newName.trim();
    if (!validTagName(name)) { setError(t.form.tagInvalid); return; }
    try {
      const stored = isTauriRuntime() ? await backend.tagCreate(name) : options.find((tag) => sameTag(tag, name)) ?? name;
      if (isTauriRuntime()) setLibrary((current) => mergeTags(current, [stored])); else setLocalTags((current) => mergeTags(current, [stored]));
      setDraft((current) => (current.some((item) => sameTag(item, stored)) ? current : [...current, stored]));
      setNewName('');
      setError('');
    } catch (reason) { setError(String(reason)); }
  }

  async function deleteTag(tag: string) {
    try {
      if (isTauriRuntime()) {
        await backend.tagDelete(tag);
        setLibrary(await backend.tagList());
      } else setLocalTags((current) => current.filter((item) => !sameTag(item, tag)));
      setError('');
    } catch (reason) { setError(String(reason)); }
  }

  function confirm() { onChange(draft); setOpen(false); }

  return <div className="tag-field">
    <span className="tag-field-label">{t.form.tags}</span>
    <div className="tag-row tag-field-selected">
      {value.length === 0 && <small className="tag-field-empty">{t.form.noTagsSelected}</small>}
      {value.map((tag) => <span className="tag tag-chip" key={tag}>{tag}<button type="button" aria-label={`${t.form.removeTag} ${tag}`} onClick={() => onChange(value.filter((item) => item !== tag))}>×</button></span>)}
      <button type="button" className="secondary-button" onClick={() => void openPicker()}>{t.form.chooseTags}</button>
    </div>
    {open && createPortal(
      <div className="modal-backdrop tag-picker-backdrop" role="dialog" aria-modal="true" aria-label={t.form.tagPickerTitle}>
        <div className="secret-form tag-picker">
          <div className="form-heading"><div><h2>{t.form.tagPickerTitle}</h2></div><button type="button" className="close-button" onClick={() => setOpen(false)}>×</button></div>
          {options.length === 0 && <p className="tag-field-empty">{t.form.tagPickerEmpty}</p>}
          <div className="tag-row tag-picker-options">
            {options.map((tag) => <span className="tag-option-wrap" key={tag}>
              <button type="button" className={`tag tag-option${isSelected(tag) ? ' selected' : ''}`} aria-pressed={isSelected(tag)} onClick={() => toggle(tag)}>{tag}</button>
              {managing && <button type="button" className="tag-delete" title={t.form.deleteTagHint} aria-label={`${t.form.deleteTagHint}: ${tag}`} onClick={() => void deleteTag(tag)}>×</button>}
            </span>)}
          </div>
          <div className="tag-new-row">
            <input aria-label={t.form.newTag} placeholder={t.form.newTagPlaceholder} maxLength={32} value={newName} onChange={(event) => setNewName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void addTag(); } }} />
            <button type="button" className="secondary-button" onClick={() => void addTag()}>{t.form.addTag}</button>
          </div>
          {managing && <small className="tag-field-empty">{t.form.deleteTagHint}</small>}
          {error && <div className="form-error" role="alert">{error}</div>}
          <div className="form-actions">
            <button type="button" className="secondary-button tag-manage-toggle" onClick={() => setManaging((current) => !current)}>{managing ? t.form.doneManaging : t.form.manageTags}</button>
            <button type="button" className="secondary-button" onClick={() => setOpen(false)}>{t.form.cancel}</button>
            <button type="button" className="primary-button" onClick={confirm}>{t.form.confirmTags}</button>
          </div>
        </div>
      </div>,
      document.body,
    )}
  </div>;
}
