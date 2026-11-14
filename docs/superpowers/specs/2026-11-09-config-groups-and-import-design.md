# 配置组、导出改进与导入助手设计

> 状态：已确认（2026-11-09）。用户确认：服务器等信息存为“配置组”；普通变量的值默认对 AI 可见（每组可关）；导入走“告诉路径、本机读取、去重后由用户确认”。
> 前置：按类型保存的第一批（login / bank_card / note / dev_secret，提交 b11a82a–9ab070f）已完成。

## 0. 不变量

1. 机密值（密码、私钥、Token、API Key）不进 AI 上下文、不进日志/审计/错误信息；MCP 对数据库只读；所有写入经用户在 Desktop 确认（AI 发起的）或由用户亲自操作。
2. **普通变量的值以明文存在本机数据库**（MCP 没有密钥，这是 AI 能读到它们的前提），所以只有不敏感的值才标为“普通”。
3. 导入时 AI 只传文件/文件夹路径，**不读文件内容**；解析、去重全部在 Desktop 的 Rust 进程里完成。
4. 开发与测试只用假数据。**任何时候都不要读取 `F:\env` 或其他真实资料文件**；需要夹具就按 §5 的结构自造假数据。

## 1. 配置组（`value_type = 'env_group'`）

### 1.1 存储

- `secrets` 行：`name` = 组名；`category` = `server` / `database` / `storage` / `other`（由模板决定，可改）；`subtitle` = “N 个变量”；`env_key` 置空。
- 新列 `secrets.ai_values INTEGER NOT NULL DEFAULT 1`：是否允许 AI 读取普通变量的值。
- 新表：

  ```sql
  CREATE TABLE IF NOT EXISTS group_variables (
    secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    name TEXT NOT NULL,
    secret INTEGER NOT NULL,
    plain_value TEXT NOT NULL DEFAULT '',   -- 只存普通变量的值；机密变量这里永远为空
    PRIMARY KEY (secret_id, name)
  );
  ```

- 加密 payload：`{"v":1,"secrets":{"NAME":"value",...},"notes":"..."}`，只含机密变量的值和备注。
- 校验（Rust）：
  - 变量名须匹配 `^[A-Za-z_][A-Za-z0-9_]*$`，组内唯一；每组最多 200 个变量；单个值 ≤ 64 KB（私钥是多行）；备注 ≤ 100 000 字符。
  - 未知字段拒绝。
- **机密自动判定**：作为新建和导入时的默认值，用户可改。纯函数实现，大小写不敏感：
  - 判为普通：含 `PUBLIC`，或以 `_ID`、`_KEY_ID` 结尾。这条优先。
  - 判为机密：含 `PASSWORD`、`PASSWD`、`PWD`、`SECRET`、`TOKEN`、`PRIVATE`、`CREDENTIAL`，或以 `_SK`、`_KEY`、`API_KEY`、`ACCESS_KEY` 结尾。
  - 其余为普通。

### 1.2 Tauri 命令

| 命令 | 说明 |
|---|---|
| `group_create({ name, category, variables: [{ name, value, secret }], notes, tags, favorite, aiValues })` | 返回 id |
| `group_update(id, { name, category, variables: [{ name, secret, value?, from? }], notes?, tags, favorite, aiValues })` | `variables` 是完整的目标列表。机密变量省略 `value` 表示保留原值，按 `from ?? name` 取原值，用于支持改名。普通变量必须给出 `value`（可为空字符串）。列表里没有的变量即删除。`notes` 省略表示不变 |
| `group_public(id)` | 返回 `{ variables: [{ name, secret, value? }], hasNotes, aiValues }`，只含普通变量的值 |
| `group_var_reveal(id, name)` | 系统原生对话框显示，与 `item_field_reveal` 相同 |
| `group_var_copy(id, name)` | Rust 写剪贴板，沿用现有的定时自动清空 |
| `group_notes(id)` | 返回备注文本，只在用户点“显示备注”或“编辑”时调用 |
| `group_export_key_file(id, name, path)` | 把一个机密变量（私钥）写成文件。路径由用户在保存对话框中选择；用 `create_new`，不覆盖已有文件 |

