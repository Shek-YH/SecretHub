import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { ModelSyncSection } from './App';
import { SecretForm } from './SecretForm';
import { CapabilityModelsEditor, capabilityError, needsCapabilitySection, normalizeRows, validateCapabilityRows } from './CapabilityModels';
import { getTranslation } from './i18n/translations';
import type { BackendModel, CapabilityModelRow } from './lib/backend';
import { useState } from 'react';

const t = getTranslation('zh-CN');
const synced = (id: string, capabilities: string[]): BackendModel => ({ model_id: id, display_name: '', input: [], output: [], capabilities, capability_source: 'provider', synced_at: 1 });
const models = [synced('chat-a', ['chat']), synced('img-a', ['image_gen']), synced('img-b', ['image_gen', 'vision']), synced('voice-a', ['tts'])];
const row = (capability: string, model_id: string, env_key: string): CapabilityModelRow => ({ capability, model_id, env_key });

function Harness({ initial = [] as CapabilityModelRow[], onChange = (_rows: CapabilityModelRow[]) => undefined }) {
  const [rows, setRows] = useState(initial);
  return <CapabilityModelsEditor t={t} models={models} value={rows} primaryModelId="chat-a" onChange={(next) => { setRows(next); onChange(next); }} />;
}

describe('capability model rules', () => {
  it('shows the section for aggregators or when synced models span two capabilities', () => {
    expect(needsCapabilitySection('aggregator', [])).toBe(true);
    expect(needsCapabilitySection('direct', [synced('a', ['chat']), synced('b', ['chat'])])).toBe(false);
    expect(needsCapabilitySection('direct', [synced('a', ['chat']), synced('b', ['image_gen'])])).toBe(true);
  });

  it('validates env names: format, uniqueness and reserved names; empty names are allowed', () => {
    expect(validateCapabilityRows([row('code', 'm', 'CODE_MODEL'), row('tts', 'v', '')], [])).toBeNull();
    expect(validateCapabilityRows([row('code', 'm', 'bad-name')], [])).toBe('invalid');
    expect(validateCapabilityRows([row('code', 'm', 'X'), row('tts', 'v', 'X')], [])).toBe('duplicate');
    expect(validateCapabilityRows([row('code', 'm', 'OPENROUTER_API_KEY')], ['OPENROUTER_API_KEY'])).toBe('duplicate');
    // A cleared model does not claim its variable name.
    expect(validateCapabilityRows([row('code', '', 'X'), row('tts', 'v', 'X')], [])).toBeNull();
    expect(capabilityError(t, [row('code', 'm', 'bad')], [])).toBe(t.cap.invalidEnv);
    expect(normalizeRows([row('tts', 'v', 'tts_model'), row('code', ' ', 'X'), row('image_gen', 'i', '')]).map((item) => [item.capability, item.env_key])).toEqual([['image_gen', ''], ['tts', 'TTS_MODEL']]);
  });
});

describe('CapabilityModelsEditor', () => {
  it('offers only models that have the capability and says so when there are none', () => {
    render(<Harness />);
    const image = screen.getByLabelText('生图 默认模型') as HTMLSelectElement;
    expect(Array.from(image.options).map((option) => option.value)).toEqual(['', 'img-a', 'img-b']);
    const vision = screen.getByLabelText('视觉理解 默认模型') as HTMLSelectElement;
    expect(Array.from(vision.options).map((option) => option.value)).toEqual(['', 'img-b']);
    // code has no candidate: no dropdown, a hint instead
    expect(screen.queryByLabelText('代码 默认模型')).not.toBeInTheDocument();
    expect(screen.getAllByText(t.cap.noModels).length).toBeGreaterThan(0);
    expect(screen.getByText('chat-a')).toBeInTheDocument();
  });

  it('fills the default variable name on pick, keeps edits, and clears the row when unset', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    const select = screen.getByLabelText('生图 默认模型');
    const env = screen.getByLabelText('生图 环境变量名') as HTMLInputElement;
    expect(env).toBeDisabled();
    await user.selectOptions(select, 'img-a');
    expect(env.value).toBe('IMAGE_MODEL');
    await user.clear(env);
    await user.type(env, 'my_image');
    expect(env.value).toBe('MY_IMAGE');
    await user.selectOptions(select, 'img-b');
    expect(env.value).toBe('MY_IMAGE');
    expect(onChange).toHaveBeenLastCalledWith([row('image_gen', 'img-b', 'MY_IMAGE')]);
    await user.selectOptions(select, '');
    expect(env.value).toBe('');
    expect(env).toBeDisabled();
  });
});

