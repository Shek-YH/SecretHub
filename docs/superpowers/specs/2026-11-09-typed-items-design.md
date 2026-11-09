# 按类型区分的保存项（第一批）设计

> 状态：已确认（2026-11-09）。用户确认按类型设计表单、个人信息与 AI 隔离、增加修改主密码，分两批实施；本文为第一批。
> 第二批（不在本文实现范围）：证件（identity）、文档附件（document，加密文件，计划用“每文件随机密钥 + 保险库密钥包裹”的信封加密，改主密码时只需重新包裹）、两步验证动态码显示。

## 1. 问题

- 每条记录只有一个加密值（`secret_payloads` 一行一个字符串），所以所有类型的表单都一样，银行卡、邮箱账号等多字段信息无处可填。
- MCP 会把**所有**记录的名称等元数据返回给 AI，个人信息（如“招行工资卡”）也会被 AI 看到。
- 没有修改主密码的功能，而保存银行卡后，主密码是唯一保护。

## 2. 类型（存于现有 `secrets.value_type` 列）

| value_type | 名称 | 加密内容（payload） | AI 可见 / 可写 .env |
|---|---|---|---|
| `api_key` | API Key（开发） | 原样字符串（不变） | 是 |
| `dev_secret` | 开发密钥（Token、数据库连接串、SSH 私钥等） | 原样字符串 | 是 |
| `login` | 账号密码（网站、邮箱、App） | JSON 字段 | 否；勾选“用于开发测试”时例外（§6） |
| `bank_card` | 银行卡 | JSON 字段 | 否 |
| `note` | 安全笔记 | JSON 字段 | 否 |

旧值 `token`、`url`、`password`、`text` 由 §7 的迁移转换。api_key、dev_secret 保持“原样字符串”，现有导出、复制、MCP 计划等开发链路完全不变。

## 3. 字段定义（单一来源）

新增 `packages/item-types/item-types.json`，Rust 用 `include_str!` 读取，前端 import。每个类型列出字段：

```jsonc
{ "key": "card_number", "kind": "card_number", "sensitive": true, "required": true, "maxLength": 32,
  "label": { "zh-CN": "卡号", "en": "Card number" } }
```

`kind` 取值：`text`、`url`、`email_or_text`、`secret`、`multiline`、`card_number`、`expiry`（MM/YY）、`digits`、`totp_secret`。

| 类型 | 字段（* 为必填；🔒 为机密字段） |
|---|---|
| login | website（url）、username*（账号：邮箱/手机号/用户名）、password 🔒、totp 🔒（两步验证密钥，Base32，可选）、notes（备注，多行） |
| bank_card | bank*（银行）、card_number* 🔒、holder（持卡人）、expiry（MM/YY）、cvv 🔒（3–4 位数字）、pin 🔒（4–6 位数字）、branch（开户行）、notes |
| note | body* 🔒（正文，多行，≤100 000 字符） |
| dev_secret | 沿用现有元数据：value* 🔒（多行）、env_key（可选，校验格式）、description（用途说明） |

- 名称（`secrets.name`）、标签、收藏沿用现有列，所有类型通用。
- login、bank_card、note 的**所有字段（包括备注）都存进加密 JSON**，即 `{"v":1,"fields":{...}}`，不进明文列。
- 校验在 Rust 端做（前端只做即时提示）：未知字段拒绝；按 kind 校验格式和长度；卡号去掉空格后为 12–19 位数字，Luhn 校验不通过只提示、不阻止；有效期格式 MM/YY；CVV 3–4 位、PIN 4–6 位数字；TOTP 为合法 Base32。

## 4. 明文副标题（列表显示用）

新增列 `secrets.subtitle TEXT NOT NULL DEFAULT ''`，保存时由 Rust 计算，只含低敏感信息：

- login：网址的主机名（例如 `accounts.google.com`），**不放账号**；
- bank_card：`{银行} · 尾号 {卡号后 4 位}`；
- note：空；
- dev_secret：env_key。

副标题只存在本机数据库，永远不返回给 AI（§6）。

## 5. 后端命令与显示规则

- `item_create({ itemType, name, fields, tags, favorite, devUse?, envKey? })` → id。只接受 login / bank_card / note；dev_secret 走现有 secret_create（value_type 为 `dev_secret`）。
- `item_update(id, { name, fields, clear, tags, favorite, devUse?, envKey? })`：`fields` 中给出的键覆盖；**未给出的机密字段保持不变**；`clear` 列出要清空的可选字段。
- `item_public_fields(id)` → 只返回非机密字段（网址、账号、银行、持卡人、有效期、开户行、备注），用于详情页和编辑表单预填。
- `item_field_reveal(id, field)` → 与现有 secret_reveal 一样，用**系统原生对话框**显示单个机密字段，明文不进网页界面。
- `item_field_copy(id, field)` → Rust 写剪贴板，沿用现有的定时自动清空。
- `item_note_body(id)` → 例外：笔记正文是长文本，原生对话框不便阅读。只在用户明确点“显示正文”或“编辑”时返回给界面；切换记录、锁定（自动锁定事件已会关闭表单）时清除。
- 所有命令先 `touch_activity`，审计只记录操作和类型，不记任何值。

## 6. AI 隔离

“AI 可见”的条件：

- `value_type IN ('api_key','dev_secret','token')`；
- 或 `value_type IN ('url','password','text') AND env_key <> ''`（兼容迁移前的旧数据）；
- 或 `value_type = 'login' AND dev_use = 1`。

不满足的记录：