所有命令先调用 `touch_activity`；审计只记录操作，不记录值。

### 1.3 AI 可见性与计划

- `env_group` 属于开发类型：`SecretMetadata::ai_visible`（crates/storage）和 MCP 的 `aiVisibleCondition`（apps/mcp/src/visibility.ts）都要加入它。
- MCP catalog / get_metadata 对配置组返回 `variables: [{ name, secret, value? }]`。`value` 仅在 `secret = false` 且 `ai_values = 1` 时返回；机密值任何情况下都不返回。
- 计划：
  - v1 的 `secret_ids` 写入整组全部变量；
  - v2 新增配置组专用绑定 `{ secret_id, variables?: [names] }`，不带 capability 或 model；`variables` 省略即全部。
  - MCP 预检：变量名必须存在于该组；与同一计划中其他条目的变量名冲突时报错。
  - Desktop 侧展开和预览同样支持。
- `skills/secrethub-project-env/SKILL.md` 增加一段：
  - 服务器、数据库等配置组用变量子集绑定；
  - 可以读取普通变量的值来编写部署配置；
  - 机密变量只写变量名，不内联值。

### 1.4 界面

- 添加类型选择页新增卡片“配置组（服务器、数据库、云服务）”。
- 表单：
  - 组名、模板（服务器 / 数据库 / 对象存储 / 自定义）、变量表。变量表每行有：名、值、机密开关、删除；可新增行。机密值用 password 输入框并可切换显示；名字含 `PRIVATE_KEY` 的行用多行文本框。
  - 备注（多行）、“允许 AI 读取普通变量值”（默认开）、标签、收藏。
  - 编辑时：普通值预填；机密值留空表示不变，每行有“清空”。
  - 模板变量（🔒 表示机密）：
    - 服务器：SSH_HOST、SSH_PORT（默认 22）、SSH_USER、SSH_PASSWORD🔒、SSH_PRIVATE_KEY🔒、BT_PANEL_URL、BT_PANEL_USERNAME、BT_PANEL_PASSWORD🔒、BT_PANEL_API_TOKEN🔒
    - 数据库：DB_HOST、DB_PORT、DB_USER、DB_PASSWORD🔒、DB_NAME
    - 对象存储：OSS_ENDPOINT、OSS_REGION、OSS_BUCKET、OSS_ACCESS_KEY_ID、OSS_ACCESS_KEY_SECRET🔒
  - 模板提供“变量名前缀”输入。例如填 `YECAOYUN`，则 `SSH_HOST` 变为 `YECAOYUN_SSH_HOST`；每个名字仍可单独修改。
- 详情：
  - 变量表：普通值直接显示并可复制；机密值显示圆点，带“显示”（原生对话框）和“复制”；私钥行有“导出为文件”。
  - “显示备注”；AI 可见性提示，如“AI 可以看到：所有变量名和 N 个普通值”；编辑和删除。
  - 不显示检测、模型同步等 API Key 专用功能。
- 列表显示类型图标、组名和副标题；左侧导航新增“配置组”筛选。

## 2. 导出改进（手动导出与方案）

- 可勾选的类型：API Key、开发密钥、配置组、勾选了“用于开发测试”的账号密码。个人类型仍不可勾选。
- 导出弹窗：
  1. 列出已选条目；配置组可展开，按变量勾选，默认全选。
  2. 目标文件夹：输入框、“选择文件夹”按钮，以及“最近项目”快捷选择（来自 projects 表，最多 8 个）。
  3. 预览表列出：变量名、来源条目、值（机密打码，普通值明文）、状态（新增 / 覆盖已有 / 与另一条目同名冲突）。
  4. 与目标 `.env` 中同名的变量，逐个选择覆盖（默认）或跳过。所选条目之间有同名变量时标红，必须取消其中一个才能导出。
  5. 写入：合并写入（`merge_env` / `merge_example`），确保 `.gitignore` 包含 `.env`，记录项目（`project_record`），写审计。
  6. “存为方案”：把当前选择（包括组内的变量子集）保存为 Profile。
