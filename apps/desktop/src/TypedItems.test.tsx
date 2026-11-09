import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App, ProfileSecretPicker } from './App';
import { getTranslation } from './i18n/translations';
import { AUTO_LOCK_EVENT, backend, type BackendSecret, type BackendSettings } from './lib/backend';

const row = (patch: Partial<BackendSecret>): BackendSecret => ({
  id: 'x', name: 'x', provider_id: 'personal', env_key: '', description: '', tags: [], status: 'unknown', value_type: 'login', category: 'personal', scope: 'global', favorite: false, archived: false,
  model_id: '', model_env_key: '', endpoint_url: '', endpoint_env_key: '', models_synced_at: 0, models_sync_status: '', created_at: 1, updated_at: Math.floor(Date.now() / 1000) - 60, validated_at: 0, subtitle: '', dev_use: false, ...patch,
});

const ROWS: BackendSecret[] = [
  row({ id: 'api', name: 'Fake API', provider_id: 'deepseek', env_key: 'DEEPSEEK_API_KEY', value_type: 'api_key', category: 'ai', status: 'valid', model_id: 'deepseek-v4-pro', endpoint_url: 'https://api.deepseek.com', endpoint_env_key: 'DEEPSEEK_BASE_URL', subtitle: '' }),
  row({ id: 'dev', name: 'Fake Dev Secret', provider_id: 'generic', env_key: 'FAKE_TOKEN', value_type: 'dev_secret', category: 'other', subtitle: 'FAKE_TOKEN' }),
  row({ id: 'old-token', name: 'Old Token', provider_id: 'generic', env_key: '', value_type: 'token', category: 'other' }),
  row({ id: 'login', name: 'Fake Login', value_type: 'login', subtitle: 'accounts.example.com' }),
  row({ id: 'login-dev', name: 'Fake Dev Login', value_type: 'login', subtitle: 'dev.example.com', env_key: 'DOUYIN_TEST', dev_use: true }),
  row({ id: 'old-password', name: 'Old Password', provider_id: 'generic', value_type: 'password' }),
  row({ id: 'card', name: 'Fake Card', value_type: 'bank_card', subtitle: 'Fake Bank · 尾号 1111' }),
  row({ id: 'note', name: 'Fake Note', value_type: 'note', subtitle: '' }),
];

const settings: BackendSettings = { locale: 'zh-CN', theme: 'dark', auto_lock_minutes: 15, clipboard_timeout_seconds: 30, require_auth_before_export: true, require_auth_before_reveal: true };

