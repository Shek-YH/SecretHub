import { act, render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import { App, McpPlanBanner, ModelSyncSection, ProfileDetailModal, ProjectEnvEditor } from './App';
import { AUTO_LOCK_EVENT, backend, isAutoLockError } from './lib/backend';
import { getTranslation } from './i18n/translations';

describe('SecretHub desktop shell', () => {
  it('drops to the unlock screen with a notice and closes open forms when the vault auto-locks', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '服务商' }));
    await user.click(screen.getAllByRole('button', { name: '配置' })[0]);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    act(() => { window.dispatchEvent(new Event(AUTO_LOCK_EVENT)); });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '解锁 SecretHub' })).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('自动锁定');
    expect(isAutoLockError('vault auto-locked after inactivity')).toBe(true);
    expect(isAutoLockError('invalid master password')).toBe(false);
  });

  it('starts in Chinese and exposes the language switch', () => {
    render(<App />);
    expect(screen.getByText('SecretHub')).toBeInTheDocument();
    expect(screen.getByText('凭据')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'English' })).toBeInTheDocument();
    expect(screen.getByText('复制 API Key')).toBeInTheDocument();
    expect(screen.getByText('复制全部内容')).toBeInTheDocument();
    expect(screen.getByText('API key')).toBeInTheDocument();
    expect(screen.getByText('TOKEN')).toBeInTheDocument();
  });

  it('opens the provider management view from the provider navigation', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '服务商' }));
    expect(screen.getByRole('heading', { name: '服务商' })).toBeInTheDocument();
    expect(screen.getByText('OpenAI')).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: '配置' })[9]);
    expect(screen.getAllByRole('combobox')[1]).toHaveValue('gpt-5.6-sol');
    expect(screen.getByDisplayValue('https://api.openai.com/v1')).toBeInTheDocument();
  });

  it('offers credential directory sorting by env export usage', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    const sort = screen.getByLabelText('排序');
    await user.selectOptions(sort, 'usage');
    expect(sort).toHaveValue('usage');
    expect(screen.getByText('调用输出次数')).toBeInTheDocument();
  });

  it('switches the provider directory between Chinese and English', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '服务商' }));
    await user.click(screen.getByRole('button', { name: 'English' }));
    expect(screen.getByRole('heading', { name: 'Providers' })).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: 'Configure' }).length).toBeGreaterThan(10);
    await user.click(screen.getByRole('button', { name: '中文' }));
    expect(screen.getByRole('heading', { name: '服务商' })).toBeInTheDocument();
  });

  it('shows batch API key validation only in the API key view', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: /API key/ }));
    expect(screen.getByRole('button', { name: '一键检测 API Key' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /TOKEN/ }));
    expect(screen.queryByRole('button', { name: '一键检测 API Key' })).not.toBeInTheDocument();
  });

  it('filters the provider picker when adding an API key', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: /^API KEY/ }));
    const search = screen.getByPlaceholderText('搜索服务商');
    await user.type(search, 'DeepSeek');
    expect(screen.getByRole('button', { name: /DeepSeek/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /OpenAI/ })).not.toBeInTheDocument();
  });

  it('opens provider templates before the secret form and makes env key optional', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    expect(screen.getByRole('button', { name: /^API KEY/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^TOKEN/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^URL/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Password/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Text/ })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^API KEY/ }));
    await user.click(screen.getByRole('button', { name: /DeepSeek 3 个模型模板/ }));
    expect(screen.getAllByDisplayValue('DeepSeek').length).toBeGreaterThan(0);
    const envKey = screen.getByPlaceholderText('OPENAI_API_KEY');
    expect(envKey).not.toBeRequired();
    expect(screen.getByText('前往官网生成 / 更新 ↗')).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'DeepSeek V4 Flash · deepseek-v4-flash' })).toBeInTheDocument();
  });

  it('opens a type-specific Token form without showing the model catalog', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: /^TOKEN/ }));
    expect(screen.getByText('TOKEN / TOKEN')).toBeInTheDocument();
    expect(screen.queryByText('TEMPLATE CATALOG / PROVIDERS')).not.toBeInTheDocument();
    expect(screen.queryByText('API Base URL')).not.toBeInTheDocument();
  });

  it('loads editable metadata and permits saving without exposing the old value', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByDisplayValue('OpenAI Main')).toBeInTheDocument();
    expect(screen.getByDisplayValue('OPENAI_API_KEY')).toBeInTheDocument();
    const value = screen.getByPlaceholderText('留空则保留原值');
    expect(value).not.toBeRequired();
    expect(screen.getByText('复制 API Key')).toBeInTheDocument();
  });

  it('opens a profile-name prompt from the multi-selection toolbar', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByLabelText('导出 OpenAI Main'));
    await user.click(screen.getByRole('button', { name: '加入组合' }));
    expect(screen.getByRole('dialog', { name: '新建组合' })).toBeInTheDocument();
    expect(screen.getByLabelText('组合名称')).toBeInTheDocument();
  });

  it('edits profile membership in a draft and saves only from the detail view', async () => {
    const user = (await import('@testing-library/user-event')).default.setup();
    const onSaved = vi.fn();
    render(<ProfileDetailModal t={getTranslation('zh-CN')} profile={{ id: 'profile-1', name: 'AI', description: '', secret_ids: ['openai-main'] }} secrets={[{ id: 'openai-main', name: 'OpenAI Main', envKey: 'OPENAI_API_KEY', provider: 'openai', tags: [], status: 'unknown', updated: 'now' }, { id: 'deepseek', name: 'DeepSeek', envKey: 'DEEPSEEK_API_KEY', provider: 'deepseek', tags: [], status: 'unknown', updated: 'now' }]} onClose={() => undefined} onSaved={onSaved} />);
    await user.click(screen.getByRole('button', { name: '增加凭据' }));
    await user.click(screen.getByLabelText(/DeepSeek/));
    await user.click(screen.getByRole('button', { name: '确定选择' }));
    expect(screen.getByText('DeepSeek')).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: '从组合移除' })[0]);
    expect(screen.queryByText('OpenAI Main')).not.toBeInTheDocument();
    expect(onSaved).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: '保存组合' }));
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  it('keeps managed env rows masked while allowing manual rows to be edited', async () => {
    const user = (await import('@testing-library/user-event')).default.setup();
    render(<ProjectEnvEditor t={getTranslation('zh-CN')} project={{ id: 'project-1', path: 'F:/demo', display_name: 'demo' }} entries={[{ original_key: 'OPENAI_API_KEY', key: 'OPENAI_API_KEY', value: '••••••••', source: 'managed', secret_id: 'secret-1' }, { original_key: 'PORT', key: 'PORT', value: '3000', source: 'manual' }]} onClose={() => undefined} onSaved={() => undefined} />);
    expect(screen.getByText(/手动条目会保留/)).toBeInTheDocument();
    expect(screen.getByDisplayValue('••••••••')).toHaveAttribute('readonly');
    await user.click(screen.getByRole('button', { name: '增加环境变量' }));
    expect(screen.getAllByRole('textbox')).toHaveLength(6);
  });

  it('renders model sync status, count, capability tags and the sync action', async () => {
    const user = (await import('@testing-library/user-event')).default.setup();
    const onSync = vi.fn();
    const models = [{ model_id: 'qwen3-coder', display_name: '', input: [], output: [], capabilities: ['code'], capability_source: 'heuristic', synced_at: 1 }];
    render(<ModelSyncSection t={getTranslation('zh-CN')} locale="zh-CN" status="ok" syncedAt={1760600000} models={models} syncing={false} message="" onSync={onSync} />);
    expect(screen.getByText('已同步')).toBeInTheDocument();
    expect(screen.getByText('qwen3-coder')).toBeInTheDocument();
    expect(screen.getByText('code')).toBeInTheDocument();
    expect(screen.getByText('模型数: 1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '同步模型' }));
    expect(onSync).toHaveBeenCalledTimes(1);
  });

  it('shows the never-synced state and caps long model lists', () => {
    const many = Array.from({ length: 205 }, (_, index) => ({ model_id: `m-${index}`, display_name: '', input: [], output: [], capabilities: [], capability_source: 'unknown', synced_at: 1 }));
    const { rerender } = render(<ModelSyncSection t={getTranslation('en-US')} locale="en-US" status="" syncedAt={0} models={[]} syncing={false} message="" onSync={() => undefined} />);
    expect(screen.getByText('Not synced yet')).toBeInTheDocument();
    rerender(<ModelSyncSection t={getTranslation('en-US')} locale="en-US" status="failed" syncedAt={5} models={many} syncing={false} message="" onSync={() => undefined} />);
    expect(screen.getByText('+5 more')).toBeInTheDocument();
    expect(screen.queryByText('m-204')).not.toBeInTheDocument();
    expect(screen.getByText('Sync failed (previous list kept)')).toBeInTheDocument();
  });
});

