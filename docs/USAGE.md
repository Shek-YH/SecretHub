# SecretHub 使用文档

SecretHub 是一个本机加密的凭据与开发资源中心：把 API Key、服务器、数据库、网站账号、银行卡、笔记、测试素材统一存放，并通过 MCP 让 AI 在**看不到机密值**的前提下替你配置项目、导入资料、选择模型。

- 桌面端（Tauri + React + Rust）：保存、查看、导入、导出、审核 AI 的请求。
- MCP 服务（Node）：给 Claude Code 等 AI 客户端用的只读接口和“请求队列”。
- Skills：教 AI 按固定流程使用 MCP 的说明书。

---

## 1. 安装与启动

### 1.1 首次准备（每台电脑一次）

```bash
npm --prefix apps/desktop ci
```

```bash
npm --prefix apps/mcp ci
```

```bash
npm --prefix apps/mcp run build
```

`node_modules` 和 `apps/mcp/dist` 不进 git。换电脑、拉取代码后，或者 MCP 报 `Connection closed` 时，重跑以上三条。

### 1.2 启动桌面端（开发版）

```bash
npm --prefix apps/desktop run tauri dev
```

第一次编译需要一两分钟。首次启动会进入“创建本地保险库”：主密码至少 9 位，建议 12 位以上且不在别处使用。**主密码无法找回**，请务必定期导出备份（见 §2.8）。

### 1.3 数据存放位置

