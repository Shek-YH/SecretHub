import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App } from './App';
import { MediaRequestBanner } from './AiRequests';
import { MediaLibrary } from './MediaLibrary';
import { allTags, filterAssets, formatBytes, formatDuration, orientationOf } from './mediaFormat';
import { backend, type MediaAsset, type MediaListing } from './lib/backend';
import { getTranslation } from './i18n/translations';

const t = getTranslation('zh-CN');
const asset = (id: string, over: Partial<MediaAsset> = {}): MediaAsset => ({ id, kind: 'image', filename: `${id}.png`, stored_path: `C:\\media\\${id}.png`, sha256: id, size_bytes: 2048, mime: 'image/png', width: 1920, height: 1080, duration_ms: null, description: '', tags: [], source: 'user', created_at: 1, ...over });
const assets = [
  asset('cat', { description: 'sleeping cat', tags: ['Pets'] }),
  asset('clip', { kind: 'video', filename: 'clip.mp4', width: 1080, height: 1920, duration_ms: 65000, size_bytes: 5 * 1024 * 1024, tags: ['pets', 'demo'] }),
  asset('voice', { kind: 'audio', filename: 'voice.mp3', width: null, height: null, duration_ms: null, source: 'ai' }),
  asset('prompt', { kind: 'text', filename: 'prompt.txt', width: null, height: null, mime: 'text/plain' }),
];
const listing = (over: Partial<MediaListing> = {}): MediaListing => ({ assets, count: assets.length, usage_bytes: 5 * 1024 * 1024 + 6144, ffprobe: true, ...over });

afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });
const useTauri = () => { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; };

describe('media formatting and filtering', () => {
  it('formats sizes, durations and orientation', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatDuration(65000)).toBe('1:05');
    expect(formatDuration(3_725_000)).toBe('1:02:05');
    expect(orientationOf(2, 1)).toBe('landscape');
    expect(orientationOf(1, 2)).toBe('portrait');
    expect(orientationOf(3, 3)).toBe('square');
    expect(orientationOf(null, 3)).toBeNull();
  });

  it('filters by kind, tag (case-insensitive) and search text, and lists unique tags', () => {
    const ids = (filters: Parameters<typeof filterAssets>[1]) => filterAssets(assets, filters).map((item) => item.id);
    expect(ids({ kind: 'all', tag: '', query: '' })).toEqual(['cat', 'clip', 'voice', 'prompt']);
    expect(ids({ kind: 'video', tag: '', query: '' })).toEqual(['clip']);
    expect(ids({ kind: 'all', tag: 'PETS', query: '' })).toEqual(['cat', 'clip']);
    expect(ids({ kind: 'all', tag: '', query: 'SLEEPING' })).toEqual(['cat']);
    expect(ids({ kind: 'image', tag: 'demo', query: '' })).toEqual([]);
    expect(allTags(assets)).toEqual(['demo', 'Pets']);
  });
});

