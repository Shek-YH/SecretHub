import type { getTranslation } from '../i18n/translations';
import { PICKER_ORDER, type ItemKind } from './itemTypes';
import { TypeIcon } from './TypeIcon';

/** First step of "add": which kind of thing is being saved. */
export function ItemTypePicker({ t, onPick, onClose }: { t: ReturnType<typeof getTranslation>; onPick: (kind: ItemKind) => void; onClose: () => void }) {
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.items.pickerTitle}>
      <div className="secret-form credential-type-picker item-type-picker">
        <div className="form-heading">
          <div>
            <span className="section-kicker">NEW ITEM / TYPE</span>
            <h2>{t.items.pickerTitle}</h2>
            <p className="form-intro">{t.items.pickerIntro}</p>
          </div>
          <div className="form-heading-actions">
            <button type="button" className="close-button" onClick={onClose} aria-label={t.form.cancel}>×</button>
          </div>
        </div>
        <div className="credential-type-grid">
          {PICKER_ORDER.map((kind) => (
            <button type="button" className="credential-type-card" data-kind={kind} key={kind} onClick={() => onPick(kind)}>
              <strong><TypeIcon kind={kind} size={15} /> {t.items.types[kind].name}</strong>
              <span>{t.items.types[kind].hint}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