- Windows：`%APPDATA%\com.secrethub.desktop\`
- macOS：`~/Library/Application Support/com.secrethub.desktop/`

MCP 服务按同样的规则找保险库（`apps/mcp/src/data-dir.ts`），可用 `SECRETHUB_DB_PATH` 覆盖。

| 路径 | 内容 |
|---|---|
| `vault.sqlite` | 保险库，机密值加密保存 |
| `mcp-pending\`、`mcp-results\` | AI 请求队列和结果（不含任何值） |
| `media\` | 素材库文件（不加密） |
| `catalog\` | 模型目录缓存 |

---

## 2. 桌面端功能

### 2.1 保存的类型

点“添加”，先选“你要保存什么？”：

| 类型 | 适合 | AI 能否看到 |
|---|---|---|
| API Key | 大模型服务商的 Key（粘贴后按前缀自动识别服务商） | 能看到名称、服务商、模型，看不到 Key |
| 开发密钥 | GitHub Token、数据库连接串等单个值 | 能看到名称和变量名，看不到值 |
| 配置组 | 一台服务器、一个数据库、一个云服务的一组变量 | 能看到变量名和**普通变量的值**（可按组关闭），看不到机密变量 |
| 账号密码 | 网站、邮箱、App 账号 | 看不到；勾选“用于开发测试”后只能看到名称和变量名 |
| 银行卡 | 卡号、有效期、CVV 等 | 看不到 |
| 安全笔记 | 长文本 | 看不到 |

- 机密字段默认只显示圆点。“显示”会弹出系统原生小窗口，“复制”后剪贴板会自动清空。
- 配置组的每个变量都可以单独标为“机密”或“普通”。名字里带 PASSWORD、TOKEN、SECRET、PRIVATE_KEY 等的变量会自动标为机密。

### 2.2 服务商与模型

- 服务商页默认显示 10 家常用服务商，其余收在“更多服务商”里，可以搜索。
- API Key 保存后会自动检测，并同步这个 Key 实际能用的模型清单和每个模型的能力（对话、视觉、生图、语音……）。
- 聚合型 Key（一个 Key 能调多家、多种模态）可以在“多能力模型”表里给每种能力选默认模型和变量名。
- 设置页的“模型目录”可以获取维护者发布的签名目录补丁（需要配置更新地址，见 §6）。

### 2.3 导出到项目 `.env`

1. 在列表里勾选 API Key、开发密钥、配置组；配置组可以只勾选其中几个变量。
2. 点“导出”，选择项目文件夹（也可以直接点“最近项目”）。
3. 在预览里，机密值打码；与项目里已有变量同名的，可以逐个选“覆盖”或“跳过”。
4. 写入方式：合并进已有的 `.env`，同时更新 `.env.example`（只写变量名和中文注释），并确保 `.gitignore` 里有 `.env`。
5. 常用的组合可以点“存为方案”，下次一键套用。

### 2.4 导入

| 方式 | 说明 |
|---|---|
| 选择文件夹或文件 | 本机扫描 `.env`、`.json`、私钥文件、文本，规则解析后弹出审核窗口，由你逐项确认 |
| 浏览器密码 CSV | 支持 Chrome、Edge、Firefox、Bitwarden 导出的格式，转为账号密码 |
| 银行卡模板 | 下载 CSV 模板，填好后导入（不含 CVV 和密码列） |
| 智能粘贴 | 把一段文字粘贴到 SecretHub 里，本机识别 `KEY=VALUE`、账号密码、卡号等 |
| **AI 整理导入** | 让 AI 整理格式混乱的资料，见 §4.2（推荐） |

导入时会去重：值相同的跳过；同名但值不同的，让你选覆盖、保留两份或跳过。

### 2.5 素材库

可以导入图片、视频、音频、文本作为测试素材。程序自动读取尺寸和时长（时长需要本机装有 ffmpeg 的 ffprobe），可以加标签和备注。AI 能按类型、标签、横竖屏、时长查找素材，拿到文件路径后用于上传测试。AI 想往素材库里加文件时，需要你在桌面端确认。

### 2.6 半自动获取 Key

在服务商卡片上点“打开官网并自动捕获 Key”，SecretHub 会在你的浏览器里打开该服务商的 Key 页面。你创建 Key 并点“复制”后，SecretHub 从剪贴板读到它，直接保存、检测并同步模型，然后清空剪贴板。读取剪贴板必须由你点“开始”，最长等待 3 分钟。

### 2.6.1 AI 请求你填写（安全录入表单）

AI 需要密码、API Key、令牌、账号等敏感信息时，不会让你在聊天里粘贴，而是发起一张表单（MCP 工具 `secrethub_request_input`）：

1. SecretHub 窗口自动弹到前面，显示 AI 按场景准备的表单（标题、说明、字段，能查到的已预填好）。保险库锁着时先显示“请先解锁”，解锁后自动弹出。
2. 密码类字段打码；下拉框既可选也可手填。大模型 Key 的表单有“获取模型列表”按钮：用你填的 Key 请求服务商的 `/models`，把模型填进下拉框。
3. 点“验证并保存”：网站账号会先用这组账号调用一次登录接口，大模型 Key 会先拉一次模型列表；表单上会写明值会发往哪个网站。验证失败会显示原因（如 HTTP 401），表单保留，改好再试；点“取消”则 AI 收到 cancelled。
4. 验证通过才写进保险库，并出现“AI 导入”通知，可一键撤销。AI 只能拿到状态和新条目的 id，拿不到你填的值。

和 §2.6 的区别：要在官网**新建** Key 时用剪贴板捕获；已有 Key、需要自定义请求地址或选模型、网站后台账号、数据库等其他凭据，用表单。

### 2.7 AI 请求的确认

AI 发起的写入计划、模型能力建议、加入素材库、捕获 Key 等请求，都会在桌面端显示横幅，由你点“查看”审核。只有 **AI 整理导入**是自动写入的，靠程序规则保证安全（见 §4.2）：每次写入都有通知，可以一键“撤销此次导入”。

### 2.8 安全设置

- **自动锁定**：设置页可选 1、5、15、30、60 分钟；在界面上打字、点击都算操作。
- **修改主密码**：设置 → 修改主密码。改完后，以前导出的备份仍然要用**旧密码**恢复，所以请立即重新导出一份备份。
- **备份**：设置 → 导出备份，文件名带时间，不会覆盖旧备份。恢复入口在设置页和解锁页（“从备份恢复”）。恢复时，当前保险库会被移到 `vault-backup-<时间>` 文件夹，不会删除。

---

## 3. MCP 服务

### 3.1 注册到 Claude Code（所有项目可用）

```bash
claude mcp add --scope user secrethub -- node "F:\CCPJ\SecretHub\apps\mcp\dist\index.js"
```

检查连接状态：

```bash
claude mcp get secrethub
```

在 Claude Code 会话里，可以用 `/mcp` 查看或重新连接。**写入类操作需要 SecretHub 桌面端开着并已解锁**；只读查询直接读本机数据库，不需要解锁。

可选环境变量：

| 变量 | 默认值 | 作用 |
|---|---|---|
| `SECRETHUB_DB_PATH` | `%APPDATA%\com.secrethub.desktop\vault.sqlite` | 指定保险库位置 |
| `SECRETHUB_PLAN_DIR` / `SECRETHUB_RESULT_DIR` | 数据库同目录下的 `mcp-pending` / `mcp-results` | 指定请求队列和结果目录 |
| `SECRETHUB_EFFECTIVE_MODELS_PATH` | 数据库同目录下的 `catalog\effective-models.json` | 指定生效的模型目录文件 |

### 3.2 工具清单

| 类别 | 工具 | 作用 |
|---|---|---|
| 查询 | `secrethub_list_secret_catalog` / `secrethub_search_secrets` / `secrethub_get_secret_metadata` | 列出、搜索、查看条目元数据（配置组含变量名和允许的普通值） |
| 查询 | `secrethub_list_profiles` / `secrethub_list_projects` | 列出方案和最近项目 |
| 查询 | `secrethub_recommend_secrets_for_project` | 按技术栈推荐需要的 Key |
| 查询 | `secrethub_validate_secret` | 查看已保存的检测状态（不联网） |
| 模型 | `secrethub_find_models` | 按能力查找可用模型（如 chat、vision、image_gen、tts），返回 `uncovered` 和 `unsyncedKeys` |
| 模型 | `secrethub_get_invocation` | 某个能力的调用方式：地址、鉴权头模板、请求体骨架，只给变量名 |
| 模型 | `secrethub_suggest_model_capabilities` | 提交模型能力建议，由你在桌面端确认 |
| 模型 | `secrethub_request_model_sync` / `secrethub_get_sync_status` | 请求桌面端重新同步模型清单，查询结果 |
| 写入项目 | `secrethub_prepare_project_env` / `secrethub_apply_profile` | 生成 `.env` 写入计划，由你在桌面端审核 |
| 写入项目 | `secrethub_get_plan_status` | 查询计划结果：pending / written / declined / failed |
| 导入 | `secrethub_import_inspect` / `secrethub_get_import_inspection` | 让桌面端生成资料的打码视图 |
| 导入 | `secrethub_apply_import_plan` | 提交整理方案，按程序规则自动写入 |
| 导入 | `secrethub_request_import` / `secrethub_get_import_status` | 手动审核方式的导入请求；查询导入结果 |
| Key 获取 | `secrethub_start_key_capture` / `secrethub_get_capture_status` | 请求剪贴板捕获，查询结果 |
| 安全录入 | `secrethub_request_input` / `secrethub_get_input_status` | 请桌面端弹出录入表单，验证后保存；查询结果（waiting / saved / cancelled / failed，只含条目 id） |
| 素材 | `secrethub_media_search` / `secrethub_media_get` | 查找素材，拿到文件路径 |
| 素材 | `secrethub_media_add` / `secrethub_get_media_add_status` | 请求把文件加入素材库（需你确认），查询结果 |

### 3.3 MCP 的安全边界

- MCP 对数据库**只读**，没有解密密钥，任何工具都不会返回机密值。
- 个人类型（账号密码、银行卡、笔记）对 AI 完全不可见，连名称也看不到。
- 需要写入或联网的操作，MCP 只把请求放进队列，由桌面端执行。

---

## 4. Skills

### 4.1 安装

把仓库 `skills\` 下的各个目录链接到全局 skill 目录（Windows，PowerShell）：

```powershell
foreach ($n in 'secrethub-project-env','secrethub-import','secrethub-model-refresh','secrethub-key-onboarding','secrethub-catalog-maintainer','secrethub-secure-input') { New-Item -ItemType Junction -Path "$env:USERPROFILE\.claude\skills\$n" -Target "F:\CCPJ\SecretHub\skills\$n" }
```

用的是目录链接，仓库里的 skill 更新后自动生效。skill 依赖 §3 的 MCP 服务。

### 4.2 各 skill 的使用场景

调用方式：直接用自然语言说出需求，Claude 会自动选用对应的 skill；也可以输入 `/<skill 名>` 指定。

#### secrethub-import：AI 整理导入（最常用）
- **场景**：服务器、数据库、云服务、API Key 等资料散落在文件夹里，格式不统一，也可能有错。
- **示例说法**：“把 D:\资料\服务器 整理导入 SecretHub”“导入这个文件夹里的 .env 和密钥文件”。
- **流程**：
  1. AI 只按文件名找到路径，不读文件内容；
  2. 桌面端生成打码视图，密码、密钥都变成 `«S3:…»` 这样的编号；
  3. AI 按“一台服务器或一个服务一组、变量名统一加前缀、每个变量一句说明”整理，并指出疑点；
  4. 桌面端自动写入。
- **规则保证**：
  - 被打码的值一律按机密保存；
  - 只新增，不覆盖、不删除，同名冲突时另存一组；
  - 每次写入都可以一键撤销。
- **你要做的**：保持 SecretHub 打开并解锁；之后看一眼 AI 列出的疑点。资料不对时，项目连接或调用会失败，AI 会请你补充。

#### secrethub-project-env：给项目准备 `.env`
- **场景**：新项目需要大模型、服务器、数据库等配置。
- **示例说法**：“这个项目要对话和生图能力，帮我配好环境”“把阿里云服务器的 SSH 配置和千问 Key 写进这个项目”。
- **流程**：
  1. AI 读项目，列出需要的能力；
  2. 用 `find_models` 选模型、用 `get_invocation` 获取调用方式；
  3. 把选型表给你确认；
  4. 生成写入计划；
  5. 你在桌面端审核后写入。配置组可以只写其中一部分变量。

#### secrethub-model-refresh：更新模型清单
- **场景**：厂商发布了新模型。
- **示例说法**：“更新一下模型”“有没有新模型”。
- **流程**：AI 先列出你已有 Key 的服务商，然后问你选哪种方式：
  - A. 只同步这几家的真实模型清单，并补充新模型的能力说明；
  - B. 全量更新：到 设置 → 模型目录 → 立即检查更新，获取维护者发布的补丁。

#### secrethub-key-onboarding：从官网获取 Key
- **场景**：还没有某个服务商的 Key。
- **示例说法**：“帮我配好 DeepSeek 的 Key”。
- **流程**：
  1. AI 查公开文档里的最新模型和调用方式；
  2. 发起剪贴板捕获，你在桌面端点“开始”；
  3. 在浏览器里创建 Key 并复制，SecretHub 自动保存。

  全程 Key 不经过聊天。

#### secrethub-secure-input：让你安全地填写密码 / Key
- **场景**：AI 需要你提供密码、API Key、令牌、网站或数据库账号。
- **示例说法**：“把后台管理员账号存进 SecretHub”“这个项目要连 MySQL，帮我存一下数据库信息”。
- **流程**：
  1. AI 按场景准备表单并预填能查到的信息（用户名、请求地址、模型等）；
  2. 你在桌面端填写，点“验证并保存”；
  3. 验证通过后保存，AI 用新条目的 id 继续后续步骤。
- **怎么选**：在官网新建大模型 Key → `secrethub-key-onboarding`（剪贴板捕获）；其余敏感信息 → 本 skill 的表单；文件里已有的资料 → `secrethub-import`。

#### secrethub-catalog-maintainer：维护模型目录（维护者用）
- **场景**：你作为维护者，为所有用户发布新的模型目录。
- **流程**：
  1. AI 按公开资料核对各家模型，列出变更表（附来源链接）；
  2. 你确认后，AI 修改 `model-catalog.json` 并运行 `catalog:check`；
  3. 你签名（`catalog:sign`），上传服务器，发版前运行 `catalog:bake`。

### 4.3 所有 skill 共同遵守的规则

- 不要求你在聊天里发送任何密码、Key 或文件内容。
- 不读取含机密的文件，只传路径。
- 汇报只说名称、数量和状态，不涉及任何值。

---

## 5. 常见问题

| 现象 | 原因与处理 |
|---|---|
| MCP 显示 `Connection closed` / `Failed to connect` | 依赖或编译产物缺失：运行 §1.1 的三条命令，再在 `/mcp` 里重新连接 |
| AI 的请求一直是 `pending` | 桌面端没开或未解锁。写入类请求需要解锁 |
| 保存时提示自动锁定 | 闲置超时，重新解锁即可；锁定时间可在设置里调整 |
| 导入的服务器信息不对 | 让 AI 用 secrethub-import 重新整理，或在 SecretHub 里直接修改；不满意可以撤销那次导入 |
| 想让 AI 直接登录服务器 | 在本机 `~/.ssh/config` 里给每台服务器配一个别名（`Host`、`HostName`、`User`、`IdentityFile`），AI 用 `ssh <别名>` 登录，不需要看到私钥 |

---

## 6. 发布前（维护者）

- 运行 `npm run catalog:keygen` 生成正式签名密钥。私钥离线保存，不要放进仓库；替换掉仓库里的开发用公钥。
- 确定模型目录的托管地址，构建时设置 `SECRETHUB_CATALOG_URL=https://…/model-catalog.json`。
- 每次更新目录：`catalog:check` → `catalog:sign` → 上传 → `catalog:bake`。

