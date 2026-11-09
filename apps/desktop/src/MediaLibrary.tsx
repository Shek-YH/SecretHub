import { useEffect, useState } from 'react';
import { backend, isTauriRuntime, type MediaAsset, type MediaImportItem, type MediaListing } from './lib/backend';
import type { getTranslation } from './i18n/translations';
import { TagField } from './TagPicker';
import { allTags, filterAssets, formatBytes, formatDuration, orientationOf } from './mediaFormat';

type T = ReturnType<typeof getTranslation>;

const PAGE_SIZE = 60;
const KIND_FILTERS = ['all', 'image', 'video', 'audio', 'text'] as const;
const GLYPH: Record<string, string> = { image: 'IMG', video: 'VID', audio: 'AUD', text: 'TXT', other: 'FILE' };

// Previews are fetched lazily over IPC (see media_thumbnail); remember them so re-renders stay cheap.
const thumbnails = new Map<string, string | null>();

function MediaThumb({ asset }: { asset: MediaAsset }) {
  const [src, setSrc] = useState<string | null>(thumbnails.get(asset.id) ?? null);
  useEffect(() => {
    if (asset.kind !== 'image' || !isTauriRuntime() || thumbnails.has(asset.id)) return;
    let cancelled = false;
    backend.mediaThumbnail(asset.id).then((url) => { thumbnails.set(asset.id, url); if (!cancelled) setSrc(url); }).catch(() => { thumbnails.set(asset.id, null); });
    return () => { cancelled = true; };
  }, [asset.id, asset.kind]);
  return <div className="media-thumb" aria-hidden="true">{src ? <img src={src} alt="" /> : <span className="media-glyph">{GLYPH[asset.kind] ?? GLYPH.other}</span>}</div>;
}

function MediaCard({ asset, t, onEdit, onCopy, onReveal, onDelete }: { asset: MediaAsset; t: T; onEdit: () => void; onCopy: () => void; onReveal: () => void; onDelete: () => void }) {
  const orientation = orientationOf(asset.width, asset.height);
  // Everything below comes from file names, descriptions and tags: rendered as text only.
  const meta = [
    asset.width && asset.height ? `${asset.width}×${asset.height}` : null,
    orientation ? t.media.orientation[orientation] : null,
    formatBytes(asset.size_bytes),
    asset.duration_ms != null ? formatDuration(asset.duration_ms) : null,
  ].filter(Boolean).join(' · ');
  return <article className="media-card" aria-label={asset.filename}>
    <MediaThumb asset={asset} />
    <strong className="media-name" title={asset.filename}>{asset.filename}</strong>
    <div className="media-meta">{t.media.kinds[asset.kind] ?? asset.kind} · {meta}{asset.source === 'ai' ? ` · ${t.media.fromAi}` : ''}</div>
    {asset.description && <p className="media-desc">{asset.description}</p>}
    {asset.tags.length > 0 && <div className="tag-row">{asset.tags.map((tag) => <span className="tag" key={tag}>{tag}</span>)}</div>}
    <div className="media-actions">
      <button type="button" className="secondary-button" onClick={onEdit}>{t.media.edit}</button>
      <button type="button" className="secondary-button" onClick={onCopy}>{t.media.copyPath}</button>
      <button type="button" className="secondary-button" onClick={onReveal}>{t.media.reveal}</button>
      <button type="button" className="delete-button" onClick={onDelete}>{t.media.delete}</button>
    </div>
  </article>;
}

function EditAssetModal({ asset, t, knownTags, onClose, onSaved }: { asset: MediaAsset; t: T; knownTags: string[]; onClose: () => void; onSaved: () => void }) {
  const [description, setDescription] = useState(asset.description);
  const [tags, setTags] = useState(asset.tags);
  const [error, setError] = useState('');
  async function save() {
    try { await backend.mediaUpdate(asset.id, description, tags); onSaved(); } catch (reason) { setError(String(reason)); }
  }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.media.edit}><div className="secret-form">
    <div className="form-heading"><div><span className="section-kicker">MEDIA / EDIT</span><h2>{asset.filename}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div>
    <label>{t.media.description}<textarea rows={4} maxLength={2000} value={description} onChange={(event) => setDescription(event.target.value)} /></label>
    <TagField t={t} value={tags} onChange={setTags} knownTags={knownTags} />
    {error && <div className="form-error" role="alert">{error}</div>}
    <div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.media.cancel}</button><button type="button" className="primary-button" onClick={() => void save()}>{t.media.save}</button></div>
  </div></div>;
}

function TextAssetModal({ t, onClose, onSaved }: { t: T; onClose: () => void; onSaved: () => void }) {
  const [name, setName] = useState('');
  const [content, setContent] = useState('');
  const [error, setError] = useState('');
  async function save() {
    if (!name.trim() || !content.trim()) { setError(t.form.required); return; }
    try { await backend.mediaCreateText(name.trim(), content, '', []); onSaved(); } catch (reason) { setError(String(reason)); }
  }
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={t.media.newText}><div className="secret-form">
    <div className="form-heading"><div><span className="section-kicker">MEDIA / TEXT</span><h2>{t.media.newText}</h2></div><button type="button" className="close-button" onClick={onClose}>×</button></div>
    <label>{t.media.textName}<input value={name} maxLength={120} onChange={(event) => setName(event.target.value)} /></label>
    <label>{t.media.textContent}<textarea rows={10} value={content} onChange={(event) => setContent(event.target.value)} /></label>
    {error && <div className="form-error" role="alert">{error}</div>}
    <div className="form-actions"><button type="button" className="secondary-button" onClick={onClose}>{t.media.cancel}</button><button type="button" className="primary-button" onClick={() => void save()}>{t.media.create}</button></div>
  </div></div>;
}

