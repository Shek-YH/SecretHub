import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { getTranslation } from '../i18n/translations';
import { backend } from '../lib/backend';
import type { SecretMetadata } from '../secretTypes';
import { ItemDetail } from './ItemDetail';

const t = getTranslation('zh-CN');
const FAKE_PASSWORD = 'fake-password';
const FAKE_CARD = '4111111111111111';
const FAKE_BODY = 'fake private note text';

const secret = (patch: Partial<SecretMetadata>): SecretMetadata => ({ id: 's1', name: 'Fake item', envKey: '', provider: 'personal', tags: ['personal-tag'], status: 'unknown', updated: '', updatedAt: 1, ...patch });

function mock(options: { fields?: Record<string, string>; secretFields?: string[]; body?: string } = {}) {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  return {
    publicFields: vi.spyOn(backend, 'itemPublicFields').mockResolvedValue(options.fields ?? {}),
    secretFields: vi.spyOn(backend, 'itemSecretFields').mockResolvedValue(options.secretFields ?? []),
    reveal: vi.spyOn(backend, 'itemFieldReveal').mockResolvedValue(undefined),
    copy: vi.spyOn(backend, 'itemFieldCopy').mockResolvedValue(undefined),
    noteBody: vi.spyOn(backend, 'itemNoteBody').mockResolvedValue(options.body ?? FAKE_BODY),
    openUrl: vi.spyOn(backend, 'openExternalUrl').mockResolvedValue(undefined),
    // Anything that could hand a secret to the page must stay untouched.
    legacyReveal: vi.spyOn(backend, 'reveal').mockResolvedValue(undefined),
    legacyCopy: vi.spyOn(backend, 'copy').mockResolvedValue(undefined),
    copyAll: vi.spyOn(backend, 'copyAll').mockResolvedValue(undefined),
    validate: vi.spyOn(backend, 'validate').mockResolvedValue('valid'),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe('login detail', () => {
  const loginFields = { website: 'https://accounts.example.com', username: 'fake-user@example.com', notes: 'fake notes' };

  it('shows plain fields, dots for secrets and no development tools', async () => {
    const calls = mock({ fields: loginFields, secretFields: ['password', 'totp'] });
    render(<ItemDetail t={t} secret={secret({ valueType: 'login', subtitle: 'accounts.example.com' })} onEdit={() => undefined} onDelete={() => undefined} />);
    expect(await screen.findByText('fake-user@example.com')).toBeInTheDocument();
    expect(calls.publicFields).toHaveBeenCalledWith('s1');
    expect(calls.secretFields).toHaveBeenCalledWith('s1');
    expect(screen.getByText('https://accounts.example.com')).toBeInTheDocument();
    expect(screen.getByText('accounts.example.com')).toBeInTheDocument();
    for (const name of ['密码', '两步验证密钥']) expect(screen.getByLabelText(new RegExp(`^${name}: `))).toHaveTextContent(/^•+$/);
    expect(screen.getByText('个人信息：AI 看不到，也不会被导出。')).toBeInTheDocument();
    // No detection, model sync or copy-all for personal items.
    for (const name of ['验证', /复制全部内容/, /同步模型/, /Reveal/, /复制 API Key/]) expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
    expect(document.body.textContent).not.toContain(FAKE_PASSWORD);
    expect(calls.noteBody).not.toHaveBeenCalled();
  });

  it('reveals through the native dialog and copies through the backend, never loading the value', async () => {
    const calls = mock({ fields: loginFields, secretFields: ['password'] });
    const user = userEvent.setup();
    render(<ItemDetail t={t} secret={secret({ valueType: 'login' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await screen.findByText('fake-user@example.com');
    await user.click(screen.getByRole('button', { name: '显示 密码' }));
    expect(calls.reveal).toHaveBeenCalledWith('s1', 'password');
    await user.click(screen.getByRole('button', { name: '复制 密码' }));
    expect(calls.copy).toHaveBeenCalledWith('s1', 'password');
    expect(await screen.findByRole('status')).toHaveTextContent('密码: 已复制');
    await user.click(screen.getByRole('button', { name: '复制 账号' }));
    expect(calls.copy).toHaveBeenCalledWith('s1', 'username');
    // The page only ever received the dots and the plain fields.
    expect(document.body.innerHTML).not.toContain(FAKE_PASSWORD);
    expect(calls.legacyReveal).not.toHaveBeenCalled();
    expect(calls.legacyCopy).not.toHaveBeenCalled();
    expect(calls.copyAll).not.toHaveBeenCalled();
    expect(calls.validate).not.toHaveBeenCalled();
  });

  it('opens the website only when it is https', async () => {
    const calls = mock({ fields: loginFields, secretFields: [] });
    const user = userEvent.setup();
    const { rerender } = render(<ItemDetail t={t} secret={secret({ valueType: 'login' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await user.click(await screen.findByRole('button', { name: '打开网站' }));
    expect(calls.openUrl).toHaveBeenCalledWith('https://accounts.example.com');
    calls.publicFields.mockResolvedValue({ ...loginFields, website: 'http://plain.example.com' });
    rerender(<ItemDetail t={t} secret={secret({ valueType: 'login', id: 's2' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await screen.findByText('http://plain.example.com');
    expect(screen.queryByRole('button', { name: '打开网站' })).not.toBeInTheDocument();
  });

  it('reports a failed action and lists the dev-test variable names without values', async () => {
    const calls = mock({ fields: loginFields, secretFields: ['password'] });
    calls.copy.mockRejectedValue(new Error('clipboard unavailable'));
    const user = userEvent.setup();
    render(<ItemDetail t={t} secret={secret({ valueType: 'login', devUse: true, envKey: 'DOUYIN_TEST' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await screen.findByText('fake-user@example.com');
    expect(screen.getByText('DOUYIN_TEST_USERNAME, DOUYIN_TEST_PASSWORD')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '复制 密码' }));
    expect(await screen.findByRole('status')).toHaveTextContent('clipboard unavailable');
  });

  it('wires the edit and delete buttons', async () => {
    mock({ fields: loginFields });
    const onEdit = vi.fn();
    const onDelete = vi.fn();
    const user = userEvent.setup();
    render(<ItemDetail t={t} secret={secret({ valueType: 'login' })} onEdit={onEdit} onDelete={onDelete} />);
    await screen.findByText('fake-user@example.com');
    await user.click(screen.getByRole('button', { name: '编辑' }));
    await user.click(screen.getByRole('button', { name: '删除' }));
    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(onDelete).toHaveBeenCalledTimes(1);
  });
});

describe('bank card detail', () => {
  it('draws dots only for the secrets that exist and copies the number by name', async () => {
    const calls = mock({ fields: { bank: 'Fake Bank', holder: 'FAKE HOLDER', expiry: '09/30' }, secretFields: ['card_number', 'cvv'] });
    const user = userEvent.setup();
    render(<ItemDetail t={t} secret={secret({ valueType: 'bank_card', subtitle: 'Fake Bank · 尾号 1111' })} onEdit={() => undefined} onDelete={() => undefined} />);
    expect(await screen.findByText('FAKE HOLDER')).toBeInTheDocument();
    expect(screen.getByText('09/30')).toBeInTheDocument();
    expect(screen.getByLabelText(/^卡号: /)).toHaveTextContent(/^•+$/);
    expect(screen.getByLabelText(/^CVV: /)).toBeInTheDocument();
    expect(screen.queryByLabelText(/^密码: /)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '显示 卡号' }));
    expect(calls.reveal).toHaveBeenCalledWith('s1', 'card_number');
    await user.click(screen.getByRole('button', { name: '复制 卡号' }));
    expect(calls.copy).toHaveBeenCalledWith('s1', 'card_number');
    expect(document.body.innerHTML).not.toContain(FAKE_CARD);
    expect(document.body.innerHTML).not.toContain('4111 1111');
  });
});

describe('note detail', () => {
  it('loads the body only on request and drops it again', async () => {
    const calls = mock({ secretFields: ['body'] });
    const user = userEvent.setup();
    render(<ItemDetail t={t} secret={secret({ valueType: 'note' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await screen.findByRole('button', { name: '显示正文' });
    expect(calls.noteBody).not.toHaveBeenCalled();
    expect(document.body.textContent).not.toContain(FAKE_BODY);
    await user.click(screen.getByRole('button', { name: '显示正文' }));
    expect(calls.noteBody).toHaveBeenCalledWith('s1');
    expect(await screen.findByText(FAKE_BODY)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '隐藏正文' }));
    expect(screen.queryByText(FAKE_BODY)).not.toBeInTheDocument();
    expect(calls.reveal).not.toHaveBeenCalled();
  });

  it('clears the body when another record is selected', async () => {
    mock({ secretFields: ['body'] });
    const user = userEvent.setup();
    const { rerender } = render(<ItemDetail key="s1" t={t} secret={secret({ valueType: 'note' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await user.click(await screen.findByRole('button', { name: '显示正文' }));
    expect(await screen.findByText(FAKE_BODY)).toBeInTheDocument();
    rerender(<ItemDetail key="s2" t={t} secret={secret({ valueType: 'note', id: 's2', name: 'Other' })} onEdit={() => undefined} onDelete={() => undefined} />);
    await waitFor(() => expect(screen.queryByText(FAKE_BODY)).not.toBeInTheDocument());
    expect(within(document.body).getByRole('button', { name: '显示正文' })).toBeInTheDocument();
  });
});
