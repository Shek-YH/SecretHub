import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import type { ComponentProps } from 'react';
import { getTranslation } from './i18n/translations';
import { defaultApiKeyEnvKey, defaultEndpointEnvKey, providerTemplate } from './providers';
import { SecretForm } from './SecretForm';
import type { BackendModel } from './lib/backend';

const t = getTranslation('zh-CN');
const FAKE_KEY = 'FAKE-TEST-VALUE';
const synced = (id: string): BackendModel => ({ model_id: id, display_name: '', input: [], output: [], capabilities: ['chat'], capability_source: 'provider', synced_at: 1 });

function setup(props: Partial<ComponentProps<typeof SecretForm>> = {}) {
  const onSave = vi.fn().mockResolvedValue(undefined);
  const onClose = vi.fn();
  render(<SecretForm t={t} knownTags={[]} initial={null} onClose={onClose} onSave={onSave} {...props} />);
  return { onSave, onClose, user: userEvent.setup() };
}

describe('one-screen API key form', () => {
  it('recognizes the provider from a distinctive prefix and saves the provider defaults', async () => {
    const { onSave, user } = setup();
    await user.type(screen.getByLabelText(t.quick.pasteKey), `sk-ant-${FAKE_KEY}`);
    expect(screen.getByRole('status')).toHaveTextContent('已识别为 Anthropic');
    expect(screen.getByRole('button', { name: 'Anthropic' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByLabelText('名称')).toHaveValue('Anthropic');
    const anthropic = providerTemplate('anthropic');
    const model = anthropic.models.find((item) => item.id === anthropic.defaultModel) ?? anthropic.models[0];
    expect(screen.getByText(new RegExp(`默认模型：${model.id}`))).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: t.form.save }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
      provider_id: 'anthropic',
      name: 'Anthropic',
      value: `sk-ant-${FAKE_KEY}`,
      value_type: 'api_key',
      category: 'ai',
      scope: 'global',
      env_key: defaultApiKeyEnvKey('anthropic'),
      endpoint_url: anthropic.baseUrl,
      endpoint_env_key: defaultEndpointEnvKey('anthropic'),
      model_id: model.id,
      model_env_key: model.envKey,
    }));
  });

  it('does not guess a provider from the generic sk- prefix', async () => {
    const { onSave, user } = setup();
    await user.type(screen.getByLabelText(t.quick.pasteKey), `sk-${FAKE_KEY}`);
    expect(screen.getByRole('status')).toHaveTextContent(t.quick.ambiguous);
    expect(document.querySelector('.provider-chip.active')).toBeNull();
    await user.click(screen.getByRole('button', { name: t.form.save }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.quick.pickProvider);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('lets the user override the detected provider', async () => {
    const { onSave, user } = setup();
    const input = screen.getByLabelText(t.quick.pasteKey);
    await user.type(input, `sk-ant-${FAKE_KEY}`);
    await user.click(screen.getByRole('button', { name: 'DeepSeek' }));
    await user.type(input, '-more');
    expect(screen.getByRole('button', { name: 'DeepSeek' })).toHaveAttribute('aria-pressed', 'true');
    await user.click(screen.getByRole('button', { name: t.form.save }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ provider_id: 'deepseek', env_key: 'DEEPSEEK_API_KEY' })));
  });

  it('keeps advanced settings collapsed and has no value type or scope fields', async () => {
    const { user } = setup();
    expect(screen.getByRole('button', { name: /高级设置/ })).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByLabelText(t.form.endpointUrl)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /高级设置/ }));
    expect(screen.getByLabelText(t.form.endpointUrl)).toBeInTheDocument();
    expect(screen.queryByLabelText(t.form.valueType)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(t.form.scope)).not.toBeInTheDocument();
    expect(screen.queryByText(t.form.valueType)).not.toBeInTheDocument();
    expect(screen.queryByText(t.form.scope)).not.toBeInTheDocument();
  });

  it('suggests a unique name and keeps used providers in the common chips', async () => {
    const used = providerTemplate('groq');
    const { user } = setup({ existingSecrets: [{ name: 'OpenAI', provider: 'openai' }, { name: used.name, provider: 'groq' }] });
    expect(screen.getByRole('button', { name: used.name })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'OpenAI' }));
    expect(screen.getByLabelText('名称')).toHaveValue('OpenAI 2');
    await user.clear(screen.getByLabelText('名称'));
    await user.type(screen.getByLabelText('名称'), 'My key');
    await user.click(screen.getByRole('button', { name: 'DeepSeek' }));
    expect(screen.getByLabelText('名称')).toHaveValue('My key');
  });

  it('searches the complete list behind the more button', async () => {
    const { user } = setup();
    expect(screen.queryByRole('button', { name: 'Groq' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: t.quick.moreButton }));
    const panel = screen.getByRole('group', { name: t.quick.allProviders });
    await user.type(within(panel).getByPlaceholderText(t.form.providerSearch), 'groq');
    await user.click(within(panel).getByRole('button', { name: 'Groq' }));
    expect(screen.getByRole('button', { name: 'Groq', pressed: true })).toBeInTheDocument();
  });

  it('requires a base URL for the custom provider and opens the advanced section', async () => {
    const { onSave, user } = setup();
    await user.click(screen.getByRole('button', { name: providerTemplate('custom').name }));
    await user.type(screen.getByLabelText(t.quick.pasteKey), FAKE_KEY);
    await user.click(screen.getByRole('button', { name: t.form.save }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.quick.customNeedsUrl);
    expect(screen.getByLabelText(t.form.endpointUrl)).toBeInTheDocument();
    expect(onSave).not.toHaveBeenCalled();
  });

  it('keeps the stored scope when editing and shows the unchanged-value placeholder', async () => {
    const initial = { id: 's1', name: 'DS', envKey: 'DEEPSEEK_API_KEY', provider: 'deepseek', tags: [], status: 'valid', updated: '', valueType: 'api_key', scope: 'project', modelId: 'deepseek-v4-pro', modelEnvKey: 'DEEPSEEK_MODEL', endpointUrl: 'https://api.deepseek.com', endpointEnvKey: 'DEEPSEEK_BASE_URL' };
    const { onSave, user } = setup({ initial });
    expect(screen.getByPlaceholderText(t.form.keepExistingValue)).not.toBeRequired();
    await user.click(screen.getByRole('button', { name: t.form.save }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ scope: 'project', value: '', value_type: 'api_key' })));
  });
});

