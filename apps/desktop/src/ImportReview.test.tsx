import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { App } from './App';
import { ImportPanel } from './ImportPanel';
import { ImportReviewModal, summarize } from './ImportReview';
import { getTranslation } from './i18n/translations';
import { backend, type BackendSecret, type ImportReview } from './lib/backend';

const t = getTranslation('zh-CN');
const FAKE_VALUE = 'fake-never-rendered-value';

const review: ImportReview = {
  sessionId: 'session-1',
  items: [
    { id: 'i0', kind: 'group', source: '服务器A', name: '服务器A', hasNotes: true, dedupe: { state: 'new', options: [], default: 'create', added: [], changed: [], missing: [] }, variables: [
      { name: 'SSH_HOST', secret: false, values: 1, pick: 0, conflict: false },
      { name: 'SSH_PRIVATE_KEY', secret: true, values: 2, pick: 1, conflict: false },
      { name: 'SSH_PRIVATE_KEY_2', secret: true, values: 1, pick: 0, conflict: true },
      { name: 'DASHSCOPE_API_KEY', secret: true, values: 1, pick: 0, conflict: false, apiKeyItem: 'i1' },
    ] },
    { id: 'i1', kind: 'api_key', source: '服务器A', name: '通义千问 / Qwen', provider: '通义千问 / Qwen', splitFrom: 'i0', hasNotes: false, dedupe: { state: 'differs', options: ['overwrite', 'keep_both', 'skip'], default: 'keep_both', added: [], changed: [], missing: [] }, variables: [{ name: 'DASHSCOPE_API_KEY', secret: true, values: 1, pick: 0, conflict: false }] },
    { id: 'i2', kind: 'group', source: '服务器B', name: '服务器B', hasNotes: false, dedupe: { state: 'differs', options: ['merge', 'copy', 'skip'], default: 'merge', added: ['NEW_VAR'], changed: ['SSH_HOST'], missing: ['OLD_VAR'] }, variables: [{ name: 'SSH_HOST', secret: false, values: 1, pick: 0, conflict: false }] },
    { id: 'i3', kind: 'group', source: '充值', name: '充值', hasNotes: true, dedupe: { state: 'same', options: ['skip'], default: 'skip', added: [], changed: [], missing: [] }, variables: [] },
  ],
  skipped: [{ name: 'empty.json', reason: 'empty' }],
};

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe('import review', () => {
  it('groups items by source, shows names, flags and dedupe state, and never a value', () => {
    render(<ImportReviewModal t={t} review={review} onClose={() => undefined} onDone={() => undefined} />);
    const dialog = screen.getByRole('dialog', { name: '确认导入' });
    expect(within(dialog).getByRole('region', { name: '服务器A' })).toBeInTheDocument();
    expect(within(dialog).getByRole('region', { name: '服务器B' })).toBeInTheDocument();
    const first = dialog.querySelector('[data-item="i0"]') as HTMLElement;
    expect(within(first).getByText('SSH_HOST')).toBeInTheDocument();
    expect(within(first).getByText('密钥冲突')).toBeInTheDocument();
    expect(within(first).getByLabelText('机密 SSH_PRIVATE_KEY')).toBeChecked();
    expect(within(first).getByLabelText('机密 SSH_HOST')).not.toBeChecked();
    expect(within(first).getByText(/同名重复 ×2/)).toBeInTheDocument();
    expect(within(first).getByText('含备注')).toBeInTheDocument();
    const key = dialog.querySelector('[data-item="i1"]') as HTMLElement;
    expect(within(key).getByText('从配置组「服务器A」中拆出')).toBeInTheDocument();
    const differs = dialog.querySelector('[data-item="i2"]') as HTMLElement;
    expect(within(differs).getByText('新增变量: NEW_VAR')).toBeInTheDocument();
    expect(within(differs).getByText('变更变量: SSH_HOST')).toBeInTheDocument();
    expect(within(differs).getByText('组内已有、导入中没有: OLD_VAR')).toBeInTheDocument();
    expect(within(differs).getByLabelText('服务器B 已存在，内容有差异')).toHaveValue('merge');
    // Identical items start unchecked; skipped files are listed with a reason.
    expect(within(dialog.querySelector('[data-item="i3"]') as HTMLElement).getByLabelText('导入 充值')).not.toBeChecked();
    expect(within(dialog).getByText('空文件')).toBeInTheDocument();
    expect(dialog.textContent).not.toContain(FAKE_VALUE);
  });

  it('updates the summary with the user choices', async () => {
    const user = userEvent.setup();
    render(<ImportReviewModal t={t} review={review} onClose={() => undefined} onDone={() => undefined} />);
    // Defaults: group A new (add), key differs keep_both (add), group B merge (update), group C same (skip).
    expect(screen.getByText('新增 2 / 更新 1 / 跳过 1')).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText('服务器B 已存在，内容有差异'), 'copy');
    expect(screen.getByText('新增 3 / 更新 0 / 跳过 1')).toBeInTheDocument();
    await user.click(screen.getByLabelText('导入 服务器A'));
    expect(screen.getByText('新增 2 / 更新 0 / 跳过 2')).toBeInTheDocument();
    // Keeping the key in its group means it is neither added nor skipped.
    await user.selectOptions(screen.getByLabelText('通义千问 / Qwen 拆为 API Key'), 'keep');
    expect(screen.getByText('新增 1 / 更新 0 / 跳过 2')).toBeInTheDocument();
    expect(summarize(review, {}).added).toBe(2);
  });

  it('sends the decisions: includes, actions, split, picks and secret overrides', async () => {
    const apply = vi.spyOn(backend, 'importApply').mockResolvedValue({ added: ['服务器A'], updated: ['服务器B'], skipped: [], apiKeyIds: [] });
    const onDone = vi.fn();
    const user = userEvent.setup();
    render(<ImportReviewModal t={t} review={review} onClose={() => undefined} onDone={onDone} />);
    await user.selectOptions(screen.getByLabelText('保留哪一个 SSH_PRIVATE_KEY'), '0');
    await user.click(within(document.querySelector('[data-item="i0"]') as HTMLElement).getByLabelText('机密 SSH_HOST'));
    await user.selectOptions(screen.getByLabelText('通义千问 / Qwen 拆为 API Key'), 'keep');
    await user.selectOptions(screen.getByLabelText('服务器B 已存在，内容有差异'), 'skip');
    await user.click(screen.getByRole('button', { name: '确认导入' }));
    await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
    const [sessionId, decisions] = apply.mock.calls[0];
    expect(sessionId).toBe('session-1');
    expect(decisions.items).toEqual([
      { id: 'i0', include: true, picks: { SSH_PRIVATE_KEY: 0 }, secrets: { SSH_HOST: true } },
      { id: 'i1', include: true, split: false },
      { id: 'i2', include: true, action: 'skip' },
      { id: 'i3', include: false },
    ]);
    expect(await screen.findByText('导入完成：新增 1，更新 1，跳过 0')).toBeInTheDocument();
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it('drops the scan when cancelled', async () => {
    const cancel = vi.spyOn(backend, 'importCancel').mockResolvedValue(undefined);
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<ImportReviewModal t={t} review={review} onClose={onClose} onDone={() => undefined} />);
    await user.click(screen.getByText('取消', { selector: 'button' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(cancel).toHaveBeenCalledWith('session-1');
  });
});

describe('import panel folder button', () => {
  it('picks a folder, scans it locally and opens the review', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const choose = vi.spyOn(backend, 'chooseFolder').mockResolvedValue('C:\\资料');
    const scan = vi.spyOn(backend, 'importScan').mockResolvedValue(review);
    const user = userEvent.setup();
    render(<ImportPanel t={t} onClose={() => undefined} onDone={() => undefined} />);
    await user.click(screen.getByRole('button', { name: '选择文件夹' }));
    expect(await screen.findByRole('dialog', { name: '确认导入' })).toBeInTheDocument();
    expect(choose).toHaveBeenCalled();
    expect(scan).toHaveBeenCalledWith(['C:\\资料']);
  });

  it('shows the scan error without any content', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'chooseFolder').mockResolvedValue('C:\\空');
    vi.spyOn(backend, 'importScan').mockRejectedValue('nothing importable was found in the selected paths');
    const user = userEvent.setup();
    render(<ImportPanel t={t} onClose={() => undefined} onDone={() => undefined} />);
    await user.click(screen.getByRole('button', { name: '选择文件夹' }));
    expect(await screen.findByText(/nothing importable/)).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: '确认导入' })).toBeNull();
  });
});