function mockDesktop(rows: BackendSecret[] = ROWS) {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked: true });
  vi.spyOn(backend, 'list').mockResolvedValue(rows);
  vi.spyOn(backend, 'audit').mockResolvedValue([]);
  vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
  vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
  vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
  vi.spyOn(backend, 'models').mockResolvedValue([]);
  vi.spyOn(backend, 'capabilityModels').mockResolvedValue([]);
  vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
  return {
    publicFields: vi.spyOn(backend, 'itemPublicFields').mockResolvedValue({ username: 'fake-user', website: 'https://accounts.example.com' }),
    secretFields: vi.spyOn(backend, 'itemSecretFields').mockResolvedValue(['password']),
    noteBody: vi.spyOn(backend, 'itemNoteBody').mockResolvedValue('fake note body'),
    create: vi.spyOn(backend, 'itemCreate').mockResolvedValue('new-id'),
    update: vi.spyOn(backend, 'itemUpdate').mockResolvedValue(undefined),
    changePassword: vi.spyOn(backend, 'changePassword').mockResolvedValue(undefined),
    secretCreate: vi.spyOn(backend, 'create').mockResolvedValue('dev-id'),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

const listPanel = () => screen.getByRole('region', { name: '凭据' });
const rowNames = () => Array.from(listPanel().querySelectorAll('.secret-row strong')).map((node) => node.textContent);

async function openList() {
  mockDesktop();
  render(<App />);
  await screen.findByText('Fake Login');
  return userEvent.setup();
}

describe('list rows and navigation', () => {
  it('shows a type icon, the name and the plaintext subtitle on every row', async () => {
    await openList();
    const loginRow = screen.getByText('Fake Login').closest('.secret-row') as HTMLElement;
    expect(loginRow.querySelector('svg[data-kind="login"]')).not.toBeNull();
    expect(within(loginRow).getByText('accounts.example.com')).toBeInTheDocument();
    const cardRow = screen.getByText('Fake Card').closest('.secret-row') as HTMLElement;
    expect(cardRow.querySelector('svg[data-kind="bank_card"]')).not.toBeNull();
    expect(within(cardRow).getByText('Fake Bank · 尾号 1111')).toBeInTheDocument();
    expect((screen.getByText('Fake Note').closest('.secret-row') as HTMLElement).querySelector('svg[data-kind="note"]')).not.toBeNull();
    expect((screen.getByText('Fake API').closest('.secret-row') as HTMLElement).querySelector('svg[data-kind="api_key"]')).not.toBeNull();
    expect(within(screen.getByText('Fake Dev Secret').closest('.secret-row') as HTMLElement).getByText('FAKE_TOKEN')).toBeInTheDocument();
  });

  it('filters by the five groups and counts old types into them', async () => {
    const user = await openList();
    const nav = screen.getByRole('navigation', { name: 'Primary navigation' });
    expect(rowNames()).toHaveLength(8);
    await user.click(within(nav).getByRole('button', { name: /API key/ }));
    expect(rowNames()).toEqual(['Fake API']);
    await user.click(within(nav).getByRole('button', { name: /开发密钥/ }));
    expect(rowNames()).toEqual(['Fake Dev Secret', 'Old Token']);
    await user.click(within(nav).getByRole('button', { name: /账号密码/ }));
    expect(rowNames()).toEqual(['Fake Login', 'Fake Dev Login', 'Old Password']);
    await user.click(within(nav).getByRole('button', { name: /银行卡/ }));
    expect(rowNames()).toEqual(['Fake Card']);
    await user.click(within(nav).getByRole('button', { name: /安全笔记/ }));
    expect(rowNames()).toEqual(['Fake Note']);
    await user.click(within(nav).getByRole('button', { name: /全部/ }));
    expect(rowNames()).toHaveLength(8);
  });

  it('has the five English group names', async () => {
    mockDesktop();
    localStorage.setItem('secrethub-locale', 'en-US');
    try {
      render(<App />);
      await screen.findByText('Fake Login');
      const nav = screen.getByRole('navigation', { name: 'Primary navigation' });
      for (const name of ['All', 'API key', 'Dev secrets', 'Logins', 'Bank cards', 'Secure notes']) expect(within(nav).getByRole('button', { name: new RegExp(name) })).toBeInTheDocument();
    } finally { localStorage.removeItem('secrethub-locale'); }
  });
});

describe('export selection', () => {
  it('shows no checkbox for personal items and keeps them out of select-all', async () => {
    const user = await openList();
    const checkboxFor = (name: string) => screen.queryByRole('checkbox', { name: `导出 ${name}` });
    for (const name of ['Fake API', 'Fake Dev Secret', 'Old Token', 'Fake Dev Login']) expect(checkboxFor(name), name).toBeInTheDocument();
    for (const name of ['Fake Login', 'Fake Card', 'Fake Note', 'Old Password']) expect(checkboxFor(name), name).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '全选当前结果' }));
    expect(screen.getByText(/^4 已选择/)).toBeInTheDocument();
    for (const name of ['Fake API', 'Fake Dev Secret', 'Old Token', 'Fake Dev Login']) expect(checkboxFor(name)).toBeChecked();
    await user.click(screen.getByRole('button', { name: '取消全选' }));
    expect(screen.queryByText(/已选择/)).not.toBeInTheDocument();
  });

  it('disables select-all when only personal items are listed', async () => {
    const user = await openList();
    await user.click(within(screen.getByRole('navigation', { name: 'Primary navigation' })).getByRole('button', { name: /银行卡/ }));
    expect(screen.getByRole('button', { name: '全选当前结果' })).toBeDisabled();
    expect(screen.queryAllByRole('checkbox', { name: /^导出 / })).toHaveLength(0);
  });

  it('offers only exportable credentials when picking profile members', async () => {
    const secrets = ROWS.map((item) => ({ id: item.id, name: item.name, envKey: item.env_key, provider: item.provider_id, tags: [], status: 'unknown', updated: '', valueType: item.value_type, devUse: item.dev_use }));
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(<ProfileSecretPicker t={getTranslation('zh-CN')} secrets={secrets} selectedIds={[]} onClose={() => undefined} onConfirm={onConfirm} />);
    const picker = screen.getByRole('dialog', { name: '选择要加入组合的凭据' });
    expect(within(picker).getAllByRole('checkbox').map((box) => box.closest('label')?.querySelector('strong')?.textContent)).toEqual(['Fake API', 'Fake Dev Secret', 'Old Token', 'Fake Dev Login']);
    await user.click(within(picker).getByRole('button', { name: '确定选择' }));
    expect(onConfirm).toHaveBeenCalledWith([]);
  });
});

