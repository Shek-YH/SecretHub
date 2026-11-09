import { act, render, screen, waitFor, within } from '@testing-library/react';
import { vi } from 'vitest';
import { App, McpPlanBanner, ModelSyncSection, ProfileDetailModal, ProjectEnvEditor } from './App';
import { AUTO_LOCK_EVENT, backend, isAutoLockError } from './lib/backend';
import { getTranslation } from './i18n/translations';
import { SecretForm } from './SecretForm';

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
    const openaiCard = screen.getByText('OpenAI').closest('article') as HTMLElement;
    await user.click(within(openaiCard).getByRole('button', { name: '配置' }));
    const form = screen.getByRole('dialog', { name: '添加凭据' });
    expect(within(form).getByRole('button', { name: 'OpenAI' })).toHaveAttribute('aria-pressed', 'true');
    expect(within(form).getByLabelText('名称')).toHaveValue('OpenAI');
    await user.click(within(form).getByRole('button', { name: /高级设置/ }));
    expect(within(form).getByLabelText('模型')).toHaveValue('gpt-5.6-sol');
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

  it('filters the full provider list when adding an API key', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: '更多…' }));
    const panel = screen.getByRole('group', { name: '全部服务商' });
    expect(within(panel).getByRole('button', { name: 'Groq' })).toBeInTheDocument();
    await user.type(within(panel).getByPlaceholderText('搜索服务商'), 'DeepSeek');
    expect(within(panel).getByRole('button', { name: 'DeepSeek' })).toBeInTheDocument();
    expect(within(panel).queryByRole('button', { name: 'Groq' })).not.toBeInTheDocument();
  });

  it('opens the one-screen API key form, picks a provider chip and makes env key optional', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    const form = screen.getByRole('dialog', { name: '添加凭据' });
    expect(within(form).getByLabelText('粘贴 API Key')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: 'DeepSeek' }));
    expect(within(form).getByLabelText('名称')).toHaveValue('DeepSeek');
    expect(within(form).getByRole('button', { name: '从官网获取 Key' })).toBeInTheDocument();
    expect(within(form).getByText('前往官网生成 / 更新 ↗')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: /高级设置/ }));
    expect(within(form).getByPlaceholderText('OPENAI_API_KEY')).not.toBeRequired();
    expect(within(form).getByRole('option', { name: 'DeepSeek V4 Flash · deepseek-v4-flash' })).toBeInTheDocument();
  });

  it('opens a type-specific Token form via the other-types link without showing the model catalog', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '添加凭据' })[0]);
    await user.click(screen.getByRole('button', { name: /添加其他类型/ }));
    expect(screen.getByRole('button', { name: /^API KEY/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^URL/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Password/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Text/ })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^TOKEN/ }));
    expect(screen.getByText('TOKEN / TOKEN')).toBeInTheDocument();
    expect(screen.queryByText('TEMPLATE CATALOG / PROVIDERS')).not.toBeInTheDocument();
    expect(screen.queryByText('API Base URL')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('粘贴 API Key')).not.toBeInTheDocument();
  });

  it('loads editable metadata and permits saving without exposing the old value', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '编辑' }));
    expect(screen.getByDisplayValue('OpenAI Main')).toBeInTheDocument();
    expect(screen.getByDisplayValue('OPENAI_API_KEY')).toBeInTheDocument();
    const value = screen.getByPlaceholderText('留空则保持不变');
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