describe('McpPlanBanner', () => {
  const plan = { request_id: 'plan-1', kind: 'prepare_project_env', project_path: 'C:/demo', secret_ids: [], version: 2, created_at: 't' };
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });

  it('shows a compact summary without any secret value and opens the review', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'planPreview').mockResolvedValue({ requestId: 'plan-1', projectPath: 'C:/demo', version: 2, errors: [], entries: [
      { key: 'API_KEY', kind: 'api_key', display: '••••••••', conflict: true, warnings: [] },
      { key: 'LLM_MODEL', kind: 'model', display: 'm1', capability: 'chat', conflict: false, warnings: ['model_not_in_synced_list'] },
    ] });
    const onReview = vi.fn();
    render(<McpPlanBanner plan={plan} t={getTranslation('en-US')} onReview={onReview} />);
    expect(await screen.findByText('API_KEY, LLM_MODEL')).toBeInTheDocument();
    expect(screen.getByText(/Existing project .env keys that will be overwritten: API_KEY/)).toBeInTheDocument();
    expect(screen.getByText(/^1 warning\(s\)/)).toBeInTheDocument();
    (await import('@testing-library/user-event')).default.setup();
    screen.getByRole('button', { name: 'Review' }).click();
    expect(onReview).toHaveBeenCalled();
  });

  it('shows preview errors in the banner', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'planPreview').mockResolvedValue({ requestId: 'plan-1', projectPath: 'C:/demo', version: 2, entries: [], errors: [{ code: 'env_conflict', message: 'x' }] });
    render(<McpPlanBanner plan={plan} t={getTranslation('en-US')} onReview={() => undefined} />);
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('env_conflict'));
  });
});
