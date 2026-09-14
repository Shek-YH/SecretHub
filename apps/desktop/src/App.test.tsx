import { render, screen } from '@testing-library/react';
import { vi } from 'vitest';
import { App, ProfileDetailModal, ProjectEnvEditor } from './App';
import { getTranslation } from './i18n/translations';

describe('SecretHub desktop shell', () => {
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
    await user.click(screen.getByRole('button', { name: 'OpenAI' }));
    expect(screen.getByRole('heading', { name: 'OpenAI' })).toBeInTheDocument();
    expect(screen.getByText('可用模型模板')).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: '配置此模型' })[0]);
    expect(screen.getAllByRole('combobox')[1]).toHaveValue('gpt-5.5');
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
    await user.click(screen.getByRole('button', { name: /DeepSeek 2 models/ }));
    expect(screen.getAllByDisplayValue('DeepSeek').length).toBeGreaterThan(0);
    const envKey = screen.getByPlaceholderText('OPENAI_API_KEY');
    expect(envKey).not.toBeRequired();
    expect(screen.getByText('前往官网生成 / 更新 ↗')).toBeInTheDocument();
    expect(screen.getByText(/DeepSeek Chat/)).toBeInTheDocument();
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
});