- 后端：
  - 新增 `export_preview_v2` / `export_apply_v2({ directory, selections: [{ secretId, variables? }], skipKeys })`，旧的导出命令保持兼容。
  - `selected_entries` 支持配置组：按变量子集展开，机密值只在写入时才解密。
- 方案：
  - `profile_secrets` 加列 `variables_json TEXT NOT NULL DEFAULT ''`，空表示全部变量；
  - Profile 详情显示变量子集，应用方案时带上子集；
  - MCP `list_profiles` 可在 secretIds 之外附带变量子集。

## 3. 导入助手

### 3.1 入口

- **桌面端**：导入弹窗在现有的“选择文件”和“粘贴文本”之外，增加“选择文件夹”（rfd pick_folder）。
- **AI**：
  - MCP `secrethub_request_import({ paths: [文件或文件夹的绝对路径] })` 写入队列文件 `mcp-pending/import-<id>.json`；
  - Desktop 显示“AI 请求导入：<路径列表>”，用户点“查看”进入与手动导入相同的审核弹窗；
  - `secrethub_get_import_status({ request_id })` 返回 `pending | done | declined | failed`；`done` 时附带新增 / 更新 / 跳过的数量和条目名称，不含任何值；
  - 与 plan / caps / sync / capture / media 各队列的 id 前缀互不识别。
- **Skill** `skills/secrethub-import/SKILL.md`：
  - 只用目录和文件名定位，**绝不读取文件内容**（不得 Read 或 cat .env、私钥、csv 等文件）；
  - 把路径交给 `secrethub_request_import`；
  - 提醒用户到 Desktop 审核；
  - 轮询状态，最后汇报数量和名称；
  - 附 evals 3 条。

### 3.2 本机解析（Rust，以纯函数为主）

- **扫描范围**：文件夹深度 ≤ 3，最多 200 个文件，单文件 ≤ 1 MB；跳过二进制文件（含 NUL 字节）和空文件。
- **分组规则**：每个直接包含文件的文件夹生成一个候选配置组，组名为文件夹名。单独导入的文件，组名取文件名（不含扩展名）；如果是 `.env`，取所在文件夹名。
- **`.env` 和 `.env.*`**：先用 dotenvy 解析；失败时退回逐行解析 KEY=VALUE（支持 `export` 前缀、引号和多行引号值）。其余非空、非注释行汇入该组备注，并加上“来源文件：<文件名>”标题。
- **`.json`**：扁平对象转为变量；否则整个文件作为备注。
- **私钥文件**（内容以 `-----BEGIN` 开头且含 `PRIVATE KEY`，或文件名为 `id_rsa`、`id_ed25519`、`*.pem` 且不是 `.pub`）：
  - 组内已有内容相同的变量 → 跳过；
  - 否则加为 `SSH_PRIVATE_KEY`；已有不同值时加为 `SSH_PRIVATE_KEY_2`（依次递增），并标记冲突。
  - `.pub` 文件加为 `SSH_PUBLIC_KEY`（普通变量）。
- **`.txt`、`.csv`、`.log`、`.md`**：非空时加入备注（带文件名标题）。符合 §3.4 的浏览器密码 CSV 除外。
- **同一文件内的同名变量**：审核时标出；默认保留最后一个（与 dotenv 一致），可改选。
- **服务商 Key 拆分**：变量名等于某服务商的 `envKeys[0]` 时，生成候选 API Key；同组中该服务商的 BASE_URL / MODEL 变量一起带走。审核时每条可选“拆为 API Key（默认）”或“留在组内”。
- **机密判定**：按 §1.1 的规则。

### 3.3 去重

所有比较都在 Rust 内存中完成，结果展示在审核弹窗里。

