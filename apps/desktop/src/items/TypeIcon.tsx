import { itemGroup, type ItemKind } from './itemTypes';
import type { SecretMetadata } from '../secretTypes';

const PATHS: Record<ItemKind, string> = {
  // key
  api_key: 'M14.5 4a4.5 4.5 0 1 0 .9 8.9L17 14.5l1.5-.2.2-1.5 1.3-1.3V9.2h-2.2L16 7.4A4.5 4.5 0 0 0 14.5 4Zm-2.6 3.3a1.1 1.1 0 1 1 0 2.2 1.1 1.1 0 0 1 0-2.2ZM4 20l6.2-6.2 1.4 1.4L5.4 21.4Z',
  // braces
  dev_secret: 'M9 4H7.5A2.5 2.5 0 0 0 5 6.5V9a2 2 0 0 1-1.5 1.9v.2A2 2 0 0 1 5 13v2.5A2.5 2.5 0 0 0 7.5 18H9v-1.6H7.9A1 1 0 0 1 7 15.4V13a3 3 0 0 0-1.2-2.4A3 3 0 0 0 7 8.2V6.6a1 1 0 0 1 .9-1H9Zm6 0h1.5A2.5 2.5 0 0 1 19 6.5V9a2 2 0 0 0 1.5 1.9v.2A2 2 0 0 0 19 13v2.5a2.5 2.5 0 0 1-2.5 2.5H15v-1.6h1.1a1 1 0 0 0 .9-1V13a3 3 0 0 1 1.2-2.4A3 3 0 0 1 17 8.2V6.6a1 1 0 0 0-.9-1H15Z',
  // server stack
  env_group: 'M5 4h14a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Zm0 10h14a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1ZM6.5 6.2v1.6h1.6V6.2Zm0 10v1.6h1.6v-1.6Z',
  // person
  login: 'M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8Zm0 9.5c3.9 0 7 1.8 7 4V20H5v-2.5c0-2.2 3.1-4 7-4Z',
  // card
  bank_card: 'M4 6h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1Zm0 3v1.6h16V9Zm2.2 5.2v1.5h4.3v-1.5Z',
  // note
  note: 'M6 3h9l4 4v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Zm8 1.8V8h3.2ZM8 11v1.5h8V11Zm0 3.2v1.5h8v-1.5Z',
};

/** Small monochrome icon of an item type; decorative, so it is hidden from assistive technology. */
export function TypeIcon({ kind, size = 16 }: { kind: ItemKind; size?: number }) {
  return <svg className={`type-icon type-icon-${kind}`} data-kind={kind} width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={PATHS[kind]} fill="currentColor" /></svg>;
}

export function iconFor(secret: Pick<SecretMetadata, 'valueType' | 'envKey'>) { return <TypeIcon kind={itemGroup(secret)} />; }
