import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App } from './App';
import { backend, type BackendSecret } from './lib/backend';
import { isSecretName, prefixedName } from './items/groupTemplates';

const FAKE_SECRET = 'fake-ssh-password-do-not-render';
const FAKE_KEY = 'FAKEFAKEFAKE-private-key-body';

const row = (patch: Partial<BackendSecret>): BackendSecret => ({
  id: 'x', name: 'x', provider_id: 'group', env_key: '', description: '', tags: [], status: 'unknown', value_type: 'env_group', category: 'server', scope: 'global', favorite: false, archived: false,
  model_id: '', model_env_key: '', endpoint_url: '', endpoint_env_key: '', models_synced_at: 0, models_sync_status: '', created_at: 1, updated_at: Math.floor(Date.now() / 1000) - 60, validated_at: 0, subtitle: '', dev_use: false, ai_values: true, ...patch,
});

const GROUP = row({ id: 'g1', name: 'Fake server', subtitle: '3 个变量', tags: ['prod'] });

function mockDesktop(rows: BackendSecret[] = [GROUP]) {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked: true });
  vi.spyOn(backend, 'list').mockResolvedValue(rows);
  vi.spyOn(backend, 'audit').mockResolvedValue([]);
  vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
  vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
  vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
  vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
  return {
    pub: vi.spyOn(backend, 'groupPublic').mockResolvedValue({
      variables: [
        { name: 'SSH_HOST', secret: false, value: '203.0.113.7' },
        { name: 'SSH_PASSWORD', secret: true },
        { name: 'SSH_PRIVATE_KEY', secret: true },
      ],
      hasNotes: true,
      aiValues: true,
    }),
    create: vi.spyOn(backend, 'groupCreate').mockResolvedValue('new-group'),
    update: vi.spyOn(backend, 'groupUpdate').mockResolvedValue(undefined),
    reveal: vi.spyOn(backend, 'groupVarReveal').mockResolvedValue(undefined),
    copy: vi.spyOn(backend, 'groupVarCopy').mockResolvedValue(undefined),
    notes: vi.spyOn(backend, 'groupNotes').mockResolvedValue('fake notes text'),
    chooseKey: vi.spyOn(backend, 'groupChooseKeyPath').mockResolvedValue('C:\\keys\\ssh_private_key'),
    exportKey: vi.spyOn(backend, 'groupExportKeyFile').mockResolvedValue(undefined),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

async function openApp() {
  render(<App />);
  await screen.findByText('Fake server');
  const user = userEvent.setup();
  await user.click(document.querySelector('.secret-row') as HTMLElement);
  return user;
}

describe('template helpers', () => {
  it('applies a prefix to template names and guesses secrets the way the backend does', () => {
    expect(prefixedName('YECAOYUN', 'SSH_HOST')).toBe('YECAOYUN_SSH_HOST');
    expect(prefixedName('YECAOYUN_', 'SSH_HOST')).toBe('YECAOYUN_SSH_HOST');
    expect(prefixedName('', 'SSH_HOST')).toBe('SSH_HOST');
    for (const name of ['SSH_PASSWORD', 'DB_PWD', 'BT_PANEL_API_TOKEN', 'SSH_PRIVATE_KEY', 'OSS_ACCESS_KEY_SECRET', 'OPENAI_API_KEY']) expect(isSecretName(name), name).toBe(true);
    for (const name of ['SSH_HOST', 'SSH_PORT', 'OSS_ACCESS_KEY_ID', 'SSH_PUBLIC_KEY', 'DB_NAME']) expect(isSecretName(name), name).toBe(false);
  });
});

describe('list, navigation and add flow', () => {
  it('shows the group with a type icon and a localized variable count, and filters by the new navigation entry', async () => {
    mockDesktop([GROUP, row({ id: 'k', name: 'Fake Key', value_type: 'api_key', provider_id: 'openai', env_key: 'OPENAI_API_KEY', category: 'ai' })]);
    const user = await openApp();
    const groupRow = screen.getAllByText('Fake server')[0].closest('.secret-row') as HTMLElement;
    expect(groupRow.querySelector('svg[data-kind="env_group"]')).not.toBeNull();
    expect(within(groupRow).getByText('3 个变量')).toBeInTheDocument();
    await user.click(within(screen.getByRole('navigation', { name: 'Primary navigation' })).getByRole('button', { name: /配置组$/ }));
    expect(Array.from(document.querySelectorAll('.secret-row strong')).map((node) => node.textContent)).toEqual(['Fake server']);
  });

  it('builds a server group from the template with a prefix and creates it with the right payload', async () => {
    const { create } = mockDesktop();
    const user = await openApp();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: /配置组（服务器/ }));
    const dialog = screen.getByRole('dialog');
    await user.type(within(dialog).getByLabelText('变量名前缀'), 'YECAOYUN');
    await user.type(within(dialog).getByPlaceholderText('必填'), 'Yecaoyun server');
    expect(within(dialog).getByLabelText('变量名 YECAOYUN_SSH_HOST')).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText('值 YECAOYUN_SSH_HOST'), '203.0.113.7');
    await user.type(within(dialog).getByLabelText('值 YECAOYUN_SSH_PASSWORD'), FAKE_SECRET);
    // The secret input is masked until the user toggles it.
    expect(within(dialog).getByLabelText('值 YECAOYUN_SSH_PASSWORD')).toHaveAttribute('type', 'password');
    await user.click(within(dialog).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1));
    const request = create.mock.calls[0][0];
    expect(request.category).toBe('server');
    expect(request.aiValues).toBe(true);
    // SSH_PORT keeps its template default 22; empty template rows are dropped.
    expect(request.variables).toEqual([
      { name: 'YECAOYUN_SSH_HOST', secret: false, value: '203.0.113.7' },
      { name: 'YECAOYUN_SSH_PORT', secret: false, value: '22' },
      { name: 'YECAOYUN_SSH_PASSWORD', secret: true, value: FAKE_SECRET },
    ]);
  });

  it('guesses the secret switch for a typed name and lets the user override it', async () => {
    const { create } = mockDesktop();
    const user = await openApp();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: /配置组（服务器/ }));
    const dialog = screen.getByRole('dialog');
    await user.selectOptions(within(dialog).getByLabelText('模板'), '自定义');
    await user.type(within(dialog).getByPlaceholderText('必填'), 'Custom');
    await user.click(within(dialog).getByRole('button', { name: '+ 添加变量' }));
    await user.click(within(dialog).getByRole('button', { name: '+ 添加变量' }));
    const names = within(dialog).getAllByPlaceholderText('NAME');
    await user.type(names[0], 'DB_PASSWORD');
    await user.type(names[1], 'APP_TOKEN');
    expect(within(dialog).getByLabelText('机密 DB_PASSWORD')).toBeChecked();
    await user.click(within(dialog).getByLabelText('机密 APP_TOKEN')); // user decides it is plain
    expect(within(dialog).getByLabelText('机密 APP_TOKEN')).not.toBeChecked();
    await user.type(within(dialog).getByLabelText('值 DB_PASSWORD'), FAKE_SECRET);
    await user.type(within(dialog).getByLabelText('值 APP_TOKEN'), 'public-looking');
    await user.click(within(dialog).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0].category).toBe('other');
    expect(create.mock.calls[0][0].variables).toEqual([
      { name: 'DB_PASSWORD', secret: true, value: FAKE_SECRET },
      { name: 'APP_TOKEN', secret: false, value: 'public-looking' },
    ]);
  });

  it('rejects invalid and duplicate variable names before calling the backend', async () => {
    const { create } = mockDesktop();
    const user = await openApp();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: /配置组（服务器/ }));
    const dialog = screen.getByRole('dialog');
    await user.type(within(dialog).getByPlaceholderText('必填'), 'Bad');
    await user.type(within(dialog).getByLabelText('值 SSH_HOST'), 'h');
    await user.clear(within(dialog).getByLabelText('变量名 SSH_HOST'));
    await user.type(within(dialog).getByLabelText('变量名 变量名'), '1BAD');
    await user.click(within(dialog).getByRole('button', { name: '保存凭据' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('变量名只能用字母、数字和下划线');
    expect(create).not.toHaveBeenCalled();
  });
});