| 类型 | 判定 | 处理 |
|---|---|---|
| 配置组 | 已存在同名组时逐变量比较 | 全部相同 → 跳过；有差异 → 选“合并更新（默认）/ 另存为新组 / 跳过”，并列出新增、变更、缺失的变量名 |
| API Key / 开发密钥 | 值相同 | 跳过 |
| API Key / 开发密钥 | 同服务商、同变量名，但值不同 | 选“覆盖 / 保留两份（默认）/ 跳过” |
| 账号密码 | 网站主机 + 账号相同 | 密码相同 → 跳过；不同 → 选“更新 / 保留两份 / 跳过” |
| 银行卡 | 卡号相同 | 跳过或更新 |

**审核弹窗**：
- 按来源文件夹或文件分组，列出将要创建的条目：类型、名称、变量名、机密标记；每条可取消勾选。
- 底部汇总“新增 N / 更新 M / 跳过 K”，确认后写入。
- 只显示名称，不显示任何值。

### 3.4 其他来源（走同一审核流程）

- **浏览器或密码管理器导出的 CSV**：按表头识别以下格式，导入为账号密码条目（有 totp 字段时一并导入）：
  - Chrome / Edge：`name,url,username,password`
  - Firefox：`url,username,password,...`
  - Bitwarden：`folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp`
- **银行卡模板**：导入弹窗提供“下载银行卡模板”，CSV 表头为 `名称,银行,卡号,持卡人,有效期,开户行,备注`。
  - 模板**不含 CVV 和密码列**，并提示这两项导入后在详情里手动补；
  - 导入为银行卡条目。
- **智能粘贴**：增强导入弹窗的“粘贴文本”模式。
  - 本机按规则识别：KEY=VALUE；中英文“账号/用户名/邮箱 + 密码”类的行；网址；通过 Luhn 校验的 12–19 位卡号；
  - 识别结果生成候选条目进入审核；无法识别的整段作为安全笔记候选；
  - 纯本机处理，不经过 AI。
- **导入后**：新建的 API Key 自动检测（沿用现有逻辑）。

## 4. 实施切片

每个切片单独提交；提交前跑完整验证清单。

| 切片 | 内容 |
|---|---|
| G1 | 配置组的存储、命令、机密判定、AI 可见性（Rust + MCP）、计划 v2 变量子集、project-env skill 的补充 |
| G2 | 配置组的表单、详情、列表、导航 |
| G3 | 导出弹窗改造，方案支持变量子集 |
| I1 | 导入引擎（文件夹、.env、json、私钥、文本）、去重、审核弹窗、MCP `request_import` / `get_import_status`、`secrethub-import` skill |
| I2 | 浏览器密码 CSV、银行卡模板、智能粘贴 |

## 5. 测试要点

- **G1**：
  - 存储往返：普通值为明文，机密值加密；
  - update 保留机密值，改名时用 `from` 带过去；
  - 机密判定用表驱动测试；
  - `ai_values` 关闭时 MCP 不返回普通值；MCP 任何情况下都不返回机密值；
  - 计划 v1 写整组，v2 写子集，同名冲突报错。
- **G2 / G3**：
  - 模板与前缀；机密值不出现在 DOM 中；详情按钮调用正确命令；
  - 导出预览的三种状态（新增 / 覆盖 / 内部冲突）、跳过；
  - 方案保存变量子集。
- **I1**：用假数据复刻下面的目录结构作为夹具。不要读取真实目录。
  - `服务器A/.env`：含重复的 `SSH_PRIVATE_KEY`、一个 `DASHSCOPE_API_KEY`、若干非 KV 行；
  - `服务器A/id_ed25519`：假的私钥块；
  - `服务器A/notes.txt`；
  - `服务器A/users.csv`：普通 CSV，不是密码表头；
  - `服务器A/empty.json`：空文件；
  - `服务器B/.env`：全是自由文字；
  - `服务器B/key_ed25519`：假私钥；
  - `充值/.env`：一行文字。

  需要断言：生成的候选组、拆出的 API Key、备注内容、跳过的空文件、重复变量的标记；去重表的每个分支；MCP 的 request / status；skill evals 的文案要求不读文件内容。
- **I2**：三种 CSV 表头、银行卡模板往返、智能粘贴的各条识别规则（含 Luhn 校验）。