/**
 * Media library page. It never touches the vault key, so it works while the vault is locked.
 * Import uses the file dialog; drag-and-drop would need Tauri core event permissions the project
 * does not configure.
 */
export function MediaLibrary({ t, revision = 0 }: { t: T; revision?: number }) {
  const [listing, setListing] = useState<MediaListing | null>(null);
  const [kind, setKind] = useState<string>('all');
  const [tag, setTag] = useState('');
  const [query, setQuery] = useState('');
  const [visible, setVisible] = useState(PAGE_SIZE);
  const [editing, setEditing] = useState<MediaAsset | null>(null);
  const [showText, setShowText] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [reloads, setReloads] = useState(0);
  const reload = () => setReloads((value) => value + 1);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    backend.mediaList().then((value) => { if (!cancelled) setListing(value); }).catch((reason) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, [revision, reloads]);

  const assets = listing?.assets ?? [];
  const shown = filterAssets(assets, { kind, tag, query });
  const tags = allTags(assets);
  const needsFfprobe = listing != null && !listing.ffprobe && assets.some((asset) => asset.kind === 'video' || asset.kind === 'audio');

  async function importFiles() {
    if (!isTauriRuntime()) { setMessage(t.media.desktopOnly); return; }
    setBusy(true); setError(''); setMessage('');
    try {
      const paths = await backend.mediaChooseFiles();
      if (!paths || paths.length === 0) return;
      const results: MediaImportItem[] = await backend.mediaImport(paths);
      const failed = results.filter((item) => !item.ok);
      const duplicates = results.filter((item) => item.ok && item.duplicate).length;
      setMessage(`${t.media.importDone}: ${results.length - failed.length - duplicates}${duplicates ? ` · ${t.media.duplicates}: ${duplicates}` : ''}`);
      if (failed.length > 0) setError(failed.map((item) => `${item.path}: ${item.error ?? ''}`).join('\n'));
      reload();
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function act(action: () => Promise<unknown>, done?: string) {
    try { await action(); if (done) setMessage(done); setError(''); } catch (reason) { setError(String(reason)); }
  }

  return <section className="settings-panel media-panel" aria-label={t.media.title}>
    <div className="settings-heading"><span className="section-kicker">MEDIA / LOCAL ONLY</span><h2>{t.media.title}</h2><p>{t.media.subtitle}</p></div>
    <div className="media-toolbar">
      <button type="button" className="primary-button" disabled={busy} onClick={() => void importFiles()}>{t.media.import}</button>
      <button type="button" className="secondary-button" onClick={() => setShowText(true)}>{t.media.newText}</button>
      <div className="filter-row" role="group" aria-label={t.media.kindFilter}>{KIND_FILTERS.map((item) => <button type="button" key={item} className={kind === item ? 'filter-button active' : 'filter-button'} aria-pressed={kind === item} onClick={() => { setKind(item); setVisible(PAGE_SIZE); }}>{item === 'all' ? t.media.all : t.media.kinds[item]}</button>)}</div>
      <select aria-label={t.media.tagFilter} value={tag} onChange={(event) => { setTag(event.target.value); setVisible(PAGE_SIZE); }}><option value="">{t.media.allTags}</option>{tags.map((item) => <option key={item} value={item}>{item}</option>)}</select>
      <label className="search-box"><input value={query} onChange={(event) => { setQuery(event.target.value); setVisible(PAGE_SIZE); }} placeholder={t.media.search} aria-label={t.media.search} /></label>
      <span className="media-usage">{t.media.usage.replace('{size}', formatBytes(listing?.usage_bytes ?? 0)).replace('{count}', String(listing?.count ?? 0))}</span>
    </div>
    {needsFfprobe && <div className="cap-hint">{t.media.ffmpegHint}</div>}
    {message && <div className="form-success">{message}</div>}
    {error && <div className="form-error media-error" role="alert">{error}</div>}
    {shown.length === 0 ? <div className="empty-state"><strong>{assets.length === 0 ? t.media.empty : t.media.emptyFiltered}</strong></div> : <div className="media-grid">{shown.slice(0, visible).map((asset) => <MediaCard key={asset.id} asset={asset} t={t}
      onEdit={() => setEditing(asset)}
      onCopy={() => void act(() => backend.mediaCopyPath(asset.id), t.media.copied)}
      onReveal={() => void act(() => backend.mediaReveal(asset.id))}
      onDelete={() => { if (window.confirm(t.media.deleteConfirm)) void act(async () => { await backend.mediaDelete(asset.id); thumbnails.delete(asset.id); reload(); }); }} />)}</div>}
    {shown.length > visible && <button type="button" className="secondary-button" onClick={() => setVisible((value) => value + PAGE_SIZE)}>{t.media.showMore} ({shown.length - visible})</button>}
    {editing && <EditAssetModal asset={editing} t={t} knownTags={tags} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); reload(); }} />}
    {showText && <TextAssetModal t={t} onClose={() => setShowText(false)} onSaved={() => { setShowText(false); reload(); }} />}
  </section>;
}
