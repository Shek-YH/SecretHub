import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { InputWaitingBanner, SecureInputModal } from './SecureInput';
import { backend, type InputRequest } from './lib/backend';
import { getTranslation } from './i18n/translations';

const t = getTranslation('zh-CN');

const loginRequest: InputRequest = {
  requestId: 'input-1', createdAt: 'now', title: '后台 · 管理员账号', hint: '填好后点验证', itemType: 'login', name: '后台',
  fields: [
    { key: 'username', label: '用户名', type: 'text', value: 'admin', options: [], required: true },
    { key: 'password', label: '密码', type: 'password', value: '', options: [], required: true },
  ],
  verify: { type: 'login', host: 'example.test' },
};

const llmRequest: InputRequest = {
  requestId: 'input-2', createdAt: 'now', title: '模型 Key', hint: '', itemType: 'api_key', name: 'Demo', providerId: 'deepseek',
  fields: [
    { key: 'DEMO_KEY', label: 'API Key', type: 'password', value: '', options: [], required: true, role: 'api_key' },
    { key: 'DEMO_URL', label: '请求地址', type: 'text', value: 'https://api.example.test/v1', options: [], required: true, role: 'base_url' },
    { key: 'DEMO_MODEL', label: '模型', type: 'select', value: '', options: ['preset-model'], required: false, role: 'model' },
  ],
  verify: { type: 'llm', host: 'api.example.test', auth: 'bearer' },
};

afterEach(() => { vi.restoreAllMocks(); });