- **MCP 全部工具都看不到**：catalog / search / get_metadata（返回 found:false）/ recommend / find_models / list_profiles 的 secretIds / prepare 预检，名称也不返回。实现为一个共用的 SQL 条件，各查询都套用。
- Desktop 计划展开和预览、手动导出、“复制全部内容”、Profile 选择器都排除，命中时报 `secret_unavailable`。
- 列表的导出勾选框不对它们显示，“全选”也不包含它们。

**login 的“用于开发测试”**：新增列 `secrets.dev_use INTEGER NOT NULL DEFAULT 0`。勾选时必须填环境变量前缀（存 env_key，例如 `DOUYIN_TEST`）。导出或计划写入时生成 `{前缀}_USERNAME` 和 `{前缀}_PASSWORD` 两行。AI 只能看到名称和这两个变量名，看不到值。

## 7. 旧数据迁移（解锁后执行，幂等，单事务）

`VaultService::migrate_legacy_items()` 在 unlock、setup_master、backup_restore 成功后调用：

| 旧类型 | 条件 | 新类型 | 加密内容 |
|---|---|---|---|
| token | — | dev_secret | 不变 |
| url / password / text | env_key 非空 | dev_secret | 不变 |
| password | env_key 为空 | login | `{password: 原值}` |
| text | env_key 为空 | note | `{body: 原值}` |
| url | env_key 为空 | login | `{website: 原值}` |

迁移时同时计算 subtitle。读取时若 login / bank_card / note 的加密内容不是带 `v:1` 标记的 JSON，按上表兜底解释，防止中途失败导致读不出。

## 8. 修改主密码

- `VaultService::change_master_password(current, new)`：
  1. 要求已解锁，并用当前 salt 和校验值验证旧密码；
  2. 新密码长度规则与创建保险库相同（≥ 9）；
  3. 生成新 salt，派生新密钥；
  4. **在一个事务内**逐条用旧密钥解密、新密钥加密全部 `secret_payloads`，并更新 `vault_meta` 的 salt 和校验值；
  5. 提交后替换内存中的密钥，checkpoint，写审计 `master_password_changed`。
- 任何一步失败则整体回滚，旧密码继续有效。
- 界面（设置 → 安全 → 修改主密码）：当前密码、新密码、确认新密码。新密码不足 12 位时提示"建议 12 位以上"。成功后提示："以前导出的备份仍需用旧密码恢复，建议现在导出一份新备份"。
- Tauri 命令 `vault_change_password(current, new)`。

## 9. 界面

- **添加**：先显示"你要保存什么？"类型选择页，含 5 张卡片：API Key（开发 · 大模型）、开发密钥、账号密码、银行卡、安全笔记，每张带一句说明。
  - API Key 卡片进入现有的一屏快速表单；
  - 服务商页点"配置"仍直接进入 API Key 表单。
- **每种类型各有一个表单组件**，放在 `apps/desktop/src/items/`：
  - **账号密码**：网址、账号、密码（显示/隐藏切换、一键生成：默认 20 位，可调 12–64 位，可开关符号，用 `crypto.getRandomValues`）、两步验证密钥（折叠为"高级"）、备注、标签、收藏、"用于开发测试"开关和前缀。
  - **银行卡**：银行、卡号（输入时按 4 位分组显示，Luhn 不通过时黄色提示）、持卡人、有效期 MM/YY、CVV、密码、开户行、备注、标签。
  - **安全笔记**：标题、正文（多行）、标签。
  - **开发密钥**：名称、值（多行）、环境变量名、用途说明、标签。
  - 编辑时：非机密字段预填；机密字段留空表示不修改，每个可选机密字段旁有"清空"按钮。
- **详情页按类型显示字段**：
  - 机密字段显示圆点，带"显示"（原生对话框）和"复制"按钮；非机密字段带"复制"；
  - 账号密码有"打开网站"（只允许 https，复用 open_external_url）；
  - 笔记有"显示正文"；
  - 个人类型不显示检测、模型同步、复制全部内容等开发功能。
- **列表**：每行显示类型图标、名称、副标题。
- **左侧导航**：类型筛选改为 全部 / API Key / 开发密钥 / 账号密码 / 银行卡 / 安全笔记，旧类型值按 §7 的规则计入对应分组。
- **分类**：个人类型的分类自动设为 `personal`。
- 所有新文案中英文齐全，不可信字符串按纯文本渲染。

## 10. 测试要求

- **Rust**：
  - 各类型字段校验（合法与非法输入）、subtitle 计算、加密 JSON 往返；
  - item_update 保留未提供的机密字段、`clear` 生效；
  - 迁移表中每一行都有用例，且重复执行幂等；
  - change_master_password：成功后新密码能解锁、所有记录可读、旧密码失效；验证失败或中途失败时整体回滚；
  - 计划和导出拒绝个人类型；
  - login 开发测试导出生成两行变量。
- **MCP**（vitest，真实 SQLite 夹具）：所有工具都不返回个人类型（包括名称）；login 勾选开发测试后可见；迁移前的旧数据按 §6 条件处理。
- **前端**：
  - 类型选择页；
  - 四种表单的字段和校验提示、密码生成器、卡号分组和 Luhn 提示；
  - 详情页显示/复制按钮调用正确命令，且机密值不出现在 DOM 中；
  - 导航筛选、导出勾选框对个人类型隐藏、修改主密码弹窗。

## 11. 不在本批范围

证件、文档附件、两步验证动态码、浏览器自动填充、密码强度审计。