describe('model search for long candidate lists', () => {
  const initial = { id: 's1', name: 'DS', envKey: 'DEEPSEEK_API_KEY', provider: 'deepseek', tags: [], status: 'valid', updated: '', valueType: 'api_key', modelId: 'deepseek-v4-pro', modelEnvKey: 'DEEPSEEK_MODEL', endpointUrl: 'https://api.deepseek.com', endpointEnvKey: 'DEEPSEEK_BASE_URL' };
  const many = Array.from({ length: 20 }, (_, index) => synced(`model-${String(index).padStart(2, '0')}`));

  it('keeps the native select up to 15 candidates', () => {
    setup({ initial, syncedModels: many.slice(0, 5) });
    expect(screen.getByLabelText('模型').tagName).toBe('SELECT');
  });

  it('switches to a filterable control above 15 candidates and accepts a custom id', async () => {
    const { onSave, user } = setup({ initial, syncedModels: many });
    const box = screen.getByRole('combobox', { name: '模型' });
    expect(box.tagName).toBe('INPUT');
    await user.click(box);
    expect(screen.getByText(t.form.syncedModelsGroup)).toBeInTheDocument();
    expect(within(screen.getByRole('listbox')).getAllByRole('option').length).toBeGreaterThan(15);
    await user.clear(box);
    await user.type(box, 'model-1');
    expect(within(screen.getByRole('listbox')).getAllByRole('option')).toHaveLength(10);
    await user.pointer({ keys: '[MouseLeft>]', target: screen.getByRole('option', { name: /model-13/ }) });
    expect(box).toHaveValue('model-13');
    await user.clear(box);
    await user.type(box, 'my-custom-model');
    await user.click(screen.getByRole('button', { name: t.form.save }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ model_id: 'my-custom-model' })));
  });
});