describe('SecureInputModal', () => {
  it('renders fields from the template with a masked password and the verify destination', () => {
    render(<SecureInputModal t={t} request={loginRequest} onSaved={() => undefined} onCancelled={() => undefined} />);
    expect(screen.getByRole('dialog', { name: loginRequest.title })).toBeInTheDocument();
    expect(screen.getByText(loginRequest.hint)).toBeInTheDocument();
    expect(screen.getByLabelText('用户名')).toHaveValue('admin');
    expect(screen.getByLabelText('密码')).toHaveAttribute('type', 'password');
    expect(screen.queryByRole('button', { name: /显示/ })).not.toBeInTheDocument();
    expect(screen.getByText(new RegExp(t.secureInput.verifyLogin.replace('{host}', 'example\\.test')))).toBeInTheDocument();
    expect(screen.getByRole('button', { name: t.secureInput.saveVerify })).toBeInTheDocument();
  });

  it('blocks submit on a missing required field without calling the backend', async () => {
    const submit = vi.spyOn(backend, 'inputSubmit');
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={loginRequest} onSaved={() => undefined} onCancelled={() => undefined} />);
    await user.click(screen.getByRole('button', { name: t.secureInput.saveVerify }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.secureInput.required.replace('{field}', '密码'));
    expect(submit).not.toHaveBeenCalled();
  });

  it('shows the reason on a failed verification and keeps what was typed', async () => {
    vi.spyOn(backend, 'inputSubmit').mockRejectedValue({ code: 'unauthorized', httpStatus: 401 });
    const onSaved = vi.fn();
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={loginRequest} onSaved={onSaved} onCancelled={() => undefined} />);
    await user.type(screen.getByLabelText('密码'), 'fake-pass-1');
    await user.click(screen.getByRole('button', { name: t.secureInput.saveVerify }));
    expect(await screen.findByRole('alert')).toHaveTextContent('用户名/密码或 Key 不正确（HTTP 401）');
    expect(screen.getByLabelText('密码')).toHaveValue('fake-pass-1');
    expect(screen.getByLabelText('用户名')).toHaveValue('admin');
    expect(onSaved).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: t.secureInput.saveVerify })).toBeEnabled();
  });

  it('asks to unlock when the backend reports a locked or auto-locked vault', async () => {
    const submit = vi.spyOn(backend, 'inputSubmit').mockRejectedValueOnce({ code: 'locked' }).mockRejectedValueOnce('vault auto-locked');
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={loginRequest} onSaved={() => undefined} onCancelled={() => undefined} />);
    await user.type(screen.getByLabelText('密码'), 'fake-pass-1');
    await user.click(screen.getByRole('button', { name: t.secureInput.saveVerify }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.secureInput.errors.locked);
    await user.click(screen.getByRole('button', { name: t.secureInput.saveVerify }));
    await waitFor(() => expect(submit).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.secureInput.errors.locked);
  });

  it('reports the saved item to the caller', async () => {
    const submit = vi.spyOn(backend, 'inputSubmit').mockResolvedValue({ itemId: 'item-9', itemType: 'login' });
    const onSaved = vi.fn();
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={loginRequest} onSaved={onSaved} onCancelled={() => undefined} />);
    await user.type(screen.getByLabelText('密码'), 'fake-pass-1');
    await user.click(screen.getByRole('button', { name: t.secureInput.saveVerify }));
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith({ itemId: 'item-9', itemType: 'login' }));
    expect(submit).toHaveBeenCalledWith('input-1', { username: 'admin', password: 'fake-pass-1' });
  });

  it('cancel tells the backend and closes the form', async () => {
    const cancel = vi.spyOn(backend, 'inputCancel').mockResolvedValue(undefined);
    const onCancelled = vi.fn();
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={loginRequest} onSaved={() => undefined} onCancelled={onCancelled} />);
    await user.click(screen.getByRole('button', { name: t.secureInput.cancel }));
    await waitFor(() => expect(onCancelled).toHaveBeenCalledTimes(1));
    expect(cancel).toHaveBeenCalledWith('input-1');
  });

  it('fetches the model list into the model suggestions', async () => {
    const fetchModels = vi.spyOn(backend, 'inputFetchModels').mockResolvedValue(['fake-model-a', 'fake-model-b']);
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={llmRequest} onSaved={() => undefined} onCancelled={() => undefined} />);
    expect(screen.getByRole('button', { name: t.secureInput.saveVerify })).toBeInTheDocument();
    await user.type(screen.getByLabelText('API Key'), 'fake-key');
    await user.click(screen.getByRole('button', { name: t.secureInput.fetchModels }));
    expect(await screen.findByRole('status')).toHaveTextContent(t.secureInput.modelsFetched.replace('{n}', '2'));
    expect(fetchModels).toHaveBeenCalledWith('input-2', { DEMO_KEY: 'fake-key', DEMO_URL: 'https://api.example.test/v1', DEMO_MODEL: '' });
    await user.click(screen.getByRole('combobox', { name: '模型' }));
    expect(await screen.findByRole('option', { name: 'fake-model-b' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'preset-model' })).toBeInTheDocument();
  });

  it('shows why fetching models failed and still allows typing a model', async () => {
    vi.spyOn(backend, 'inputFetchModels').mockRejectedValue({ code: 'http_error', httpStatus: 500 });
    const user = userEvent.setup();
    render(<SecureInputModal t={t} request={llmRequest} onSaved={() => undefined} onCancelled={() => undefined} />);
    await user.click(screen.getByRole('button', { name: t.secureInput.fetchModels }));
    expect(await screen.findByRole('alert')).toHaveTextContent('服务器返回错误（HTTP 500）');
    await user.type(screen.getByRole('combobox', { name: '模型' }), 'my-model');
    expect(screen.getByRole('combobox', { name: '模型' })).toHaveValue('my-model');
  });

  it('uses the plain save label when nothing is verified', () => {
    render(<SecureInputModal t={t} request={{ ...loginRequest, verify: { type: 'none' } }} onSaved={() => undefined} onCancelled={() => undefined} />);
    expect(screen.getByRole('button', { name: t.secureInput.save })).toBeInTheDocument();
  });
});

describe('InputWaitingBanner', () => {
  it('asks the user to unlock the vault first', () => {
    render(<InputWaitingBanner t={t} request={loginRequest} />);
    expect(screen.getAllByText(t.secureInput.waitingTitle.replace('{title}', loginRequest.title)).length).toBeGreaterThan(0);
  });
});
