import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App } from './App';
import { CaptureRequestBanner } from './AiRequests';
import { CaptureModal, captureMessage, formatCountdown } from './KeyCapture';
import { backend, type CaptureStatus } from './lib/backend';
import { getTranslation } from './i18n/translations';

const t = getTranslation('zh-CN');
const status = (over: Partial<CaptureStatus>): CaptureStatus => ({ state: 'waiting', providerId: 'deepseek', remainingSeconds: 150, secretId: null, validation: null, ...over });

afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

describe('capture helpers', () => {
  it('formats the countdown as m:ss and never goes negative', () => {
    expect(formatCountdown(180)).toBe('3:00');
    expect(formatCountdown(65)).toBe('1:05');
    expect(formatCountdown(-4)).toBe('0:00');
  });

  it('describes each state with a clear status text', () => {
    expect(captureMessage(t, status({}))).toBe(t.capture.waiting);
    expect(captureMessage(t, status({ state: 'timeout' }))).toBe(t.capture.timeout);
    expect(captureMessage(t, status({ state: 'cancelled' }))).toBe(t.capture.cancelled);
    expect(captureMessage(t, status({ state: 'captured', secretId: 's1' }))).toBe(t.capture.checking);
    expect(captureMessage(t, status({ state: 'captured', secretId: 's1', validation: 'valid' }))).toBe(t.capture.valid);
    expect(captureMessage(t, status({ state: 'captured', secretId: 's1', validation: 'unauthorized' }))).toBe(t.capture.badKey);
    expect(captureMessage(t, status({ state: 'captured', secretId: 's1', validation: 'network_error' }))).toContain('network_error');
  });
});

describe('CaptureModal', () => {
  it('counts down while waiting and cancels through the backend', async () => {
    const user = userEvent.setup();
    vi.spyOn(backend, 'captureStatus').mockResolvedValue(status({ remainingSeconds: 125 }));
    const cancel = vi.spyOn(backend, 'captureCancel').mockResolvedValue(status({ state: 'cancelled', remainingSeconds: 0 }));
    render(<CaptureModal t={t} providerName="DeepSeek" onCaptured={() => undefined} onClose={() => undefined} />);
    expect(await screen.findByText('2:05')).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent(t.capture.waiting);
    expect(screen.getByText(t.capture.privacy)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: t.capture.cancel }));
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(await screen.findByText(t.capture.cancelled)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: t.capture.done })).toBeInTheDocument();
  });

  it('reports the captured secret id once, shows the check result and never needs the key', async () => {
    const onCaptured = vi.fn();
    vi.spyOn(backend, 'captureStatus').mockResolvedValue(status({ state: 'captured', secretId: 'secret-1', remainingSeconds: 0, validation: 'unauthorized' }));
    render(<CaptureModal t={t} providerName="DeepSeek" onCaptured={onCaptured} onClose={() => undefined} />);
    expect(await screen.findByText(t.capture.badKey)).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(onCaptured).toHaveBeenCalledTimes(1);
    expect(onCaptured).toHaveBeenCalledWith('secret-1');
    expect(screen.queryByRole('button', { name: t.capture.cancel })).not.toBeInTheDocument();
  });

  it('shows the timeout state with a done button', async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    vi.spyOn(backend, 'captureStatus').mockResolvedValue(status({ state: 'timeout', remainingSeconds: 0 }));
    render(<CaptureModal t={t} providerName="DeepSeek" onCaptured={() => undefined} onClose={onClose} />);
    await user.click(await screen.findByRole('button', { name: t.capture.done }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('capture entry points', () => {
  it('renders an AI request as text and reports start and decline', async () => {
    const user = userEvent.setup();
    const onStart = vi.fn();
    const onDecline = vi.fn();
    render(<CaptureRequestBanner t={t} request={{ requestId: 'capture-1', providerId: 'x', providerName: '<b>Evil</b>', createdAt: 'now' }} onStart={onStart} onDecline={onDecline} />);
    expect(screen.getByText(t.capture.requestTitle.replace('{provider}', '<b>Evil</b>'))).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: t.capture.start }));
    await user.click(screen.getByRole('button', { name: t.capture.decline }));
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(onDecline).toHaveBeenCalledTimes(1);
  });

  function mockDesktop(requests: Array<{ requestId: string; providerId: string; providerName: string; createdAt: string }>) {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked: true });
    vi.spyOn(backend, 'list').mockResolvedValue([]);
    vi.spyOn(backend, 'audit').mockResolvedValue([]);
    vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingSyncRequests').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCaptureRequests').mockResolvedValue(requests);
    vi.spyOn(backend, 'catalogEffective').mockRejectedValue('skip');
    vi.spyOn(backend, 'models').mockResolvedValue([]);
    vi.spyOn(backend, 'capabilityModels').mockResolvedValue([]);
    vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
    vi.spyOn(backend, 'captureStatus').mockResolvedValue(status({}));
  }

  it('starts a capture from a provider card and opens the countdown dialog', async () => {
    mockDesktop([]);
    const start = vi.spyOn(backend, 'captureStart').mockResolvedValue(status({}));
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: '服务商' }));
    const buttons = await screen.findAllByRole('button', { name: t.capture.button });
    expect(buttons.length).toBeGreaterThan(5);
    await user.click(buttons[0]);
    await waitFor(() => expect(start).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole('dialog', { name: t.capture.title })).toBeInTheDocument();
  });

  it('shows an AI capture request only after unlock and starts it on the user\'s click', async () => {
    mockDesktop([{ requestId: 'capture-9', providerId: 'deepseek', providerName: 'DeepSeek', createdAt: 'now' }]);
    const decide = vi.spyOn(backend, 'captureDecision').mockResolvedValue(status({}));
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: t.capture.start }));
    await waitFor(() => expect(decide).toHaveBeenCalledWith('capture-9', true));
    expect(await screen.findByRole('dialog', { name: t.capture.title })).toBeInTheDocument();
  });

  it('declining sends false and opens no dialog', async () => {
    mockDesktop([{ requestId: 'capture-9', providerId: 'deepseek', providerName: 'DeepSeek', createdAt: 'now' }]);
    const decide = vi.spyOn(backend, 'captureDecision').mockResolvedValue(status({ state: 'idle' }));
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: t.capture.decline }));
    await waitFor(() => expect(decide).toHaveBeenCalledWith('capture-9', false));
    expect(screen.queryByRole('dialog', { name: t.capture.title })).not.toBeInTheDocument();
  });
});
