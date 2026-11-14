import { useState } from 'react';
import { ItemFormShell, SecretInput } from './ItemFormShell';
import { labelOf, type ItemFormProps } from './formTypes';
import { cardChecksumWarning, formatExpiry, groupCardNumber, validExpiry } from './itemTypes';

/** Bank card: the number is grouped by four while typing; a bad checksum only warns. */
export function BankCardForm({ t, knownTags, initial, initialFields = {}, onBack, onClose, onSave }: ItemFormProps) {
  const editing = Boolean(initial);
  const [name, setName] = useState(initial?.name ?? '');
  const [bank, setBank] = useState(initialFields.bank ?? '');
  const [cardNumber, setCardNumber] = useState('');
  const [holder, setHolder] = useState(initialFields.holder ?? '');
  const [expiry, setExpiry] = useState(initialFields.expiry ?? '');
  const [cvv, setCvv] = useState('');
  const [pin, setPin] = useState('');
  const [branch, setBranch] = useState(initialFields.branch ?? '');
  const [notes, setNotes] = useState(initialFields.notes ?? '');
  const [cleared, setCleared] = useState<string[]>([]);
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [error, setError] = useState('');
  const label = (key: string) => labelOf(t, 'bank_card', key);
  const toggleClear = (key: string) => setCleared((current) => (current.includes(key) ? current.filter((item) => item !== key) : [...current, key]));
  const digits = cardNumber.replace(/\s/g, '');

  async function submit() {
    setError('');
    const cardOk = digits.length >= 12 && digits.length <= 19;
    const problem = !name.trim() || !bank.trim() ? t.form.required
      : (!editing || digits) && !cardOk ? t.items.cardInvalid
      : expiry.trim() && !validExpiry(expiry.trim()) ? t.items.expiryInvalid
      : cvv && !/^\d{3,4}$/.test(cvv) ? t.items.cvvInvalid
      : pin && !/^\d{4,6}$/.test(pin) ? t.items.pinInvalid
      : '';
    if (problem) { setError(problem); return; }
    const fields: Record<string, string> = { bank: bank.trim() };
    if (digits) fields.card_number = digits;
    if (holder.trim() || editing) fields.holder = holder.trim();
    if (expiry.trim() || editing) fields.expiry = expiry.trim();
    if (branch.trim() || editing) fields.branch = branch.trim();
    if (notes.trim() || editing) fields.notes = notes;
    if (cvv && !cleared.includes('cvv')) fields.cvv = cvv;
    if (pin && !cleared.includes('pin')) fields.pin = pin;
    try {
      await onSave({ itemType: 'bank_card', name: name.trim(), fields, clear: editing ? cleared : [], tags, favorite });
      onClose();
    } catch (reason) { setError(String(reason)); }
  }

  return (
    <ItemFormShell t={t} kind="bank_card" editing={editing} name={name} onName={setName} tags={tags} onTags={setTags} knownTags={knownTags} favorite={favorite} onFavorite={setFavorite} error={error} onBack={onBack} onClose={onClose} onSubmit={submit}>
      <label>
        {label('bank')}
        <input required value={bank} onChange={(event) => setBank(event.target.value)} autoComplete="off" />
      </label>
      <label>
        {label('card_number')}
        <input value={cardNumber} onChange={(event) => setCardNumber(groupCardNumber(event.target.value))} inputMode="numeric" autoComplete="off" spellCheck={false} placeholder={editing ? t.items.keepUnchanged : '0000 0000 0000 0000'} />
      </label>
      {cardChecksumWarning(cardNumber) && <p className="luhn-warning" role="status">{t.items.cardChecksum}</p>}
      <label>
        {label('holder')}
        <input value={holder} onChange={(event) => setHolder(event.target.value)} autoComplete="off" />
      </label>
      <label>
        {label('expiry')}
        <input value={expiry} onChange={(event) => setExpiry(formatExpiry(event.target.value))} placeholder="MM/YY" inputMode="numeric" autoComplete="off" maxLength={5} />
      </label>
      <div className="form-two-col">
        <SecretInput t={t} label={label('cvv')} value={cvv} onChange={(value) => setCvv(value.replace(/\D/g, ''))} editing={editing} clearable cleared={cleared.includes('cvv')} onToggleClear={() => toggleClear('cvv')} inputMode="numeric" maxLength={4} />
        <SecretInput t={t} label={label('pin')} value={pin} onChange={(value) => setPin(value.replace(/\D/g, ''))} editing={editing} clearable cleared={cleared.includes('pin')} onToggleClear={() => toggleClear('pin')} inputMode="numeric" maxLength={6} />
      </div>
      <label>
        {label('branch')}
        <input value={branch} onChange={(event) => setBranch(event.target.value)} autoComplete="off" />
      </label>
      <label>
        {label('notes')}
        <textarea value={notes} onChange={(event) => setNotes(event.target.value)} rows={3} />
      </label>
    </ItemFormShell>
  );
}
