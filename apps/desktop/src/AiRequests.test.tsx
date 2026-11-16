import { render, screen } from '@testing-library/react';
import { vi } from 'vitest';
import { App } from './App';
import { SyncRequestBanner } from './AiRequests';
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
