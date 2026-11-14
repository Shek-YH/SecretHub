import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { UpdateButton, UpdateSettings } from './UpdateControls';
import { backend } from './lib/backend';
import { getTranslation } from './i18n/translations';

let readyHandler: ((event: { payload: string }) => void) | undefined;
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, handler: (event: { payload: string }) => void) => { readyHandler = handler; return () => undefined; }),
}));

describe('update controls', () => {
  beforeEach(() => { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; readyHandler = undefined; });
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

  it('hides the update button until a download finishes, then installs on click', async () => {
    vi.spyOn(backend, 'updateStatus').mockResolvedValue({ readyVersion: null, currentVersion: '0.1.0' });
    const install = vi.spyOn(backend, 'updateInstall').mockResolvedValue(undefined);
    render(<UpdateButton t={getTranslation('zh-CN')} />);
    await vi.waitFor(() => expect(readyHandler).toBeDefined());
    expect(screen.queryByRole('button', { name: '更新' })).not.toBeInTheDocument();
    readyHandler?.({ payload: '0.2.0' });
    await userEvent.click(await screen.findByRole('button', { name: '更新' }));
    expect(install).toHaveBeenCalledTimes(1);
  });

  it('shows the button immediately when an update was already downloaded', async () => {
    vi.spyOn(backend, 'updateStatus').mockResolvedValue({ readyVersion: '0.2.0', currentVersion: '0.1.0' });
    render(<UpdateButton t={getTranslation('zh-CN')} />);
    expect(await screen.findByRole('button', { name: '更新' })).toBeInTheDocument();
  });

  it('manual check reports up-to-date, ready and failed results', async () => {
    vi.spyOn(backend, 'updateStatus').mockResolvedValue({ readyVersion: null, currentVersion: '0.1.0' });
    const check = vi.spyOn(backend, 'updateCheckNow')
      .mockResolvedValueOnce({ result: 'up_to_date', version: null, detail: '' })
      .mockResolvedValueOnce({ result: 'ready', version: '0.2.0', detail: '' })
      .mockResolvedValueOnce({ result: 'failed', version: null, detail: 'offline' });
    render(<UpdateSettings t={getTranslation('zh-CN')} />);
    expect(await screen.findByText('0.1.0')).toBeInTheDocument();
    const button = screen.getByRole('button', { name: '检查更新' });
    await userEvent.click(button);
    expect(await screen.findByText('已是最新版本')).toBeInTheDocument();
    await userEvent.click(button);
    expect(await screen.findByText(/新版本 0\.2\.0 已下载/)).toBeInTheDocument();
    await userEvent.click(button);
    expect(await screen.findByText('检查失败: offline')).toBeInTheDocument();
    expect(check).toHaveBeenCalledTimes(3);
  });
});
