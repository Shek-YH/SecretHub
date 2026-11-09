import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { ExportDialog } from './ExportDialog';
import { getTranslation } from './i18n/translations';
import { backend, type ExportRow } from './lib/backend';
import type { SecretMetadata } from './secretTypes';

const t = getTranslation('zh-CN');

const secrets: SecretMetadata[] = [
  { id: 'k1', name: 'Fake OpenAI', envKey: 'OPENAI_API_KEY', provider: 'openai', tags: [], status: 'valid', updated: '', valueType: 'api_key' },
  { id: 'g1', name: 'Fake server', envKey: '', provider: 'group', tags: [], status: 'unknown', updated: '', valueType: 'env_group', subtitle: '3 个变量' },
];

const row = (key: string, status: ExportRow['status'], patch: Partial<ExportRow> = {}): ExportRow => ({ key, secretId: 'g1', sourceName: 'Fake server', secret: false, display: 'plain', status, ...patch });

function mock(rows: ExportRow[] = [row('SSH_HOST', 'new')]) {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  return {
    group: vi.spyOn(backend, 'groupPublic').mockResolvedValue({ variables: [{ name: 'SSH_HOST', secret: false, value: 'h' }, { name: 'SSH_PASSWORD', secret: true }, { name: 'SSH_PORT', secret: false, value: '22' }], hasNotes: false, aiValues: true }),
    projects: vi.spyOn(backend, 'projects').mockResolvedValue(Array.from({ length: 10 }, (_, index) => ({ id: `p${index}`, path: `C:\\work\\project${index}`, display_name: `project${index}`, last_used_at: 100 - index }))),
    preview: vi.spyOn(backend, 'exportPreviewV2').mockResolvedValue({ rows }),
    apply: vi.spyOn(backend, 'exportApplyV2').mockResolvedValue({ written: ['SSH_HOST'], skipped: [] }),
    save: vi.spyOn(backend, 'saveProfile').mockResolvedValue(undefined),
    choose: vi.spyOn(backend, 'chooseFolder').mockResolvedValue('C:\\work\\chosen'),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

function open(props: Partial<React.ComponentProps<typeof ExportDialog>> = {}) {
  const onExported = vi.fn();
  render(<ExportDialog t={t} secrets={secrets} selectedIds={['k1', 'g1']} onClose={() => undefined} onExported={onExported} {...props} />);
  return { user: userEvent.setup(), onExported };
}

describe('export dialog', () => {
  it('lists the selected items and offers at most eight recent projects', async () => {
    mock();
    const { user } = open();
    expect(screen.getByText('Fake OpenAI')).toBeInTheDocument();
    expect(await screen.findByText('3 / 3 个变量')).toBeInTheDocument();
    const chips = await screen.findAllByRole('button', { name: /^project\d$/ });
    expect(chips).toHaveLength(8);
    await user.click(chips[2]);
    expect(screen.getByPlaceholderText('项目文件夹路径')).toHaveValue('C:\\work\\project2');
  });

  it('sends the chosen folder and the variable subset to the preview, whole groups without a list', async () => {
    const mocks = mock();
    const { user } = open();
    await screen.findByText('3 / 3 个变量');
    await user.click(screen.getByRole('button', { name: '选择项目目录' }));
    await waitFor(() => expect(mocks.preview).toHaveBeenCalled());
    expect(mocks.preview.mock.calls.at(-1)![0]).toEqual({ directory: 'C:\\work\\chosen', selections: [{ secretId: 'k1' }, { secretId: 'g1' }], skipKeys: [] });
    await user.click(screen.getByRole('button', { name: '展开变量' }));
    expect(screen.getByLabelText('SSH_HOST')).toBeChecked();
    await user.click(screen.getByLabelText('SSH_PASSWORD'));
    await waitFor(() => expect(mocks.preview.mock.calls.at(-1)![0].selections[1]).toEqual({ secretId: 'g1', variables: ['SSH_HOST', 'SSH_PORT'] }));
    expect(screen.getByText('2 / 3 个变量')).toBeInTheDocument();
  });

  it('starts from the subset of a profile', async () => {
    const mocks = mock();
    open({ initialVariables: { g1: ['SSH_PORT'] } });
    await screen.findByText('1 / 3 个变量');
    await userEvent.setup().type(screen.getByPlaceholderText('项目文件夹路径'), 'C:\\p');
    await waitFor(() => expect(mocks.preview.mock.calls.at(-1)![0].selections[1]).toEqual({ secretId: 'g1', variables: ['SSH_PORT'] }));
  });

  it('shows new, overwrite and conflict states, masks secrets and lets overwrites be skipped', async () => {
    const mocks = mock([
      row('SSH_HOST', 'new', { display: '203.0.113.7' }),
      row('SSH_PASSWORD', 'overwrite', { secret: true, display: '••••••••' }),
      row('SSH_PORT', 'overwrite', { display: '22' }),
    ]);
    const { user } = open({ selectedIds: ['g1'] });
    await user.type(screen.getByPlaceholderText('项目文件夹路径'), 'C:\\p');
    const table = await screen.findByRole('table', { name: '预览' });
    await within(table).findByText('203.0.113.7');
    expect(within(table).getAllByText('••••••••')).toHaveLength(1);
    expect(table.querySelector('[data-key="SSH_HOST"]')).toHaveAttribute('data-status', 'new');
    // Overwrites default to overwrite; choosing skip feeds skipKeys back into the preview and apply.
    await user.selectOptions(screen.getByLabelText('SSH_PORT 覆盖已有'), 'skip');
    await waitFor(() => expect(mocks.preview.mock.calls.at(-1)![0].skipKeys).toEqual(['SSH_PORT']));
    await user.click(screen.getByRole('button', { name: '写入 .env' }));
    await waitFor(() => expect(mocks.apply).toHaveBeenCalledTimes(1));
    expect(mocks.apply.mock.calls[0][0]).toEqual({ directory: 'C:\\p', selections: [{ secretId: 'g1' }], skipKeys: ['SSH_PORT'] });
    expect(await screen.findByText('已写入 1 个变量，跳过 0 个')).toBeInTheDocument();
  });

  it('blocks the write while two items produce the same name', async () => {
    const mocks = mock([row('SSH_HOST', 'conflict'), row('SSH_HOST', 'conflict', { secretId: 'g2', sourceName: 'Other server' })]);
    const { user } = open({ selectedIds: ['g1'] });
    await user.type(screen.getByPlaceholderText('项目文件夹路径'), 'C:\\p');
    expect(await screen.findByText(/同名变量，请取消其中一个/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '写入 .env' })).toBeDisabled();
    expect(mocks.apply).not.toHaveBeenCalled();
  });

  it('removes an item from the selection', async () => {
    const mocks = mock();
    const { user } = open();
    await screen.findByText('3 / 3 个变量');
    await user.type(screen.getByPlaceholderText('项目文件夹路径'), 'C:\\p');
    await user.click(screen.getByRole('button', { name: '取消选择 Fake OpenAI' }));
    await waitFor(() => expect(mocks.preview.mock.calls.at(-1)![0].selections).toEqual([{ secretId: 'g1' }]));
  });

  it('saves the current selection, including the variable subset, as a profile', async () => {
    const mocks = mock();
    const { user } = open();
    await screen.findByText('3 / 3 个变量');
    await user.click(screen.getByRole('button', { name: '展开变量' }));
    await user.click(screen.getByLabelText('SSH_PASSWORD'));
    await user.type(screen.getByLabelText('方案名称'), 'Deploy');
    await user.click(screen.getByRole('button', { name: '存为方案' }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(1));
    const saved = mocks.save.mock.calls[0][0];
    expect(saved).toMatchObject({ name: 'Deploy', secret_ids: ['k1', 'g1'], variables: { g1: ['SSH_HOST', 'SSH_PORT'] } });
    expect(await screen.findByText('已保存为方案')).toBeInTheDocument();
  });
});

describe('profile detail', () => {
  it('shows the variable subset of a group and saves it back unchanged', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const save = vi.spyOn(backend, 'saveProfile').mockResolvedValue(undefined);
    const { ProfileDetailModal } = await import('./App');
    const user = userEvent.setup();
    render(<ProfileDetailModal t={t} profile={{ id: 'p1', name: 'Deploy', description: '', secret_ids: ['k1', 'g1'], variables: { g1: ['SSH_HOST', 'SSH_PORT'] } }} secrets={secrets} onClose={() => undefined} onSaved={() => undefined} />);
    expect(screen.getByText('子集: SSH_HOST, SSH_PORT')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '保存组合' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][0]).toMatchObject({ id: 'p1', secret_ids: ['k1', 'g1'], variables: { g1: ['SSH_HOST', 'SSH_PORT'] } });
  });
});