describe('detail and edit flow', () => {
  it('shows the typed detail for a login without development tools', async () => {
    const calls = mockDesktop();
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByText('Fake Login'));
    const detail = screen.getByRole('region', { name: '凭据详情' });
    expect(await within(detail).findByText('fake-user')).toBeInTheDocument();
    expect(calls.publicFields).toHaveBeenCalledWith('login');
    expect(within(detail).getByRole('button', { name: '显示 密码' })).toBeInTheDocument();
    for (const name of ['验证', '复制全部内容', 'Reveal']) expect(within(detail).queryByRole('button', { name })).not.toBeInTheDocument();
    expect(calls.noteBody).not.toHaveBeenCalled();
  });

  it('keeps the old developer detail (copy all, reveal) for dev secrets but without validation', async () => {
    mockDesktop();
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByText('Fake Dev Secret'));
    const detail = screen.getByRole('region', { name: '凭据详情' });
    expect(within(detail).getByRole('button', { name: '复制全部内容' })).toBeInTheDocument();
    expect(within(detail).getByRole('button', { name: 'Reveal' })).toBeInTheDocument();
    expect(within(detail).queryByRole('button', { name: '验证' })).not.toBeInTheDocument();
  });

  it('creates a login through the picker and sends the camelCase payload', async () => {
    const calls = mockDesktop();
    render(<App />);
    const user = userEvent.setup();
    await screen.findByText('Fake Login');
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(within(screen.getByRole('dialog', { name: '你要保存什么？' })).getByRole('button', { name: /账号密码/ }));
    const form = screen.getByRole('dialog', { name: '添加账号密码' });
    await user.type(within(form).getByLabelText('名称'), 'New login');
    await user.type(within(form).getByLabelText('账号'), 'fake-user');
    await user.type(within(form).getByLabelText('密码'), 'fake-password');
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(calls.create).toHaveBeenCalled());
    expect(calls.create).toHaveBeenCalledWith({ itemType: 'login', name: 'New login', fields: { username: 'fake-user', password: 'fake-password' }, tags: [], favorite: false, devUse: false, envKey: '' });
    expect(screen.queryByRole('dialog', { name: '添加账号密码' })).not.toBeInTheDocument();
  });

  it('creates a dev secret through the existing secret_create command', async () => {
    const calls = mockDesktop();
    render(<App />);
    const user = userEvent.setup();
    await screen.findByText('Fake Login');
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(within(screen.getByRole('dialog', { name: '你要保存什么？' })).getByRole('button', { name: /开发密钥/ }));
    const form = screen.getByRole('dialog', { name: '添加开发密钥' });
    await user.type(within(form).getByLabelText('名称'), 'DB url');
    await user.type(within(form).getByLabelText('值'), 'postgres://fake');
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(calls.secretCreate).toHaveBeenCalledWith(expect.objectContaining({ value_type: 'dev_secret', value: 'postgres://fake', name: 'DB url' })));
  });

  it('edits a login from its public fields only and updates through item_update', async () => {
    const calls = mockDesktop();
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByText('Fake Login'));
    const detail = screen.getByRole('region', { name: '凭据详情' });
    await within(detail).findByText('fake-user');
    await user.click(within(detail).getByRole('button', { name: '编辑' }));
    const form = await screen.findByRole('dialog', { name: '编辑账号密码' });
    expect(within(form).getByLabelText('账号')).toHaveValue('fake-user');
    expect(within(form).getByLabelText('密码')).toHaveValue('');
    expect(calls.noteBody).not.toHaveBeenCalled();
    await user.clear(within(form).getByLabelText('账号'));
    await user.type(within(form).getByLabelText('账号'), 'other-user');
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(calls.update).toHaveBeenCalled());
    const [id, request] = calls.update.mock.calls[0];
    expect(id).toBe('login');
    expect(request.fields).toEqual({ username: 'other-user', website: 'https://accounts.example.com', notes: '' });
    expect(request).toEqual(expect.objectContaining({ name: 'Fake Login', clear: [], devUse: false }));
  });

  it('loads the note body only when editing a note, and drops it when the vault auto-locks', async () => {
    const calls = mockDesktop();
    calls.publicFields.mockResolvedValue({});
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByText('Fake Note'));
    expect(calls.noteBody).not.toHaveBeenCalled();
    await user.click(await screen.findByRole('button', { name: '编辑' }));
    const form = await screen.findByRole('dialog', { name: '编辑安全笔记' });
    expect(calls.noteBody).toHaveBeenCalledWith('note');
    expect(within(form).getByLabelText('正文')).toHaveValue('fake note body');
    act(() => { window.dispatchEvent(new Event(AUTO_LOCK_EVENT)); });
    expect(screen.queryByRole('dialog', { name: '编辑安全笔记' })).not.toBeInTheDocument();
    expect(document.body.textContent).not.toContain('fake note body');
  });
});