### 6.1 发布安装包与自动更新（Windows）

安装包是 NSIS `.exe`，按当前用户安装（不需要管理员权限）。MCP 服务已打成单文件 `secrethub-mcp.exe` 随包安装，不需要装 Node；用户在「设置 → 连接 AI 工具」点一次，即可注册到 Claude Code / Claude Desktop / Cursor 并安装 6 个 skill。

**一次性准备**

- 更新签名密钥：`C:\Users\Administrator\.tauri\secrethub-updater.key`（无密码，只在本机，不进仓库；公钥已写进 `tauri.conf.json` 的 `plugins.updater.pubkey`）。**丢了这把私钥，已安装的老版本将永远收不到更新**，请离线备份。
- 更新地址只有一处：`apps/desktop/src-tauri/tauri.conf.json` 的 `plugins.updater.endpoints`，值为 `https://updates.9aizhuan.com/secrethub/latest.json`（腾讯宝塔服务器上的静态站点 `/www/wwwroot/updates.9aizhuan.com`，每个产品一个子目录）。**第一个对外分发的安装包必须已经是真实地址**，否则老版本收不到更新。
- 服务器上准备一个静态目录（HTTPS 可访问），例如 `/var/www/secrethub`，对应 `-BaseUrl https://你的域名/secrethub`。

