import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { getTranslation } from '../i18n/translations';
import type { SecretMetadata } from '../secretTypes';
import { BankCardForm } from './BankCardForm';
import { DevSecretForm } from './DevSecretForm';
import { LoginForm } from './LoginForm';
import { NoteForm } from './NoteForm';

const t = getTranslation('zh-CN');
const FAKE_PASSWORD = 'fake-password';
const FAKE_CARD = '4111 1111 1111 1111';

function setup<P extends { onSave: unknown; onClose: unknown }>(Form: (props: P) => React.ReactElement, props: Omit<P, 'onSave' | 'onClose'>) {
  const onSave = vi.fn().mockResolvedValue(undefined);
  const onClose = vi.fn();
  render(<Form {...({ ...props, onSave, onClose } as unknown as P)} />);
  return { onSave, onClose, user: userEvent.setup() };
}

const base = { t, knownTags: [] as string[], initial: null };
const stored = (patch: Partial<SecretMetadata>): SecretMetadata => ({ id: 's1', name: 'Fake', envKey: '', provider: 'personal', tags: ['work'], status: 'unknown', updated: '', favorite: true, ...patch });

describe('login form', () => {
  it('shows the login fields and saves them as a typed payload', async () => {
    const { onSave, onClose, user } = setup(LoginForm, base);
    const form = screen.getByRole('dialog', { name: '添加账号密码' });
    for (const label of ['名称', '网址', '账号', '密码']) expect(within(form).getByLabelText(label)).toBeInTheDocument();
    // The two-step secret and notes are folded under "advanced".
    expect(within(form).queryByLabelText('两步验证密钥（Base32）')).not.toBeInTheDocument();
    await user.type(within(form).getByLabelText('名称'), 'Fake site');
    await user.type(within(form).getByLabelText('网址'), 'https://accounts.example.com');
    await user.type(within(form).getByLabelText('账号'), 'fake-user@example.com');
    await user.type(within(form).getByLabelText('密码'), FAKE_PASSWORD);
    await user.click(within(form).getByRole('checkbox', { name: '加入收藏' }));
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave).toHaveBeenCalledWith({
      itemType: 'login',
      name: 'Fake site',
      fields: { username: 'fake-user@example.com', website: 'https://accounts.example.com', password: FAKE_PASSWORD },
      clear: [],
      tags: [],
      favorite: true,
      devUse: false,
      envKey: '',
    });
    expect(onClose).toHaveBeenCalled();
  });

  it('masks the password until asked and checks required fields and formats', async () => {
    const { onSave, user } = setup(LoginForm, base);
    const password = screen.getByLabelText('密码');
    expect(password).toHaveAttribute('type', 'password');
    await user.click(screen.getByRole('button', { name: '显示 密码' }));
    expect(password).toHaveAttribute('type', 'text');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.form.required);
    await user.type(screen.getByLabelText('名称'), 'Fake');
    await user.type(screen.getByLabelText('账号'), 'u');
    await user.type(screen.getByLabelText('网址'), 'not a url');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.urlInvalid);
    await user.clear(screen.getByLabelText('网址'));
    await user.click(screen.getByRole('button', { name: /高级设置/ }));
    await user.type(screen.getByLabelText('两步验证密钥（Base32）'), 'not base32!');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.totpInvalid);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('generates a random password of the chosen length with or without symbols', async () => {
    const { user } = setup(LoginForm, base);
    const password = screen.getByLabelText('密码') as HTMLInputElement;
    const spy = vi.spyOn(crypto, 'getRandomValues');
    await user.click(screen.getByRole('button', { name: '生成密码' }));
    expect(spy).toHaveBeenCalled();
    spy.mockRestore();
    expect(password.value).toHaveLength(20);
    expect(password).toHaveAttribute('type', 'text');
    const slider = screen.getByRole('slider');
    expect(slider).toHaveAttribute('min', '12');
    expect(slider).toHaveAttribute('max', '64');
    await user.click(screen.getByRole('checkbox', { name: '包含符号' }));
    // Setting a range value through the DOM avoids dragging in the test.
    const { fireEvent } = await import('@testing-library/react');
    fireEvent.change(slider, { target: { value: '40' } });
    await user.click(screen.getByRole('button', { name: '生成密码' }));
    expect(password.value).toHaveLength(40);
    expect(password.value).toMatch(/^[A-Za-z0-9]+$/);
  });

  it('needs a valid variable prefix for dev use and sends it with the flag', async () => {
    const { onSave, user } = setup(LoginForm, base);
    await user.type(screen.getByLabelText('名称'), 'Fake');
    await user.type(screen.getByLabelText('账号'), 'fake-user');
    await user.click(screen.getByRole('checkbox', { name: '用于开发测试' }));
    expect(screen.getByText(/AI 只能看到名称和变量名/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.devPrefixInvalid);
    await user.type(screen.getByLabelText('环境变量前缀'), 'douyin_test');
    expect(screen.getByLabelText('环境变量前缀')).toHaveValue('DOUYIN_TEST');
    expect(screen.getByText(/DOUYIN_TEST_USERNAME 和 DOUYIN_TEST_PASSWORD/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ devUse: true, envKey: 'DOUYIN_TEST' })));
  });

  it('pre-fills only non-secret fields when editing and leaves empty secrets out', async () => {
    const { onSave, user } = setup(LoginForm, { ...base, initial: stored({ valueType: 'login', name: 'Fake site', tags: ['work'] }), initialFields: { website: 'https://a.example.com', username: 'fake-user', notes: 'fake note' } });
    const form = screen.getByRole('dialog', { name: '编辑账号密码' });
    expect(within(form).getByLabelText('名称')).toHaveValue('Fake site');
    expect(within(form).getByLabelText('网址')).toHaveValue('https://a.example.com');
    expect(within(form).getByLabelText('账号')).toHaveValue('fake-user');
    expect(within(form).getByLabelText('密码')).toHaveValue('');
    expect(within(form).getByLabelText('密码')).toHaveAttribute('placeholder', '留空则保持不变');
    expect(form.textContent).not.toContain(FAKE_PASSWORD);
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const payload = onSave.mock.calls[0][0];
    expect(payload.fields).toEqual({ username: 'fake-user', website: 'https://a.example.com', notes: 'fake note' });
    expect(payload.fields).not.toHaveProperty('password');
    expect(payload.clear).toEqual([]);
    expect(payload.tags).toEqual(['work']);
  });

  it('turns the clear button into a clear list entry and blocks typing meanwhile', async () => {
    const { onSave, user } = setup(LoginForm, { ...base, initial: stored({ valueType: 'login' }), initialFields: { username: 'fake-user' } });
    await user.click(screen.getByRole('button', { name: '清空 密码' }));
    expect(screen.getByLabelText('密码')).toBeDisabled();
    expect(screen.getByText('保存后将清空此项')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /高级设置/ }));
    await user.click(screen.getByRole('button', { name: '清空 两步验证密钥（Base32）' }));
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave.mock.calls[0][0].clear).toEqual(['password', 'totp']);
    expect(onSave.mock.calls[0][0].fields).not.toHaveProperty('password');
  });

  it('shows no clear button while adding', () => {
    setup(LoginForm, base);
    expect(screen.queryByRole('button', { name: /清空/ })).not.toBeInTheDocument();
  });
});