describe('change master password', () => {
  async function openDialog() {
    const calls = mockDesktop();
    vi.spyOn(backend, 'settingsGet').mockResolvedValue(settings);
    vi.spyOn(backend, 'catalogStatus').mockResolvedValue({ serial: 1, publishedAt: '2026-01-01', source: 'bundled', urlConfigured: false, autoUpdate: true, lastCheck: null });
    render(<App />);
    const user = userEvent.setup();
    await screen.findByText('Fake Login');
    await user.click(screen.getByRole('button', { name: /设置/ }));
    await user.click(await screen.findByRole('button', { name: '修改主密码' }));
    return { calls, user, dialog: screen.getByRole('dialog', { name: '修改主密码' }) };
  }

  it('checks length and confirmation, recommends 12 characters and calls the backend', async () => {
    const { calls, user, dialog } = await openDialog();
    await user.type(within(dialog).getByLabelText('当前主密码'), 'old-password-1');
    await user.type(within(dialog).getByLabelText('新主密码'), 'short');
    await user.type(within(dialog).getByLabelText('确认新主密码'), 'short');
    await user.click(within(dialog).getByRole('button', { name: '修改主密码' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('至少 9 个字符');
    await user.clear(within(dialog).getByLabelText('新主密码'));
    await user.type(within(dialog).getByLabelText('新主密码'), 'nine-chars');
    expect(within(dialog).getByText('建议 12 位以上')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: '修改主密码' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('两次输入的 Password 不一致');
    expect(calls.changePassword).not.toHaveBeenCalled();
    await user.clear(within(dialog).getByLabelText('新主密码'));
    await user.type(within(dialog).getByLabelText('新主密码'), 'a-longer-new-password');
    expect(within(dialog).queryByText('建议 12 位以上')).not.toBeInTheDocument();
    await user.clear(within(dialog).getByLabelText('确认新主密码'));
    await user.type(within(dialog).getByLabelText('确认新主密码'), 'a-longer-new-password');
    await user.click(within(dialog).getByRole('button', { name: '修改主密码' }));
    await waitFor(() => expect(calls.changePassword).toHaveBeenCalledWith('old-password-1', 'a-longer-new-password'));
    expect(await within(dialog).findByRole('status')).toHaveTextContent('以前导出的备份仍需用旧密码恢复');
    expect(within(dialog).queryByLabelText('新主密码')).not.toBeInTheDocument();
  });

  it('keeps the dialog and shows the error when the backend refuses', async () => {
    const { calls, user, dialog } = await openDialog();
    calls.changePassword.mockRejectedValue('invalid master password');
    await user.type(within(dialog).getByLabelText('当前主密码'), 'wrong-password');
    await user.type(within(dialog).getByLabelText('新主密码'), 'a-longer-new-password');
    await user.type(within(dialog).getByLabelText('确认新主密码'), 'a-longer-new-password');
    await user.click(within(dialog).getByRole('button', { name: '修改主密码' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('invalid master password');
    expect(within(dialog).getByLabelText('新主密码')).toBeInTheDocument();
  });
});
