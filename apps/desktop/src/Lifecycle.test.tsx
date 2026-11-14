import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App } from './App';
import { RestoreDialog, restoreErrorText } from './BackupRestore';
import { backend, type BackendSecret, type BackendSettings } from './lib/backend';
import { getTranslation } from './i18n/translations';

const t = getTranslation('zh-CN');
const nowSeconds = () => Math.floor(Date.now() / 1000);

const secretRow = (patch: Partial<BackendSecret> = {}): BackendSecret => ({
  id: 's1', name: 'DS', provider_id: 'deepseek', env_key: 'DEEPSEEK_API_KEY', description: '', tags: [], status: 'valid', value_type: 'api_key', category: 'ai', scope: 'global', favorite: false, archived: false,
  model_id: 'deepseek-v4-pro', model_env_key: 'DEEPSEEK_MODEL', endpoint_url: 'https://api.deepseek.com', endpoint_env_key: 'DEEPSEEK_BASE_URL', models_synced_at: 5, models_sync_status: 'ok', created_at: 1, updated_at: 1, validated_at: nowSeconds() - 3600, ...patch,
});

const settings: BackendSettings = { locale: 'zh-CN', theme: 'dark', auto_lock_minutes: 15, clipboard_timeout_seconds: 30, require_auth_before_export: true, require_auth_before_reveal: true };

function mockDesktop(options: { unlocked?: boolean; rows?: BackendSecret[] } = {}) {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  const unlocked = options.unlocked ?? true;
  vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked });
  vi.spyOn(backend, 'list').mockResolvedValue(options.rows ?? [secretRow()]);
  vi.spyOn(backend, 'audit').mockResolvedValue([]);
  vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
  vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
  vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
  vi.spyOn(backend, 'models').mockResolvedValue([]);
  return { validateMany: vi.spyOn(backend, 'validateMany').mockResolvedValue([]) };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe('on-demand validation', () => {
  it('re-checks only stale keys at startup, all keys from the toolbar, and shows the last check', async () => {
    const { validateMany } = mockDesktop();
    render(<App />);
    const user = userEvent.setup();
    await waitFor(() => expect(validateMany).toHaveBeenCalledWith(true));
    expect(validateMany).not.toHaveBeenCalledWith(false);
    await user.click(await screen.findByText('DS'));
    expect(await screen.findByText('上次检测')).toBeInTheDocument();
    expect(screen.getByText('1 小时前')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /API key/ }));
    await user.click(screen.getByRole('button', { name: '一键检测 API Key' }));
    await waitFor(() => expect(validateMany).toHaveBeenLastCalledWith(false));
  });

  it('says a key was never checked when validated_at is 0', async () => {
    mockDesktop({ rows: [secretRow({ validated_at: 0 })] });
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByText('DS'));
    expect(await screen.findByText('从未检测')).toBeInTheDocument();
  });
});