describe('CapabilityModelsEditor with many candidates', () => {
  it('uses a searchable control above 15 candidates and allows a typed id', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const many = Array.from({ length: 20 }, (_, index) => synced(`img-${String(index).padStart(2, '0')}`, ['image_gen']));
    function Stateful() {
      const [rows, setRows] = useState<CapabilityModelRow[]>([]);
      return <CapabilityModelsEditor t={t} models={many} value={rows} primaryModelId="chat-a" onChange={(next) => { setRows(next); onChange(next); }} />;
    }
    render(<Stateful />);
    const box = screen.getByRole('combobox', { name: '生图 默认模型' });
    await user.click(box);
    expect(within(screen.getByRole('listbox')).getAllByRole('option')).toHaveLength(20);
    await user.type(box, 'img-1');
    expect(within(screen.getByRole('listbox')).getAllByRole('option')).toHaveLength(10);
    await user.type(box, 'x');
    expect(screen.getByText(t.quick.modelNoMatch)).toBeInTheDocument();
    expect(onChange).toHaveBeenLastCalledWith([row('image_gen', 'img-1x', 'IMAGE_MODEL')]);
  });
});

describe('SecretForm capability section', () => {
  const aggregator = { id: 's1', name: 'OR', envKey: 'OPENROUTER_API_KEY', provider: 'openrouter', tags: [], status: 'valid', updated: 'now', valueType: 'api_key', modelId: '', modelEnvKey: 'OPENROUTER_MODEL', endpointUrl: 'https://openrouter.ai/api/v1', endpointEnvKey: 'OPENROUTER_BASE_URL' };

  it('lets a new aggregator key be saved without a primary model and explains the auto sync', async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<SecretForm t={t} knownTags={[]} initial={null} preset={{ provider: 'openrouter', name: 'OR', envKey: 'OPENROUTER_API_KEY', modelId: '', modelEnvKey: 'OPENROUTER_MODEL', endpointUrl: 'https://openrouter.ai/api/v1', endpointEnvKey: 'OPENROUTER_BASE_URL' }} onClose={() => undefined} onSave={onSave} />);
    expect(screen.getByText(t.cap.newKeyHint)).toBeInTheDocument();
    expect(screen.queryByText(t.cap.title)).not.toBeInTheDocument();
    await user.type(screen.getByLabelText(t.quick.pasteKey), 'fake-test-value');
    await user.click(screen.getByRole('button', { name: t.form.save }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ provider_id: 'openrouter', model_id: '' })));
    expect(onSave.mock.calls[0][0]).not.toHaveProperty('capability_models');
  });

  it('still requires a model for a direct provider', async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(<SecretForm t={t} knownTags={[]} initial={null} preset={{ provider: 'deepseek', name: 'DS', envKey: 'DEEPSEEK_API_KEY', modelId: '', modelEnvKey: 'DEEPSEEK_MODEL', endpointUrl: 'https://api.deepseek.com', endpointEnvKey: 'DEEPSEEK_BASE_URL' }} onClose={() => undefined} onSave={onSave} />);
    await user.type(screen.getByLabelText(t.quick.pasteKey), 'fake-test-value');
    await user.click(screen.getByRole('button', { name: t.form.save }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.form.apiKeyFieldsRequired);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('edits capability rows for an aggregator key and submits them normalized', async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<SecretForm t={t} knownTags={[]} initial={aggregator} syncedModels={models} capabilityRows={[row('tts', 'voice-a', 'TTS_MODEL')]} onClose={() => undefined} onSave={onSave} />);
    expect(screen.getByText(t.cap.title)).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText('生图 默认模型'), 'img-a');
    await user.click(screen.getByRole('button', { name: t.form.save }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave.mock.calls[0][0].capability_models).toEqual([row('image_gen', 'img-a', 'IMAGE_MODEL'), row('tts', 'voice-a', 'TTS_MODEL')]);
  });

  it('rejects duplicate variable names, including the key\'s own names', async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(<SecretForm t={t} knownTags={[]} initial={aggregator} syncedModels={models} capabilityRows={[row('tts', 'voice-a', 'OPENROUTER_MODEL')]} onClose={() => undefined} onSave={onSave} />);
    await user.click(screen.getByRole('button', { name: t.form.save }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.cap.duplicateEnv);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('hides the section for a direct key whose models cover one capability', () => {
    render(<SecretForm t={t} knownTags={[]} initial={{ ...aggregator, provider: 'deepseek' }} syncedModels={[synced('a', ['chat']), synced('b', ['chat'])]} onClose={() => undefined} onSave={vi.fn()} />);
    expect(screen.queryByText(t.cap.title)).not.toBeInTheDocument();
  });
});

describe('model list capability menu', () => {
  it('offers one entry per non-chat capability and reports the pick', async () => {
    const user = userEvent.setup();
    const onPick = vi.fn();
    render(<ModelSyncSection t={t} locale="zh-CN" status="ok" syncedAt={1} models={models} syncing={false} message="" onSync={() => undefined} capabilityRows={[row('vision', 'img-b', 'VISION_MODEL')]} onSetCapabilityDefault={onPick} />);
    expect(screen.queryByLabelText('设为能力默认 chat-a')).not.toBeInTheDocument();
    const menu = screen.getByLabelText('设为能力默认 img-b') as HTMLSelectElement;
    expect(Array.from(menu.options).map((option) => option.textContent)).toEqual(['设为能力默认…', '设为视觉理解默认 ✓', '设为生图默认']);
    await user.selectOptions(menu, 'image_gen');
    expect(onPick).toHaveBeenCalledWith('image_gen', 'img-b');
  });
});
