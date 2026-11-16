import { render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import { backend } from './lib/backend';
import { getTranslation } from './i18n/translations';
import { CapabilitySuggestionBanner, CapabilitySuggestionModal, PlanReviewModal } from './McpReview';

const userEvent = async () => (await import('@testing-library/user-event')).default.setup();

describe('PlanReviewModal', () => {
  const plan = { request_id: 'plan-1', kind: 'prepare_project_env', project_path: 'C:/demo', secret_ids: [], version: 2, created_at: 't' };
  const preview = { requestId: 'plan-1', projectPath: 'C:/demo', version: 2, errors: [], entries: [
    { key: 'API_KEY', secretName: 'OpenRouter 主账号', kind: 'api_key' as const, display: '••••••••', conflict: true, warnings: [] },
    { key: 'BASE_URL', secretName: 'OpenRouter 主账号', kind: 'base_url' as const, display: 'https://openrouter.ai/api/v1', conflict: false, warnings: [] },
    { key: 'LLM_MODEL', secretName: 'OpenRouter 主账号', kind: 'model' as const, display: 'google/gemini', capability: 'chat', conflict: true, warnings: ['model_not_in_synced_list'] },
  ] };
  const t = getTranslation('en-US');
  afterEach(() => { vi.restoreAllMocks(); });

  it('renders masked keys, plain model ids, statuses and only offers a toggle on conflicts', async () => {
    vi.spyOn(backend, 'planPreview').mockResolvedValue(preview);
    render(<PlanReviewModal plan={plan} t={t} onClose={() => undefined} onDone={() => undefined} />);
    expect(await screen.findByText('••••••••')).toBeInTheDocument();
    expect(screen.getByText('https://openrouter.ai/api/v1')).toBeInTheDocument();
    expect(screen.getByText('google/gemini')).toBeInTheDocument();
    expect(screen.getAllByText('OpenRouter 主账号')).toHaveLength(3);
    expect(screen.getByText('Model · chat')).toBeInTheDocument();
    expect(screen.getByText('Model is not in the synced list of this key')).toBeInTheDocument();
    expect(screen.getAllByText('Overwrites existing')).toHaveLength(2);
    expect(screen.getByText('New')).toBeInTheDocument();
    expect(screen.getByLabelText('Action API_KEY')).toHaveValue('overwrite');
    expect(screen.queryByLabelText('Action BASE_URL')).not.toBeInTheDocument();
  });

  it('approves with the skipped conflict keys and then lists written and skipped names', async () => {
    vi.spyOn(backend, 'planPreview').mockResolvedValue(preview);
    const confirm = vi.spyOn(backend, 'confirmPlan').mockResolvedValue(undefined);
    const onDone = vi.fn();
    const user = await userEvent();
    render(<PlanReviewModal plan={plan} t={t} onClose={() => undefined} onDone={onDone} />);
    await user.selectOptions(await screen.findByLabelText('Action LLM_MODEL'), 'skip');
    await user.click(screen.getByRole('button', { name: 'Approve' }));
    expect(confirm).toHaveBeenCalledWith('plan-1', true, ['LLM_MODEL']);
    expect(await screen.findByText(/Written: API_KEY, BASE_URL/)).toBeInTheDocument();
    expect(screen.getByText(/Skipped: LLM_MODEL/)).toBeInTheDocument();
    expect(onDone).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Close' }));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('sends an empty skip list by default and shows failures without closing', async () => {
    vi.spyOn(backend, 'planPreview').mockResolvedValue(preview);
    const confirm = vi.spyOn(backend, 'confirmPlan').mockRejectedValue('env_tracked: .env is already tracked by Git');
    const user = await userEvent();
    render(<PlanReviewModal plan={plan} t={t} onClose={() => undefined} onDone={() => undefined} />);
    await user.click(await screen.findByRole('button', { name: 'Approve' }));
    expect(confirm).toHaveBeenCalledWith('plan-1', true, []);
    expect(await screen.findByRole('alert')).toHaveTextContent('env_tracked');
    expect(screen.getByRole('button', { name: 'Approve' })).toBeEnabled();
  });

  it('disables approval and lists the errors when the preview has errors', async () => {
    vi.spyOn(backend, 'planPreview').mockResolvedValue({ ...preview, entries: [], errors: [{ code: 'env_conflict', message: 'FOO is bound to different values' }] });
    render(<PlanReviewModal plan={plan} t={t} onClose={() => undefined} onDone={() => undefined} />);
    expect(await screen.findByRole('alert')).toHaveTextContent('env_conflict: FOO is bound to different values');
    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled();
  });

  it('declines without skip keys and closes', async () => {
    vi.spyOn(backend, 'planPreview').mockResolvedValue(preview);
    const confirm = vi.spyOn(backend, 'confirmPlan').mockResolvedValue(undefined);
    const onDone = vi.fn();
    const user = await userEvent();
    render(<PlanReviewModal plan={plan} t={t} onClose={() => undefined} onDone={onDone} />);
    await user.click(await screen.findByRole('button', { name: 'Decline' }));
    expect(confirm).toHaveBeenCalledWith('plan-1', false);
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it('renders untrusted text as text, not markup', async () => {
    vi.spyOn(backend, 'planPreview').mockResolvedValue({ ...preview, entries: [{ key: 'M', secretName: '<img src=x onerror=alert(1)>', kind: 'model', display: '<b>bold</b>', capability: 'chat', conflict: false, warnings: [] }] });
    const { container } = render(<PlanReviewModal plan={plan} t={t} onClose={() => undefined} onDone={() => undefined} />);
    expect(await screen.findByText('<b>bold</b>')).toBeInTheDocument();
    expect(container.querySelector('img, b')).toBeNull();
  });
});

describe('CapabilitySuggestionModal', () => {
  const batch = { requestId: 'caps-1', providerId: 'openrouter', createdAt: 't', suggestions: [
    { modelId: 'vendor/model-a', capabilities: ['chat', 'vision'], evidenceUrl: 'https://docs.example/a', current: { capabilities: [], source: 'unknown' } },
    { modelId: 'vendor/model-b', capabilities: ['tts'] },
  ] };
  const t = getTranslation('en-US');
  afterEach(() => { vi.restoreAllMocks(); });

  it('shows the notice with the model count', () => {
    render(<CapabilitySuggestionBanner batch={batch} t={t} onReview={() => undefined} />);
    expect(screen.getByText('AI suggested capabilities for 2 models')).toBeInTheDocument();
  });

  it('accepts only the checked models and opens evidence only on request', async () => {
    const resolve = vi.spyOn(backend, 'resolveCapabilitySuggestions').mockResolvedValue(undefined);
    const open = vi.spyOn(backend, 'openExternalUrl').mockResolvedValue(undefined);
    const onDone = vi.fn();
    const user = await userEvent();
    render(<CapabilitySuggestionModal batch={batch} t={t} onClose={() => undefined} onDone={onDone} />);
    expect(screen.getByText('https://docs.example/a')).toBeInTheDocument();
    expect(open).not.toHaveBeenCalled();
    expect(screen.getByLabelText('vendor/model-a')).toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Open vendor/model-a' }));
    expect(open).toHaveBeenCalledWith('https://docs.example/a');
    await user.click(screen.getByLabelText('vendor/model-b'));
    await user.click(screen.getByRole('button', { name: 'Accept selected' }));
    expect(resolve).toHaveBeenCalledWith('caps-1', ['vendor/model-a']);
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it('rejects everything with an empty list and blocks accepting nothing', async () => {
    const resolve = vi.spyOn(backend, 'resolveCapabilitySuggestions').mockResolvedValue(undefined);
    const user = await userEvent();
    render(<CapabilitySuggestionModal batch={batch} t={t} onClose={() => undefined} onDone={() => undefined} />);
    await user.click(screen.getByLabelText('vendor/model-a'));
    await user.click(screen.getByLabelText('vendor/model-b'));
    expect(screen.getByRole('button', { name: 'Accept selected' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Reject all' }));
    expect(resolve).toHaveBeenCalledWith('caps-1', []);
  });

  it('shows a backend failure and keeps the modal open', async () => {
    vi.spyOn(backend, 'resolveCapabilitySuggestions').mockRejectedValue('vault is locked');
    const onDone = vi.fn();
    const user = await userEvent();
    render(<CapabilitySuggestionModal batch={batch} t={t} onClose={() => undefined} onDone={onDone} />);
    await user.click(screen.getByRole('button', { name: 'Accept selected' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('vault is locked');
    expect(onDone).not.toHaveBeenCalled();
  });
});