describe('settings page', () => {
  it('persists the auto-lock choice and drops the fake rows', async () => {
    mockDesktop();
    vi.spyOn(backend, 'settingsGet').mockResolvedValue(settings);
    const update = vi.spyOn(backend, 'settingsUpdate').mockResolvedValue(undefined);
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /设置/ }));
    const select = (await screen.findByLabelText('自动锁定')) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe('15'));
    expect(Array.from(select.options).map((option) => option.value)).toEqual(['1', '5', '15', '30', '60']);
    await user.selectOptions(select, '5');
    await waitFor(() => expect(update).toHaveBeenCalledWith({ ...settings, auto_lock_minutes: 5 }));
    expect(screen.queryByText(t.settings.theme)).not.toBeInTheDocument();
    expect(screen.queryByText(t.settings.defaultExport)).not.toBeInTheDocument();
    expect(screen.queryByText(t.settings.requireAuth)).not.toBeInTheDocument();
    expect(screen.queryByText('Metadata access')).not.toBeInTheDocument();
  });

  it('keeps an unusual stored value selectable and reverts when saving fails', async () => {
    mockDesktop();
    vi.spyOn(backend, 'settingsGet').mockResolvedValue({ ...settings, auto_lock_minutes: 10 });
    vi.spyOn(backend, 'settingsUpdate').mockRejectedValue('vault unavailable');
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /设置/ }));
    const select = (await screen.findByLabelText('自动锁定')) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe('10'));
    expect(Array.from(select.options).map((option) => option.value)).toEqual(['1', '5', '10', '15', '30', '60']);
    await user.selectOptions(select, '30');
    expect(await screen.findByText('vault unavailable')).toBeInTheDocument();
    expect(select.value).toBe('10');
  });

  it('exports a backup into the chosen folder and reports the written file', async () => {
    mockDesktop();
    vi.spyOn(backend, 'settingsGet').mockResolvedValue(settings);
    vi.spyOn(backend, 'chooseFolder').mockResolvedValue('C:/backups');
    const backup = vi.spyOn(backend, 'backup').mockResolvedValue('C:/backups/SecretHub-20261109-0805.secrethub-backup');
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /设置/ }));
    await user.click(await screen.findByRole('button', { name: t.settings.backup }));
    await waitFor(() => expect(backup).toHaveBeenCalledWith('C:/backups'));
    expect(await screen.findByText(/SecretHub-20261109-0805\.secrethub-backup/)).toBeInTheDocument();
  });
});

describe('restore from backup', () => {
  it('walks through the dialog from the lock screen and unlocks the restored vault', async () => {
    mockDesktop({ unlocked: false });
    vi.spyOn(backend, 'chooseBackupFile').mockResolvedValue('C:/backups/old.secrethub-backup');
    const restore = vi.spyOn(backend, 'restoreBackup').mockResolvedValue(undefined);
    render(<App />);
    const user = userEvent.setup();
    expect(await screen.findByRole('heading', { name: '解锁 SecretHub' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '从备份恢复' }));
    const dialog = screen.getByRole('dialog', { name: '从备份恢复' });
    expect(within(dialog).getByText(/不会删除/)).toBeInTheDocument();
    const submit = within(dialog).getByRole('button', { name: '恢复' });
    expect(submit).toBeDisabled();
    await user.click(within(dialog).getByRole('button', { name: '选择备份文件' }));
    expect(await within(dialog).findByText('C:/backups/old.secrethub-backup')).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText('备份的主密码'), 'fake backup password');
    expect(submit).toBeDisabled();
    await user.click(within(dialog).getByRole('checkbox'));
    expect(submit).toBeEnabled();
    await user.click(submit);
    await waitFor(() => expect(restore).toHaveBeenCalledWith('C:/backups/old.secrethub-backup', 'fake backup password'));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: '从备份恢复' })).not.toBeInTheDocument());
    expect(screen.queryByRole('heading', { name: '解锁 SecretHub' })).not.toBeInTheDocument();
  });

  it('shows localized messages for known errors and keeps the dialog open', async () => {
    vi.spyOn(backend, 'chooseBackupFile').mockResolvedValue('C:/x.secrethub-backup');
    const restore = vi.spyOn(backend, 'restoreBackup').mockRejectedValueOnce('restore_bad_password').mockRejectedValueOnce('restore_files_busy');
    const onRestored = vi.fn();
    render(<RestoreDialog t={t} onClose={() => undefined} onRestored={onRestored} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: '选择备份文件' }));
    await user.type(screen.getByLabelText('备份的主密码'), 'x');
    await user.click(screen.getByRole('checkbox'));
    await user.click(screen.getByRole('button', { name: '恢复' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('密码错误或文件已损坏');
    await user.click(screen.getByRole('button', { name: '恢复' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('请关闭正在使用 SecretHub 的 AI 工具后重试'));
    expect(restore).toHaveBeenCalledTimes(2);
    expect(onRestored).not.toHaveBeenCalled();
    expect(restoreErrorText(getTranslation('en-US'), 'restore_legacy_other_vault')).toBe('Old-format backups can only be restored in their original vault');
    expect(restoreErrorText(t, 'something else')).toBe('something else');
  });

  it('is available from the settings page and refreshes the list afterwards', async () => {
    mockDesktop();
    vi.spyOn(backend, 'settingsGet').mockResolvedValue(settings);
    vi.spyOn(backend, 'chooseBackupFile').mockResolvedValue('C:/b.secrethub-backup');
    const restore = vi.spyOn(backend, 'restoreBackup').mockResolvedValue(undefined);
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /设置/ }));
    await user.click(await screen.findByRole('button', { name: '从备份恢复' }));
    const dialog = screen.getByRole('dialog', { name: '从备份恢复' });
    await user.click(within(dialog).getByRole('button', { name: '选择备份文件' }));
    await user.type(within(dialog).getByLabelText('备份的主密码'), 'fake backup password');
    await user.click(within(dialog).getByRole('checkbox'));
    await user.click(within(dialog).getByRole('button', { name: '恢复' }));
    await waitFor(() => expect(restore).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByRole('dialog', { name: '从备份恢复' })).not.toBeInTheDocument());
    expect(await screen.findByText(t.tools.restoreDone)).toBeInTheDocument();
  });
});