**每次发布**

1. 改版本号：`tauri.conf.json`、根 `package.json`、`Cargo.toml`（workspace）、`apps/desktop/package.json`、`apps/mcp/package.json` 保持一致。
2. 先演练（构建并签名，生成 `latest.json`，只打印 scp 命令，不上传）：
   `powershell -File scripts\release-windows.ps1 -Notes "本次更新说明" -DryRun`（默认上传到 tx-bt 的 `/www/wwwroot/updates.9aizhuan.com/secrethub`）
3. 确认无误后去掉 `-DryRun` 正式上传（先传安装包，最后传 `latest.json`）。`-SkipBuild` 可复用已有产物，`-Notes` 写更新说明。
4. 产物在 `target/release/bundle/nsis/`：`SecretHub_<版本>_x64-setup.exe`、`.sig`、`latest.json`。

**客户端更新行为**：启动约 3 秒后检查，之后每小时一次；有新版本就静默下载（失败不打扰，下次再试）；下载完顶栏出现「更新」按钮，点击后静默安装并自动重启；一直不点，退出应用时静默安装（不自动重启）。「设置 → 软件更新 → 检查更新」是手动入口，会提示结果。安装前安装程序会先结束正在运行的 `secrethub-mcp.exe`（Claude 等客户端下次调用会自动重新拉起），避免文件被占用导致升级失败。

