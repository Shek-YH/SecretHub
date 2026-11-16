import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { AiConnectSettings } from './AiConnectSettings';
import { backend } from './lib/backend';
import { getTranslation } from './i18n/translations';

describe('AiConnectSettings', () => {
  beforeEach(() => { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; });
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

  it('shows per-item results after connecting', async () => {
    vi.spyOn(backend, 'aiConnectStatus')
      .mockResolvedValueOnce({ connectedVersion: null, appVersion: '0.1.0', sidecarFound: true })
      .mockResolvedValue({ connectedVersion: '0.1.0', appVersion: '0.1.0', sidecarFound: true });
    vi.spyOn(backend, 'aiConnect').mockResolvedValue([
      { id: 'claude-code', status: 'skipped', detail: 'PATH 里没有找到 claude 命令' },
      { id: 'cursor', status: 'connected', detail: 'x' },
      { id: 'skill:secrethub-import', status: 'skipped', detail: '已链接开发目录' },
    ]);
    render(<AiConnectSettings t={getTranslation('zh-CN')} />);
    expect(await screen.findByText('尚未连接')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: '连接 AI 工具' }));
    expect(await screen.findByText(/已跳过 · PATH 里没有找到 claude 命令/)).toBeInTheDocument();
    expect(screen.getByText(/已跳过 · 已链接开发目录/)).toBeInTheDocument();
    expect(screen.getByText('已连接（应用版本 0.1.0）')).toBeInTheDocument();
  });
});