describe('smart import', () => {
  it('previews grouped rows from a chosen file and checks the new API keys after importing', async () => {
    mockDesktop();
    vi.spyOn(backend, 'chooseImportFile').mockResolvedValue('C:/project/.env');
    vi.spyOn(backend, 'importFilePreview').mockResolvedValue([
      { label: 'OpenAI', generic: false, keys: ['OPENAI_API_KEY', 'OPENAI_BASE_URL', 'OPENAI_MODEL'] },
      { label: '', generic: true, keys: ['FOO_TOKEN'] },
    ]);
    const importFile = vi.spyOn(backend, 'importFile').mockResolvedValue({ ids: ['n1', 'n2'], apiKeyIds: ['n1'] });
    const validate = vi.spyOn(backend, 'validate').mockResolvedValue('valid');
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: t.actions.import }));
    const dialog = screen.getByRole('dialog', { name: t.actions.import });
    const confirm = within(dialog).getByRole('button', { name: t.actions.confirmImport });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByRole('button', { name: t.tools.importChooseFile }));
    expect(await within(dialog).findByText('OpenAI ← OPENAI_API_KEY, OPENAI_BASE_URL, OPENAI_MODEL')).toBeInTheDocument();
    expect(within(dialog).getByText('通用 ← FOO_TOKEN')).toBeInTheDocument();
    await user.click(confirm);
    await waitFor(() => expect(importFile).toHaveBeenCalledWith('C:/project/.env'));
    await waitFor(() => expect(validate).toHaveBeenCalledTimes(1));
    expect(validate).toHaveBeenCalledWith('n1');
    expect(await within(dialog).findByText(t.tools.importValidated)).toBeInTheDocument();
  });

  it('keeps pasted text as a secondary path through the text endpoints', async () => {
    mockDesktop();
    const preview = vi.spyOn(backend, 'importPreview').mockResolvedValue([{ label: 'Anthropic', generic: false, keys: ['ANTHROPIC_API_KEY'] }]);
    const run = vi.spyOn(backend, 'importSecrets').mockResolvedValue({ ids: ['n1'], apiKeyIds: [] });
    render(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: t.actions.import }));
    const dialog = screen.getByRole('dialog', { name: t.actions.import });
    await user.type(within(dialog).getByLabelText(t.actions.importContent), 'ANTHROPIC_API_KEY=fake-value');
    await user.click(within(dialog).getByRole('button', { name: t.actions.importPreview }));
    expect(await within(dialog).findByText('Anthropic ← ANTHROPIC_API_KEY')).toBeInTheDocument();
    expect(preview).toHaveBeenCalledWith({ format: 'env', contents: 'ANTHROPIC_API_KEY=fake-value', confirmed: false });
    await user.click(within(dialog).getByRole('button', { name: t.actions.confirmImport }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ format: 'env', contents: 'ANTHROPIC_API_KEY=fake-value', confirmed: true }));
  });
});