**验证升级必须用真实旧版本**：更新模式由已安装的旧版本决定，所以要连发两个版本，先装旧版，再发新版到服务器，在旧版里实测一次升级。

### 6.2 发布安装包与自动更新（macOS）

产物是 `.dmg`（首次安装）和 `.app.tar.gz`（自动更新包）。Mac 专用配置在 `apps/desktop/src-tauri/tauri.macos.conf.json`（只在 Mac 上合并生效）：打 `app` + `dmg`、ad-hoc 签名、更新地址为单独的 `latest-macos.json`，所以 Mac 和 Windows 可以各自发版。

- 更新签名私钥与 Windows **是同一个文件**，放到 Mac 的 `~/.tauri/secrethub-updater.key`。
- MCP sidecar：`npm --prefix apps/mcp run build:sea` 在 Mac 上会自动下载 nodejs.org 官方 Node（版本见 `apps/mcp/scripts/sea-platform.mjs`，校验 SHA-256，缓存在 `apps/mcp/build/node-dist/`）；Homebrew 的 node 依赖系统外的动态库，不能打进包里。
- 每次发布：`bash scripts/release-macos.sh --notes "本次更新说明" --dry-run`，确认后去掉 `--dry-run`。上传顺序：更新包 → dmg → `latest-macos.json`，并把 `SecretHub-latest-<架构>.dmg` 链接指向新 dmg。
- 当前只打本机架构（Apple Silicon 为 `aarch64`）；Intel Mac 需要另外在 Intel 机器上构建，或以后改成 universal。
- 签名与公证：脚本自动使用钥匙串里的「Developer ID Application」证书，并用 App Store Connect API Key 公证 .app 和 .dmg（配置在仓库外的 `~/.config/secrethub-release/macos.env`：`APPLE_API_ISSUER`、`APPLE_API_KEY`、`APPLE_API_KEY_PATH`，`.p8` 放 `~/.appstoreconnect/private_keys/`）。缺证书或配置时脚本直接报错；`--adhoc` 只用于本机测试（别人下载会被 Gatekeeper 拦）。开发版（`tauri dev` / 手动 `tauri build`）仍是 ad-hoc 签名。
- 与 Windows 的差异：Mac 安装更新只替换 `.app`，不会自行重启，所以点「更新」后由应用自己重启（`platform::INSTALLER_RESTARTS_APP`）；运行中的 `secrethub-mcp` 不用先结束（macOS 允许替换正在运行的文件）。

更多设计细节见 `docs/superpowers/specs/`，安全模型见 `SECURITY.md` 和 `docs/security/`。