describe('model choice from synced models', () => {
  afterEach(() => { vi.restoreAllMocks(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; });
  const synced = (id: string, capabilities: string[] = ['chat']) => ({ model_id: id, display_name: '', input: [], output: [], capabilities, capability_source: 'provider', synced_at: 1 });
  const keyInitial = { id: 's1', name: 'DS', envKey: 'DEEPSEEK_API_KEY', provider: 'deepseek', tags: [], status: 'valid', updated: 'now', valueType: 'api_key', modelId: 'deepseek-v4-pro', modelEnvKey: 'DEEPSEEK_MODEL', endpointUrl: 'https://api.deepseek.com', endpointEnvKey: 'DEEPSEEK_BASE_URL' };

  it('groups synced models first, then catalog leftovers, then custom', () => {
    render(<SecretForm t={getTranslation('zh-CN')} knownTags={[]} initial={keyInitial} syncedModels={[synced('zz-new', ['chat', 'vision']), synced('deepseek-v4-flash')]} onClose={() => undefined} onSave={vi.fn()} />);
    const select = screen.getByLabelText('模型') as HTMLSelectElement;
    const groups = Array.from(select.querySelectorAll('optgroup')).map((group) => [group.label, Array.from(group.querySelectorAll('option')).map((option) => option.value)]);
    expect(groups).toEqual([
      ['此 Key 可用的模型', ['deepseek-v4-flash', 'zz-new']],
      ['目录推荐（未在此 Key 清单中）', ['deepseek-v4-pro', 'deepseek-v4-flash-vision-exp']],
    ]);
    expect(within(select).getByText(/zz-new \[chat, vision\]/)).toBeInTheDocument();
    expect(select.value).toBe('deepseek-v4-pro');
    expect(select.lastElementChild).toHaveValue('__custom__');
  });

  it('keeps the flat catalog list when the key has no synced models', () => {
    render(<SecretForm t={getTranslation('zh-CN')} knownTags={[]} initial={keyInitial} onClose={() => undefined} onSave={vi.fn()} />);
    expect((screen.getByLabelText('模型') as HTMLSelectElement).querySelectorAll('optgroup')).toHaveLength(0);
  });

  it('offers a set-default button per row that does not re-enter the secret value', async () => {
    const user = (await import('@testing-library/user-event')).default.setup();
    const onSetDefault = vi.fn();
    render(<ModelSyncSection t={getTranslation('zh-CN')} locale="zh-CN" status="ok" syncedAt={1} models={[synced('a'), synced('b')]} syncing={false} message="" onSync={() => undefined} currentModelId="a" onSetDefault={onSetDefault} />);
    expect(screen.getByText('当前默认')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '设为默认模型 b' }));
    expect(onSetDefault).toHaveBeenCalledWith('b');
  });

  it('updates the secret model_id through a metadata-only update in the app', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const row = { id: 's1', name: 'DS', provider_id: 'deepseek', env_key: 'DEEPSEEK_API_KEY', description: '', tags: [], status: 'valid', value_type: 'api_key', category: 'ai', scope: 'global', favorite: false, archived: false, model_id: 'deepseek-v4-pro', model_env_key: 'DEEPSEEK_MODEL', endpoint_url: 'https://api.deepseek.com', endpoint_env_key: 'DEEPSEEK_BASE_URL', models_synced_at: 5, models_sync_status: 'ok', created_at: 1, updated_at: 1 };
    vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked: true });
    vi.spyOn(backend, 'list').mockResolvedValue([row]);
    vi.spyOn(backend, 'audit').mockResolvedValue([]);
    vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
    vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
    vi.spyOn(backend, 'models').mockResolvedValue([synced('deepseek-v4-flash'), synced('deepseek-v4-pro')]);
    const update = vi.spyOn(backend, 'update').mockResolvedValue(undefined);
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(await screen.findByText('DS'));
    await user.click(await screen.findByRole('button', { name: '设为默认模型 deepseek-v4-flash' }));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update).toHaveBeenCalledWith('s1', expect.objectContaining({ value: '', model_id: 'deepseek-v4-flash', model_env_key: 'DEEPSEEK_MODEL', provider_id: 'deepseek' }));
    expect(await screen.findByRole('button', { name: '设为默认模型 deepseek-v4-pro' })).toBeInTheDocument();
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