describe('MediaLibrary page', () => {
  it('shows cards with size, resolution, orientation and duration plus the storage usage', async () => {
    useTauri();
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing());
    vi.spyOn(backend, 'mediaThumbnail').mockResolvedValue('data:image/png;base64,AAAA');
    render(<MediaLibrary t={t} />);
    const clip = await screen.findByRole('article', { name: 'clip.mp4' });
    expect(within(clip).getByText(/1080×1920 · 竖 · 5\.0 MB · 1:05/)).toBeInTheDocument();
    expect(within(screen.getByRole('article', { name: 'cat.png' })).getByText(/1920×1080 · 横 · 2\.0 KB/)).toBeInTheDocument();
    expect(within(screen.getByRole('article', { name: 'voice.mp3' })).getByText(/AI 添加/)).toBeInTheDocument();
    expect(screen.getByText('占用 5.0 MB · 4 个文件')).toBeInTheDocument();
    expect(screen.queryByText(t.media.ffmpegHint)).not.toBeInTheDocument();
  });

  it('filters by type, tag and search', async () => {
    useTauri();
    const user = userEvent.setup();
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing());
    vi.spyOn(backend, 'mediaThumbnail').mockResolvedValue(null);
    render(<MediaLibrary t={t} />);
    await screen.findByRole('article', { name: 'clip.mp4' });
    await user.click(screen.getByRole('button', { name: '视频' }));
    expect(screen.getAllByRole('article')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: '全部' }));
    await user.selectOptions(screen.getByLabelText(t.media.tagFilter), 'Pets');
    expect(screen.getAllByRole('article').map((card) => card.getAttribute('aria-label'))).toEqual(['cat.png', 'clip.mp4']);
    await user.type(screen.getByLabelText(t.media.search), 'sleeping');
    expect(screen.getAllByRole('article').map((card) => card.getAttribute('aria-label'))).toEqual(['cat.png']);
    await user.clear(screen.getByLabelText(t.media.search));
    await user.type(screen.getByLabelText(t.media.search), 'zzz-no-match');
    expect(screen.getByText(t.media.emptyFiltered)).toBeInTheDocument();
  });

  it('hints at ffmpeg only when ffprobe is missing and audio/video assets exist', async () => {
    useTauri();
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ ffprobe: false }));
    vi.spyOn(backend, 'mediaThumbnail').mockResolvedValue(null);
    const { unmount } = render(<MediaLibrary t={t} />);
    expect(await screen.findByText(t.media.ffmpegHint)).toBeInTheDocument();
    unmount();
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ ffprobe: false, assets: [assets[0]], count: 1 }));
    render(<MediaLibrary t={t} />);
    await screen.findByRole('article', { name: 'cat.png' });
    expect(screen.queryByText(t.media.ffmpegHint)).not.toBeInTheDocument();
  });

  it('renders untrusted names and descriptions as text and shows the empty state', async () => {
    useTauri();
    vi.spyOn(backend, 'mediaThumbnail').mockResolvedValue(null);
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ assets: [asset('x', { filename: '<img src=x onerror=alert(1)>.png', description: '<b>bold</b>' })], count: 1 }));
    const { container, unmount } = render(<MediaLibrary t={t} />);
    expect(await screen.findByText('<b>bold</b>')).toBeInTheDocument();
    expect(container.querySelector('b')).toBeNull();
    expect(container.querySelector('img[src="x"]')).toBeNull();
    unmount();
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ assets: [], count: 0, usage_bytes: 0 }));
    render(<MediaLibrary t={t} />);
    expect(await screen.findByText(t.media.empty)).toBeInTheDocument();
  });

  it('imports through the file dialog and reports duplicates and failures', async () => {
    useTauri();
    const user = userEvent.setup();
    const list = vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ assets: [], count: 0 }));
    vi.spyOn(backend, 'mediaChooseFiles').mockResolvedValue(['C:\\a.png', 'C:\\b.png', 'C:\\dir']);
    const imported = vi.spyOn(backend, 'mediaImport').mockResolvedValue([
      { path: 'C:\\a.png', ok: true, id: 'asset-1', duplicate: false, error: null },
      { path: 'C:\\b.png', ok: true, id: 'asset-1', duplicate: true, error: null },
      { path: 'C:\\dir', ok: false, id: null, duplicate: false, error: 'not_a_file: the path is not a regular file' },
    ]);
    render(<MediaLibrary t={t} />);
    await screen.findByText(t.media.empty);
    await user.click(screen.getByRole('button', { name: t.media.import }));
    await waitFor(() => expect(imported).toHaveBeenCalledWith(['C:\\a.png', 'C:\\b.png', 'C:\\dir']));
    expect(await screen.findByText(/已导入: 1 · 重复/)).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent('not_a_file');
    await waitFor(() => expect(list.mock.calls.length).toBeGreaterThan(1));
  });

  it('creates text assets, edits description and tags, copies, reveals and deletes after confirmation', async () => {
    useTauri();
    const user = userEvent.setup();
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ assets: [assets[0]], count: 1 }));
    vi.spyOn(backend, 'mediaThumbnail').mockResolvedValue(null);
    vi.spyOn(backend, 'tagList').mockResolvedValue(['Pets']);
    const createText = vi.spyOn(backend, 'mediaCreateText').mockResolvedValue(assets[3]);
    const update = vi.spyOn(backend, 'mediaUpdate').mockResolvedValue(true);
    const copy = vi.spyOn(backend, 'mediaCopyPath').mockResolvedValue(undefined);
    const reveal = vi.spyOn(backend, 'mediaReveal').mockResolvedValue(undefined);
    const remove = vi.spyOn(backend, 'mediaDelete').mockResolvedValue(true);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    render(<MediaLibrary t={t} />);
    await screen.findByRole('article', { name: 'cat.png' });

    await user.click(screen.getByRole('button', { name: t.media.newText }));
    await user.type(screen.getByLabelText(t.media.textName), 'Shot list');
    await user.type(screen.getByLabelText(t.media.textContent), 'scene 1');
    await user.click(screen.getByRole('button', { name: t.media.create }));
    await waitFor(() => expect(createText).toHaveBeenCalledWith('Shot list', 'scene 1', '', []));

    await user.click(screen.getByRole('button', { name: t.media.edit }));
    const dialog = screen.getByRole('dialog', { name: t.media.edit });
    const description = within(dialog).getByLabelText(t.media.description);
    await user.clear(description);
    await user.type(description, 'new text');
    await user.click(within(dialog).getByRole('button', { name: t.media.save }));
    await waitFor(() => expect(update).toHaveBeenCalledWith('cat', 'new text', ['Pets']));

    await user.click(screen.getByRole('button', { name: t.media.copyPath }));
    await waitFor(() => expect(copy).toHaveBeenCalledWith('cat'));
    expect(await screen.findByText(t.media.copied)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: t.media.reveal }));
    await waitFor(() => expect(reveal).toHaveBeenCalledWith('cat'));

    await user.click(screen.getByRole('button', { name: t.media.delete }));
    expect(remove).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: t.media.delete }));
    await waitFor(() => expect(remove).toHaveBeenCalledWith('cat'));
    expect(confirm).toHaveBeenCalledTimes(2);
  });
});