describe('AI import request', () => {
  const row = (patch: Partial<BackendSecret>): BackendSecret => ({
    id: 'x', name: 'x', provider_id: 'generic', env_key: '', description: '', tags: [], status: 'unknown', value_type: 'dev_secret', category: 'other', scope: 'global', favorite: false, archived: false,
    model_id: '', model_env_key: '', endpoint_url: '', endpoint_env_key: '', models_synced_at: 0, models_sync_status: '', created_at: 1, updated_at: Math.floor(Date.now() / 1000) - 60, validated_at: 0, subtitle: '', dev_use: false, ...patch,
  });

  function mockDesktop() {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(backend, 'status').mockResolvedValue({ initialized: true, unlocked: true });
    vi.spyOn(backend, 'list').mockResolvedValue([row({ id: 'a', name: 'Fake Dev' })]);
    vi.spyOn(backend, 'audit').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingPlans').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingCapabilitySuggestions').mockResolvedValue([]);
    vi.spyOn(backend, 'touch').mockResolvedValue(undefined);
    vi.spyOn(backend, 'validateMany').mockResolvedValue([]);
    vi.spyOn(backend, 'pendingImportRequests').mockResolvedValue([{ requestId: 'import-1-aa', paths: ['C:\\资料\\服务器'], createdAt: 't' }]);
  }

  it('shows the paths, scans only on View and reports the decision on Decline', async () => {
    mockDesktop();
    const scan = vi.spyOn(backend, 'importScan').mockResolvedValue({ ...review, requestId: 'import-1-aa' });
    const decline = vi.spyOn(backend, 'importDecline').mockResolvedValue(undefined);
    const user = userEvent.setup();
    render(<App />);
    const banner = await screen.findByRole('region', { name: /AI 请求导入/ });
    expect(within(banner).getByText('C:\\资料\\服务器')).toBeInTheDocument();
    expect(scan).not.toHaveBeenCalled();
    await user.click(within(banner).getByRole('button', { name: '查看' }));
    expect(await screen.findByRole('dialog', { name: '确认导入' })).toBeInTheDocument();
    expect(scan).toHaveBeenCalledWith(['C:\\资料\\服务器'], 'import-1-aa');
    vi.spyOn(backend, 'importCancel').mockResolvedValue(undefined);
    await user.click(screen.getByText('取消', { selector: 'button' }));
    await user.click(within(await screen.findByRole('region', { name: /AI 请求导入/ })).getByRole('button', { name: '拒绝' }));
    await waitFor(() => expect(decline).toHaveBeenCalledWith('import-1-aa'));
  });
});

