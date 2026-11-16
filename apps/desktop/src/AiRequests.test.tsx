import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App } from './App';
import { AiImportNoticeBanner, OrganizeWaitingBanner, SyncRequestBanner } from './AiRequests';
import { backend } from './lib/backend';
import { getTranslation } from './i18n/translations';

const t = getTranslation('zh-CN');

describe('AI sync request banner', () => {
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

  it('renders the request as text with its key count', () => {
    render(<SyncRequestBanner t={t} request={{ requestId: 'sync-<b>1</b>', secretCount: 3, createdAt: 'now' }} />);
    expect(screen.getByText(t.ai.syncTitle)).toBeInTheDocument();
    expect(screen.getByText(`3 ${t.nav.secrets}`)).toBeInTheDocument();
    expect(screen.getByText(t.ai.syncBody)).toBeInTheDocument();
  });

  function mockBackend(unlocked: boolean) {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked });
    vi.spyOn(backend, 'list').mockResolvedValue([]);
    vi.spyOn(backend, 'audit').mockResolvedValue([]);
    vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingSyncRequests').mockResolvedValue([{ requestId: 'sync-1', secretCount: null, createdAt: 'now' }]);
    vi.spyOn(backend, 'catalogEffective').mockRejectedValue('skip');
    vi.spyOn(backend, 'models').mockResolvedValue([]);
    vi.spyOn(backend, 'capabilityModels').mockResolvedValue([]);
    vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
  }

  it('shows the waiting-for-unlock banner only while the vault is locked', async () => {
    mockBackend(false);
    const { unmount } = render(<App />);
    expect(await screen.findByText(t.ai.syncTitle)).toBeInTheDocument();
    unmount();
    mockBackend(true);
    render(<App />);
    await screen.findByText('凭据中心');
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByText(t.ai.syncTitle)).not.toBeInTheDocument();
  });
});

describe('AI import plan banners', () => {
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

  it('shows the waiting banner text-only with the item count', () => {
    render(<OrganizeWaitingBanner t={t} request={{ requestId: 'organize-<b>1</b>', itemCount: 4, createdAt: 'now' }} />);
    expect(screen.getByText(t.ai.organizeWaitingTitle)).toBeInTheDocument();
    expect(screen.getByText(`4 ${t.nav.secrets}`)).toBeInTheDocument();
  });

  it('summarises a notice with counts only and offers view, undo and dismiss', async () => {
    const handlers = { onView: vi.fn(), onUndo: vi.fn(), onDismiss: vi.fn() };
    render(<AiImportNoticeBanner t={t} notice={{ batchId: 'batch-1', createdAt: 1, createdGroups: 2, addedVars: 3, savedSeparately: 1, otherItems: 1 }} {...handlers} />);
    expect(screen.getByText('AI 导入了 7 条（新增组 2、补充变量 3、另存 1）')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: t.ai.noticeView }));
    await userEvent.click(screen.getByRole('button', { name: t.ai.noticeUndo }));
    await userEvent.click(screen.getByRole('button', { name: t.ai.noticeDismiss }));
    expect(handlers.onView).toHaveBeenCalledOnce();
    expect(handlers.onUndo).toHaveBeenCalledOnce();
    expect(handlers.onDismiss).toHaveBeenCalledOnce();
  });

  function mockBackend(unlocked: boolean) {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked });
    vi.spyOn(backend, 'list').mockResolvedValue([]);
    vi.spyOn(backend, 'audit').mockResolvedValue([]);
    vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingSyncRequests').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingOrganizeRequests').mockResolvedValue([{ requestId: 'organize-1', itemCount: 2, createdAt: 'now' }]);
    vi.spyOn(backend, 'catalogEffective').mockRejectedValue('skip');
    vi.spyOn(backend, 'models').mockResolvedValue([]);
    vi.spyOn(backend, 'capabilityModels').mockResolvedValue([]);
    vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
  }

  it('waits for the unlock when locked and shows the undoable notice once unlocked', async () => {
    mockBackend(false);
    vi.spyOn(backend, 'aiImportNotices').mockRejectedValue('locked');
    const { unmount } = render(<App />);
    expect(await screen.findByText(t.ai.organizeWaitingTitle)).toBeInTheDocument();
    unmount();

    mockBackend(true);
    const notices = [{ batchId: 'batch-9', createdAt: 1, createdGroups: 1, addedVars: 0, savedSeparately: 0, otherItems: 0 }];
    vi.spyOn(backend, 'aiImportNotices').mockResolvedValue(notices);
    const undo = vi.spyOn(backend, 'aiImportUndo').mockResolvedValue({ removedItems: 1, removedVariables: 0 });
    render(<App />);
    await userEvent.click(await screen.findByRole('button', { name: t.ai.noticeUndo }));
    expect(undo).toHaveBeenCalledWith('batch-9');
    expect(screen.queryByText(t.ai.organizeWaitingTitle)).not.toBeInTheDocument();
  });
});