describe('media in the app shell', () => {
  function mockDesktop(unlocked: boolean, requests: Awaited<ReturnType<typeof backend.pendingMediaRequests>> = []) {
    useTauri();
    vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked });
    vi.spyOn(backend, 'list').mockResolvedValue([]);
    vi.spyOn(backend, 'audit').mockResolvedValue([]);
    vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingSyncRequests').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCaptureRequests').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingMediaRequests').mockResolvedValue(requests);
    vi.spyOn(backend, 'catalogEffective').mockRejectedValue('skip');
    vi.spyOn(backend, 'models').mockResolvedValue([]);
    vi.spyOn(backend, 'capabilityModels').mockResolvedValue([]);
    vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
    vi.spyOn(backend, 'mediaList').mockResolvedValue(listing({ assets: [assets[0]], count: 1 }));
    vi.spyOn(backend, 'mediaThumbnail').mockResolvedValue(null);
  }

  it('opens the library from the sidebar while the vault is locked', async () => {
    mockDesktop(false);
    const user = userEvent.setup();
    render(<App />);
    expect(await screen.findByRole('heading', { name: '解锁 SecretHub' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: new RegExp(t.media.nav) }));
    expect(await screen.findByRole('article', { name: 'cat.png' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '解锁 SecretHub' })).not.toBeInTheDocument();
  });

  it('shows an AI add request with filename, size and full path and sends the decision', async () => {
    const request = { requestId: 'media-7', path: 'C:\\Users\\me\\out\\result.mp4', filename: 'result.mp4', sizeBytes: 3 * 1024 * 1024, problem: null, description: 'render', tags: ['demo'], createdAt: 'now' };
    mockDesktop(false, [request]);
    const decide = vi.spyOn(backend, 'mediaDecision').mockResolvedValue(undefined);
    const user = userEvent.setup();
    render(<App />);
    expect(await screen.findByText('result.mp4')).toBeInTheDocument();
    expect(screen.getByText('C:\\Users\\me\\out\\result.mp4')).toBeInTheDocument();
    expect(screen.getByText(`${t.media.requestSize}: 3.0 MB`)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: t.media.accept }));
    await waitFor(() => expect(decide).toHaveBeenCalledWith('media-7', true));
  });

  it('disables accepting a request whose file is no longer usable', () => {
    const onAccept = vi.fn();
    render(<MediaRequestBanner t={t} request={{ requestId: 'media-8', path: 'C:\\gone.png', filename: 'gone.png', sizeBytes: null, problem: 'not_found', description: '', tags: [], createdAt: 'now' }} onAccept={onAccept} onDecline={() => undefined} />);
    expect(screen.getByRole('button', { name: t.media.accept })).toBeDisabled();
    expect(screen.getByRole('alert')).toHaveTextContent(t.media.problems.not_found);
  });
});