describe('I2 sources in the import panel', () => {
  const loginReview: ImportReview = {
    sessionId: 'session-2',
    items: [
      { id: 'i0', kind: 'login', source: '粘贴文本', name: 'panel.example.com', hasNotes: false, fields: ['password', 'username', 'website'], dedupe: { state: 'differs', options: ['update', 'keep_both', 'skip'], default: 'keep_both', added: [], changed: ['password'], missing: [] }, variables: [] },
      { id: 'i1', kind: 'bank_card', source: '粘贴文本', name: 'Fake Bank 尾号 1111', hasNotes: false, fields: ['bank', 'card_number'], dedupe: { state: 'new', options: [], default: 'create', added: [], changed: [], missing: [] }, variables: [] },
    ],
    skipped: [{ name: 'passwords.csv', reason: 'invalid_row' }],
  };

  beforeEach(() => { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; });

  it('lists the field names of logins and cards in the user language, never values', () => {
    render(<ImportReviewModal t={t} review={loginReview} onClose={() => undefined} onDone={() => undefined} />);
    const login = document.querySelector('[data-item="i0"]') as HTMLElement;
    expect(within(login).getByText('将保存的字段: 密码, 账号, 网址')).toBeInTheDocument();
    expect(within(login).getByText('变更变量: password')).toBeInTheDocument();
    expect(within(login).getByLabelText('panel.example.com 已存在，内容有差异')).toHaveValue('keep_both');
    expect(within(login).getByText('账号密码')).toBeInTheDocument();
    expect(within(document.querySelector('[data-item="i1"]') as HTMLElement).getByText('银行卡')).toBeInTheDocument();
    expect(screen.getByText(/passwords.csv/)).toBeInTheDocument();
  });

  it('opens the review for chosen files (password exports, keys)', async () => {
    const choose = vi.spyOn(backend, 'importChooseFiles').mockResolvedValue(['C:/x/passwords.csv', 'C:/x/id_ed25519']);
    const scan = vi.spyOn(backend, 'importScan').mockResolvedValue(loginReview);
    const user = userEvent.setup();
    render(<ImportPanel t={t} onClose={() => undefined} onDone={() => undefined} />);
    await user.click(screen.getByRole('button', { name: '选择文件（密码导出 CSV、密钥等）' }));
    expect(await screen.findByRole('dialog', { name: '确认导入' })).toBeInTheDocument();
    expect(choose).toHaveBeenCalled();
    expect(scan).toHaveBeenCalledWith(['C:/x/passwords.csv', 'C:/x/id_ed25519']);
  });

  it('offers the bank card template with the CVV and password notice', async () => {
    const save = vi.spyOn(backend, 'importSaveBankTemplate').mockResolvedValue('C:/x/bank-cards-template.csv');
    const user = userEvent.setup();
    render(<ImportPanel t={t} onClose={() => undefined} onDone={() => undefined} />);
    expect(screen.getByText(/模板不含 CVV 和密码列/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '下载银行卡模板' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(await screen.findByText('模板已保存')).toBeInTheDocument();
  });

  it('sends pasted text to the local recognizer, not to the format preview, and needs some text', async () => {
    const scanText = vi.spyOn(backend, 'importScanText').mockResolvedValue(loginReview);
    const preview = vi.spyOn(backend, 'importPreview').mockResolvedValue([]);
    const user = userEvent.setup();
    render(<ImportPanel t={t} onClose={() => undefined} onDone={() => undefined} />);
    const smart = screen.getByRole('button', { name: '智能识别并审核' });
    expect(smart).toBeDisabled();
    await user.type(screen.getByLabelText('粘贴 .env 或 JSON 内容'), '账号：fake-user{enter}密码：fake-pass');
    expect(smart).toBeEnabled();
    await user.click(smart);
    expect(await screen.findByRole('dialog', { name: '确认导入' })).toBeInTheDocument();
    expect(scanText).toHaveBeenCalledWith('账号：fake-user\n密码：fake-pass');
    expect(preview).not.toHaveBeenCalled();
  });
});