describe('bank card form', () => {
  it('groups the number by four, warns softly on a bad checksum and still saves', async () => {
    const { onSave, user } = setup(BankCardForm, base);
    const form = screen.getByRole('dialog', { name: '添加银行卡' });
    for (const label of ['名称', '银行', '卡号', '持卡人', '有效期', 'CVV', '密码', '开户行', '备注']) expect(within(form).getByLabelText(label)).toBeInTheDocument();
    await user.type(within(form).getByLabelText('名称'), 'Fake card');
    await user.type(within(form).getByLabelText('银行'), 'Fake Bank');
    const number = within(form).getByLabelText('卡号');
    await user.type(number, '4111111111111111');
    expect(number).toHaveValue(FAKE_CARD);
    expect(within(form).queryByText(t.items.cardChecksum)).not.toBeInTheDocument();
    await user.clear(number);
    await user.type(number, '4111111111111112');
    expect(number).toHaveValue('4111 1111 1111 1112');
    const warning = within(form).getByText(t.items.cardChecksum);
    expect(warning).toHaveClass('luhn-warning');
    await user.type(within(form).getByLabelText('有效期'), '0930');
    expect(within(form).getByLabelText('有效期')).toHaveValue('09/30');
    await user.type(within(form).getByLabelText('CVV'), '123');
    await user.type(within(form).getByLabelText('密码'), '654321');
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave.mock.calls[0][0]).toEqual({
      itemType: 'bank_card',
      name: 'Fake card',
      fields: { bank: 'Fake Bank', card_number: '4111111111111112', expiry: '09/30', cvv: '123', pin: '654321' },
      clear: [],
      tags: [],
      favorite: false,
    });
  });

  it('rejects malformed numbers, CVV and PIN with specific messages', async () => {
    const { onSave, user } = setup(BankCardForm, base);
    await user.type(screen.getByLabelText('名称'), 'Fake');
    await user.type(screen.getByLabelText('银行'), 'Fake Bank');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.cardInvalid);
    await user.type(screen.getByLabelText('卡号'), '4111111111111111');
    await user.type(screen.getByLabelText('CVV'), '12');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.cvvInvalid);
    await user.clear(screen.getByLabelText('CVV'));
    await user.type(screen.getByLabelText('密码'), '123');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.pinInvalid);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('edits without the secret fields and offers clear for the optional ones only', async () => {
    const { onSave, user } = setup(BankCardForm, { ...base, initial: stored({ valueType: 'bank_card', name: 'Fake card' }), initialFields: { bank: 'Fake Bank', holder: 'FAKE HOLDER', expiry: '09/30' } });
    const form = screen.getByRole('dialog', { name: '编辑银行卡' });
    expect(within(form).getByLabelText('卡号')).toHaveValue('');
    expect(within(form).getByLabelText('持卡人')).toHaveValue('FAKE HOLDER');
    expect(within(form).getByRole('button', { name: '清空 CVV' })).toBeInTheDocument();
    expect(within(form).getByRole('button', { name: '清空 密码' })).toBeInTheDocument();
    expect(within(form).queryByRole('button', { name: '清空 卡号' })).not.toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: '清空 CVV' }));
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const payload = onSave.mock.calls[0][0];
    expect(payload.clear).toEqual(['cvv']);
    expect(payload.fields).not.toHaveProperty('card_number');
    expect(payload.fields.bank).toBe('Fake Bank');
  });
});