describe('detail page', () => {
  it('shows plain values and dots, and never renders a secret value', async () => {
    const { pub } = mockDesktop();
    await openApp();
    await screen.findByText('203.0.113.7');
    expect(pub).toHaveBeenCalledWith('g1');
    const secretRow = document.querySelector('[data-variable="SSH_PASSWORD"]') as HTMLElement;
    expect(within(secretRow).getByText('••••••••')).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain(FAKE_SECRET);
    expect(document.body.innerHTML).not.toContain(FAKE_KEY);
    expect(screen.getByText('AI 可以看到：所有变量名和 1 个普通值')).toBeInTheDocument();
  });

  it('calls the matching backend command for reveal, copy, key export and notes', async () => {
    const mocks = mockDesktop();
    const user = await openApp();
    await screen.findByText('203.0.113.7');
    await user.click(screen.getByRole('button', { name: '显示 SSH_PASSWORD' }));
    expect(mocks.reveal).toHaveBeenCalledWith('g1', 'SSH_PASSWORD');
    await user.click(screen.getByRole('button', { name: '复制 SSH_PASSWORD' }));
    expect(mocks.copy).toHaveBeenCalledWith('g1', 'SSH_PASSWORD');
    await user.click(screen.getByRole('button', { name: '复制 SSH_HOST' }));
    expect(mocks.copy).toHaveBeenCalledWith('g1', 'SSH_HOST');
    // Only the private key row offers a file export, and the user picks the path.
    expect(screen.queryByRole('button', { name: '导出为文件 SSH_PASSWORD' })).toBeNull();
    await user.click(screen.getByRole('button', { name: '导出为文件 SSH_PRIVATE_KEY' }));
    await waitFor(() => expect(mocks.exportKey).toHaveBeenCalledWith('g1', 'SSH_PRIVATE_KEY', 'C:\\keys\\ssh_private_key'));
    // Notes load only on request.
    expect(mocks.notes).not.toHaveBeenCalled();
    expect(screen.queryByText('fake notes text')).toBeNull();
    await user.click(screen.getByRole('button', { name: '显示备注' }));
    expect(await screen.findByText('fake notes text')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '隐藏备注' }));
    expect(screen.queryByText('fake notes text')).toBeNull();
  });

  it('says AI sees no values when the group turns them off', async () => {
    const mocks = mockDesktop([row({ ...GROUP, ai_values: false })]);
    mocks.pub.mockResolvedValue({ variables: [{ name: 'SSH_HOST', secret: false, value: '203.0.113.7' }], hasNotes: false, aiValues: false });
    await openApp();
    expect(await screen.findByText(/AI 可以看到：所有变量名，看不到任何值/)).toBeInTheDocument();
  });
});

