import type { getTranslation } from '../i18n/translations';
import type { SecretMetadata } from '../secretTypes';
import { fieldDef, fieldLabel, type PersonalKind } from './itemTypes';

type T = ReturnType<typeof getTranslation>;

/** What a personal-item form hands to the app; the app decides between create and update. */
export type ItemSavePayload = {
  itemType: PersonalKind;
  name: string;
  /** Typed values only. Secret fields the user left empty are absent, so they stay unchanged. */
  fields: Record<string, string>;
  /** Optional secret fields to empty (edit only). */
  clear: string[];
  tags: string[];
  favorite: boolean;
  devUse?: boolean;
  envKey?: string;
};

export type ItemFormProps = {
  t: T;
  knownTags: string[];
  initial: SecretMetadata | null;
  /** Non-secret fields (and the note body) of the item being edited. */
  initialFields?: Record<string, string>;
  onBack?: () => void;
  onClose: () => void;
  onSave: (payload: ItemSavePayload) => Promise<void>;
};

/** Label of a field in the current UI language, from the shared definitions. */
export function labelOf(t: T, type: PersonalKind | 'dev_secret', key: string): string {
  const def = fieldDef(type, key);
  return def ? fieldLabel(def, t.items.labelLocale) : key;
}

export const validWebsite = (value: string): boolean => {
  if (/\s/.test(value)) return false;
  try {
    const url = new URL(value.includes('://') ? value : `https://${value}`);
    return (url.protocol === 'https:' || url.protocol === 'http:') && (url.hostname.includes('.') || url.hostname === 'localhost');
  } catch { return false; }
};

export const validTotp = (value: string): boolean => /^[A-Z2-7]{8,}=*$/.test(value.replace(/[\s-]/g, '').toUpperCase());
