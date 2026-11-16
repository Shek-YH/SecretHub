import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { CatalogSettings } from './CatalogSettings';
import { backend, type CatalogStatus } from './lib/backend';
import { getTranslation } from './i18n/translations';

const base: CatalogStatus = { serial: 2026101601, publishedAt: '2026-10-16T12:00:00Z', source: 'bundled', urlConfigured: false, autoUpdate: true, lastCheck: null };

describe('CatalogSettings', () => {
  beforeEach(() => { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; });
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

  it('shows the serial, bundled source and the not-configured state', async () => {
    vi.spyOn(backend, 'catalogStatus').mockResolvedValue(base);
    render(<CatalogSettings t={getTranslation('zh-CN')} locale="zh-CN" />);
    expect(await screen.findByText('2026101601')).toBeInTheDocument();
    expect(screen.getByText('内置')).toBeInTheDocument();
    expect(screen.getByText('未配置更新地址')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: '自动更新模型目录' })).toBeChecked();
  });

  it('toggles automatic updates through the backend', async () => {
    vi.spyOn(backend, 'catalogStatus').mockResolvedValue(base);
    const set = vi.spyOn(backend, 'catalogSetAutoUpdate').mockResolvedValue({ ...base, autoUpdate: false });
    render(<CatalogSettings t={getTranslation('zh-CN')} locale="zh-CN" />);
    const box = await screen.findByRole('checkbox', { name: '自动更新模型目录' });
    await waitFor(() => expect(box).toBeEnabled());
    await userEvent.setup().click(box);
    expect(set).toHaveBeenCalledWith(false);
    await waitFor(() => expect(box).not.toBeChecked());
  });

  it('reports each check result and refreshes the catalog only after an update', async () => {
    vi.spyOn(backend, 'catalogStatus').mockResolvedValue({ ...base, urlConfigured: true });
    const check = vi.spyOn(backend, 'catalogCheckNow');
    const onUpdated = vi.fn();
    render(<CatalogSettings t={getTranslation('zh-CN')} locale="zh-CN" onUpdated={onUpdated} />);
    const user = userEvent.setup();
    const button = await screen.findByRole('button', { name: '立即检查更新' });

    check.mockResolvedValueOnce({ result: 'up_to_date', detail: '', status: base });
    await user.click(button);
    expect(await screen.findByRole('status')).toHaveTextContent('已是最新');
    expect(onUpdated).not.toHaveBeenCalled();

    check.mockResolvedValueOnce({ result: 'updated', detail: '', status: { ...base, serial: 2026101700, source: 'remote' } });
    await user.click(button);
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('模型目录已更新'));
    expect(onUpdated).toHaveBeenCalledTimes(1);
    expect(screen.getByText('2026101700')).toBeInTheDocument();
    expect(screen.getByText('远程（已验签）')).toBeInTheDocument();

    check.mockResolvedValueOnce({ result: 'failed', detail: 'signature: <b>bad</b>', status: base });
    await user.click(button);
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('检查失败: signature: <b>bad</b>'));
    expect(document.querySelector('.catalog-settings b')).toBeNull();

    check.mockResolvedValueOnce({ result: 'not_configured', detail: '', status: base });
    await user.click(button);
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('未配置更新地址'));
  });

  it('is desktop-only outside the Tauri runtime', () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    render(<CatalogSettings t={getTranslation('en-US')} locale="en-US" />);
    expect(screen.getByText('Model catalog updates are available in the desktop app')).toBeInTheDocument();
  });
});