describe('edit flow', () => {
  it('keeps untouched secrets, prefills plain values and sends renames with from', async () => {
    const mocks = mockDesktop();
    const user = await openApp();
    await screen.findByText('203.0.113.7');
    await user.click(screen.getByRole('button', { name: '编辑' }));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByLabelText('值 SSH_HOST')).toHaveValue('203.0.113.7');
    // Secret inputs start empty: the stored value never comes back to the form.
    expect(within(dialog).getByLabelText('值 SSH_PASSWORD')).toHaveValue('');
    expect(document.body.innerHTML).not.toContain(FAKE_SECRET);
    // Notes stay encrypted until the user asks to edit them.
    expect(mocks.notes).not.toHaveBeenCalled();
    const hostName = within(dialog).getByLabelText('变量名 SSH_HOST');
    await user.clear(hostName);
    await user.type(hostName, 'SERVER_HOST');
    await user.click(within(dialog).getByRole('button', { name: '清空 值 SSH_PRIVATE_KEY' }));
    await user.click(within(dialog).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(mocks.update).toHaveBeenCalledTimes(1));
    const [id, request] = mocks.update.mock.calls[0];
    expect(id).toBe('g1');
    expect(request.notes).toBeUndefined();
    expect(request.variables).toEqual([
      { name: 'SERVER_HOST', secret: false, value: '203.0.113.7', from: 'SSH_HOST' },
      { name: 'SSH_PASSWORD', secret: true },
      { name: 'SSH_PRIVATE_KEY', secret: true, value: '' },
    ]);
  });

  it('loads and sends notes only after the user opens them for editing', async () => {
    const mocks = mockDesktop();
    const user = await openApp();
    await screen.findByText('203.0.113.7');
    await user.click(screen.getByRole('button', { name: '编辑' }));
    const dialog = screen.getByRole('dialog');
    await user.click(within(dialog).getByRole('button', { name: '编辑备注' }));
    const area = await within(dialog).findByDisplayValue('fake notes text');
    await user.type(area, ' more');
    await user.click(within(dialog).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(mocks.update).toHaveBeenCalled());
    expect(mocks.update.mock.calls[0][1].notes).toBe('fake notes text more');
  });
});
