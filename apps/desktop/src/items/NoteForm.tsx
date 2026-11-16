import { useState } from 'react';
import { ItemFormShell } from './ItemFormShell';
import { labelOf, type ItemFormProps } from './formTypes';

const BODY_LIMIT = 100_000;

/** Secure note. The body is the one secret that is shown in the form, and only while editing. */
export function NoteForm({ t, knownTags, initial, initialFields = {}, onBack, onClose, onSave }: ItemFormProps) {
  const editing = Boolean(initial);
  const [name, setName] = useState(initial?.name ?? '');
  const [body, setBody] = useState(initialFields.body ?? '');
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [error, setError] = useState('');

  async function submit() {
    setError('');
    if (!name.trim() || !body.trim()) { setError(t.form.required); return; }
    try {
      await onSave({ itemType: 'note', name: name.trim(), fields: { body }, clear: [], tags, favorite });
      onClose();
    } catch (reason) { setError(String(reason)); }
  }

  return (
    <ItemFormShell t={t} kind="note" editing={editing} nameLabel={t.items.noteTitle} name={name} onName={setName} tags={tags} onTags={setTags} knownTags={knownTags} favorite={favorite} onFavorite={setFavorite} error={error} onBack={onBack} onClose={onClose} onSubmit={submit}>
      <label>
        {labelOf(t, 'note', 'body')}
        <textarea required value={body} onChange={(event) => setBody(event.target.value)} rows={10} maxLength={BODY_LIMIT} spellCheck={false} />
      </label>
    </ItemFormShell>
  );
}