describe('note form', () => {
  it('asks for a title and text and saves the body as a field', async () => {
    const { onSave, user } = setup(NoteForm, base);
    const form = screen.getByRole('dialog', { name: '添加安全笔记' });
    expect(within(form).getByLabelText('标题')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    expect(await within(form).findByRole('alert')).toHaveTextContent(t.form.required);
    await user.type(within(form).getByLabelText('标题'), 'Fake note');
    await user.type(within(form).getByLabelText('正文'), 'line one{Enter}line two');
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave.mock.calls[0][0]).toEqual({ itemType: 'note', name: 'Fake note', fields: { body: 'line one\nline two' }, clear: [], tags: [], favorite: false });
  });

  it('shows the body only because it was handed over for editing', () => {
    setup(NoteForm, { ...base, initial: stored({ valueType: 'note', name: 'Fake note' }), initialFields: { body: 'fake body text' } });
    expect(screen.getByLabelText('正文')).toHaveValue('fake body text');
  });
});

describe('dev secret form', () => {
  it('saves a raw value with the dev_secret type and an uppercase variable name', async () => {
    const { onSave, user } = setup(DevSecretForm, base);
    const form = screen.getByRole('dialog', { name: '添加开发密钥' });
    await user.type(within(form).getByLabelText('名称'), 'Fake token');
    await user.type(within(form).getByLabelText('值'), 'fake-token-value');
    await user.type(within(form).getByLabelText('环境变量名'), 'github_token');
    await user.type(within(form).getByLabelText('用途说明'), 'CI');
    await user.click(within(form).getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ name: 'Fake token', value: 'fake-token-value', value_type: 'dev_secret', env_key: 'GITHUB_TOKEN', description: 'CI', provider_id: 'generic' }));
  });

  it('validates the variable name and keeps the stored value on edit', async () => {
    const { onSave, user } = setup(DevSecretForm, { ...base, initial: stored({ valueType: 'dev_secret', name: 'Fake token', envKey: 'FAKE_TOKEN', provider: 'generic' }) });
    expect(screen.getByLabelText('值')).toHaveValue('');
    expect(screen.getByLabelText('值')).toHaveAttribute('placeholder', '留空则保持不变');
    await user.clear(screen.getByLabelText('环境变量名'));
    await user.type(screen.getByLabelText('环境变量名'), '1BAD');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(t.items.envInvalid);
    await user.clear(screen.getByLabelText('环境变量名'));
    await user.type(screen.getByLabelText('环境变量名'), 'GOOD_NAME');
    await user.click(screen.getByRole('button', { name: '保存凭据' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ value: '', env_key: 'GOOD_NAME', value_type: 'dev_secret' })));
  });
});

describe('English text', () => {
  it('has English labels for the typed forms', () => {
    const en = getTranslation('en-US');
    render(<LoginForm t={en} knownTags={[]} initial={null} onClose={() => undefined} onSave={async () => undefined} />);
    const form = screen.getByRole('dialog', { name: 'Add Login' });
    for (const label of ['Website', 'Account', 'Password']) expect(within(form).getByLabelText(label)).toBeInTheDocument();
    expect(within(form).getByRole('button', { name: 'Generate password' })).toBeInTheDocument();
  });
});