describe('Tag picker in the secret form', () => {
  async function openEditForm() {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getAllByRole('button', { name: '编辑' })[0]);
    return user;
  }

  it('pre-selects existing tags and lists library tags from demo secrets', async () => {
    const user = await openEditForm();
    const form = screen.getByRole('dialog', { name: '编辑凭据' });
    expect(within(form).getByText('production')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: '选择标签' }));
    const picker = screen.getByRole('dialog', { name: '选择标签' });
    expect(within(picker).getByRole('button', { name: 'ai' })).toHaveAttribute('aria-pressed', 'true');
    expect(within(picker).getByRole('button', { name: 'research' })).toHaveAttribute('aria-pressed', 'false');
    expect(within(picker).getByRole('button', { name: 'database' })).toBeInTheDocument();
  });

  it('multi-selects, auto-selects new tags, discards on cancel and applies on confirm', async () => {
    const user = await openEditForm();
    const form = screen.getByRole('dialog', { name: '编辑凭据' });
    await user.click(within(form).getByRole('button', { name: '选择标签' }));
    let picker = screen.getByRole('dialog', { name: '选择标签' });
    await user.click(within(picker).getByRole('button', { name: 'research' }));
    await user.click(within(picker).getByRole('button', { name: 'ai' }));
    await user.type(within(picker).getByLabelText('新建标签'), 'Fresh{Enter}');
    expect(within(picker).getByRole('button', { name: 'Fresh' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(within(picker).getByRole('button', { name: '取消' }));
    expect(screen.queryByRole('dialog', { name: '选择标签' })).not.toBeInTheDocument();
    expect(within(form).queryByText('research')).not.toBeInTheDocument();
    expect(within(form).getByText('production')).toBeInTheDocument();

    await user.click(within(form).getByRole('button', { name: '选择标签' }));
    picker = screen.getByRole('dialog', { name: '选择标签' });
    await user.click(within(picker).getByRole('button', { name: 'research' }));
    await user.type(within(picker).getByLabelText('新建标签'), 'RESEARCH');
    await user.click(within(picker).getByRole('button', { name: '添加' }));
    expect(within(picker).getAllByRole('button', { name: /^research$/i })).toHaveLength(1);
    await user.type(within(picker).getByLabelText('新建标签'), 'a,b');
    await user.click(within(picker).getByRole('button', { name: '添加' }));
    expect(within(picker).getByRole('alert')).toHaveTextContent('1–32');
    await user.click(within(picker).getByRole('button', { name: '确定' }));
    expect(within(form).getByText('research')).toBeInTheDocument();
    expect(within(form).getByText('production')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: '移除标签 production' }));
    expect(within(form).queryByText('production')).not.toBeInTheDocument();
  });

  it('passes the selected tags array to onSave', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const initial = { id: 's1', name: 'Demo', envKey: 'DEMO_KEY', provider: 'custom', tags: ['ai'], status: 'unknown', updated: 'now', valueType: 'text' };
    render(<SecretForm t={getTranslation('zh-CN')} knownTags={['ai', 'ops']} initial={initial} onClose={() => undefined} onSave={onSave} />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '选择标签' }));
    await user.click(screen.getByRole('button', { name: 'ops' }));
    await user.click(screen.getByRole('button', { name: '确定' }));
    await user.type(screen.getByLabelText(/Secret 值/), 'v');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ tags: ['ai', 'ops'] }));
  });
});

describe('shell conveniences', () => {
  it('focuses the search box with Ctrl+K and shows the matching hint', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    expect(screen.getByText('Ctrl K')).toBeInTheDocument();
    const search = screen.getByRole('textbox', { name: '搜索凭据、服务商或标签' });
    expect(search).not.toHaveFocus();
    await user.keyboard('{Control>}k{/Control}');
    expect(search).toHaveFocus();
  });

  it('jumps back to the list from another view with Ctrl+K', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '服务商' }));
    expect(screen.queryByRole('textbox', { name: '搜索凭据、服务商或标签' })).not.toBeInTheDocument();
    await user.keyboard('{Control>}k{/Control}');
    expect(await screen.findByRole('textbox', { name: '搜索凭据、服务商或标签' })).toHaveFocus();
  });

  it('masks the detail value with dots only, without a fake tail', () => {
    render(<App />);
    const masked = document.querySelector('.masked-value') as HTMLElement;
    expect(masked.textContent).toMatch(/^•+$/);
    expect(masked.textContent).not.toMatch(/Km2/);
  });

  it('shows relative update times in the list', () => {
    render(<App />);
    expect(screen.getByText('OpenAI · 2 小时前')).toBeInTheDocument();
    expect(screen.getByText('Anthropic · 1 天前')).toBeInTheDocument();
  });

  it('groups providers into common and collapsed more, and searching reveals matches across both', async () => {
    render(<App />);
    const user = (await import('@testing-library/user-event')).default.setup();
    await user.click(screen.getByRole('button', { name: '服务商' }));
    expect(screen.getByText('DeepSeek')).toBeInTheDocument();
    expect(screen.queryByText('Groq')).not.toBeInTheDocument();
    const toggle = screen.getByRole('button', { name: /^更多服务商（\d+）/ });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await user.type(screen.getByPlaceholderText('搜索服务商'), 'groq');
    expect(screen.getByText('Groq')).toBeInTheDocument();
    expect(screen.queryByText('DeepSeek')).not.toBeInTheDocument();
    await user.clear(screen.getByPlaceholderText('搜索服务商'));
    await user.click(toggle);
    expect(screen.getByText('Groq')).toBeInTheDocument();
  });
});
