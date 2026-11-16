# 多模态 Key、模型更新模式、Key 半自动获取与素材库设计

> 状态：已确认（2026-10-28）。用户确认：全量模型更新只走维护者发布的目录补丁；其余采用本文推荐方案。
> 前置：模型能力路由（S1–S6）、模型目录更新（C1–C4）、标签库已完成。
> 实施顺序：M1 → M2 → M3 → M4（§6）。

## 0. 不变量（沿用并扩展）

1. Key 明文只在 Desktop 的 Rust 进程和用户自己的剪贴板里出现，**绝不进入 AI 上下文**，包括截图。
2. MCP 对 vault 只读；任何"写"或"触发动作"都以待处理文件入队，由 Desktop 执行或经用户确认。
3. 发往服务商的请求地址只能来自内置目录（host 固定），远程目录与 AI 都不能改变 Key 被发往的主机。
4. 素材、模型 ID、调用方式、能力标签不是秘密，可以返回给 AI。

---

## M1. 聚合型（多模态）Key

### M1.1 能力模型表

现有的 `secrets.model_id` / `model_env_key` 保持不变，作为"主模型"，也就是对话能力，v1 导出继续使用。新增表存放其他能力：

```sql
CREATE TABLE IF NOT EXISTS secret_capability_models (
  secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
  capability TEXT NOT NULL,   -- 能力词表中除 chat 外的值
  model_id TEXT NOT NULL,
  env_key TEXT NOT NULL,      -- 默认见 M1.3
  PRIMARY KEY (secret_id, capability)
);
```

- 对话能力就是 `model_id`，不在表里重复存储。
- 只存元数据，不涉及加密值。

### M1.2 表单差异化

- **触发条件**（满足其一即显示"多能力模型"区块）：
  - 服务商 `kind = aggregator`；
  - 该 Key 已同步的模型覆盖 ≥ 2 种能力（例如 OpenAI 同时有生图和语音）。
- **新建聚合型 Key**：主模型字段不再必填，提示"保存后会自动同步模型清单，再为各能力选择默认模型"。保存后自动检测并同步，然后打开该 Key 的详情。
- **"多能力模型"区块**（编辑表单和详情页都有）：每种能力一行，依次为 对话（即主模型）、代码、视觉理解、生图、文生视频/图生视频（video_gen）、语音合成、语音识别、向量、重排。
  - 下拉候选为该 Key 同步清单中具备这种能力的模型；没有候选的行显示"该 Key 暂无此能力的模型"。
  - 每行可编辑环境变量名，可清空。
- 详情页的"可用模型"列表：每行在原有"设为默认模型"之外，增加"设为某能力默认"的菜单。

### M1.3 默认环境变量名

| 能力 | 默认变量名 |
|---|---|
| code | `CODE_MODEL` |
| vision | `VISION_MODEL` |
| image_gen | `IMAGE_MODEL` |
| video_gen | `VIDEO_MODEL` |
| tts | `TTS_MODEL` |
| asr | `ASR_MODEL` |
| embedding | `EMBEDDING_MODEL` |
| rerank | `RERANK_MODEL` |
| reasoning | `REASONING_MODEL` |

用户可以修改。保存时校验格式，同一个 Key 内变量名不能重复。

### M1.4 导出与 AI 选型

- **手动导出**（`selected_entries`）：主模型行之后，按能力顺序追加能力表中的每一行（`ENV=model_id`）。
- **`secrethub_find_models`**：用户在能力表里选定的模型作为候选时，`capabilitySource` 记为 `user`，并加 `preferred: true`，排在该能力的最前面。
- **计划 v2**：不变。skill 会优先采用 `preferred` 候选和用户填写的变量名。

### M1.5 调用方式（invocations）

不同能力调用的是不同接口。在**内置**目录 `providers.json` 中按服务商加 `invocations`：

```jsonc
"invocations": {
  "chat":      { "kind": "openai-chat",        "path": "/chat/completions" },
  "image_gen": { "kind": "openai-images",      "path": "/images/generations" },
  "tts":       { "kind": "openai-audio-speech","path": "/audio/speech" },
  "asr":       { "kind": "openai-audio-transcriptions", "path": "/audio/transcriptions" },
  "embedding": { "kind": "openai-embeddings",  "path": "/embeddings" },
  "video_gen": { "kind": "async-task", "path": "/contents/generations/tasks", "pollPath": "/contents/generations/tasks/{id}" }
},
"docsUrl": "https://..."   // 官方文档入口，仅展示
```

- `kind` 枚举：`openai-chat`、`openai-images`、`openai-audio-speech`、`openai-audio-transcriptions`、`openai-embeddings`、`anthropic-messages`、`gemini-generate`、`async-task`、`native`。其中 `native` 表示非标准接口，只给 `docsUrl`。
- `path` 是相对于 `baseUrl` 的路径：必须以 `/` 开头，不得含 `//`、`..`、`:` 或空白。因此**主机始终是内置的 baseUrl**。
- invocations **只随程序版本发布**，不进远程目录白名单（远程补丁仍只能改模型名、默认模型、能力标签）。
- **新 MCP 工具** `secrethub_get_invocation({ secret_id, capability })`：返回 `{ kind, method, url (baseUrl+path), pollUrl?, authHeader: "Authorization: Bearer ${<API_KEY_ENV>}", bodySkeleton, envVars: {apiKey, baseUrl, model}, docsUrl }`。
  - 鉴权头只给模板，例如 `Authorization: Bearer ${DEEPSEEK_API_KEY}`，**不含 Key 值**。
  - `bodySkeleton` 由 `kind` 在 MCP 内生成，不存储任意文本。
- 各服务商 invocations 的数据由调研产出（§6 R1），每条都要有官方文档来源；不确定的标 `native`。

---

## M2. 模型资讯更新的两种模式

### M2.1 用户侧 skill：`skills/secrethub-model-refresh`

开始时先列出用户已有 API Key 的服务商（通过 `secrethub_list_secret_catalog`，只读元数据），然后询问：

> **A. 只更新已添加的 N 个服务商**（DeepSeek、OpenAI…）：同步真实模型清单，并补充新模型的能力和调用资料，耗时短、消耗少。
> **B. 全量更新**：由 SecretHub 官方发布目录补丁；请到 设置 → 模型目录 → 立即检查更新。

- **选 A**：
  1. 调用 `secrethub_request_model_sync`（M2.2）；
  2. 轮询 `secrethub_get_sync_status`，等待结果；
  3. 对结果中**新增**的模型，以及能力来源为 `unknown`/`heuristic` 的模型，按模型 ID 查官方公开文档，用 `secrethub_suggest_model_capabilities` 提交能力建议（需用户在 Desktop 确认）；
  4. 汇报：每个服务商新增/下线了哪些模型、哪些能力待确认。
- **选 B**：只做引导，不由 AI 修改目录。维护者的全量流程仍是 `skills/secrethub-catalog-maintainer`，即"核对 → 签名 → 上传服务器 → 用户自动获取补丁"。
- 全程不读取、不索要 Key。

### M2.2 MCP 请求同步

- 新工具 `secrethub_request_model_sync({ secret_ids? })`：
  - 缺省时为所有未归档、有值的 API Key；
  - 写入 `mcp-pending/sync-<id>.json`；
  - 返回 requestId。
- **Desktop 处理**：
  - 保险库解锁时，自动依次执行同步，复用 S2 的检测加同步逻辑，每个 Key 之间间隔 ≥ 1 秒；
  - 保险库锁定时保持 pending，横幅提示"AI 请求同步模型清单，解锁后自动执行"。
  - 这个动作等同于用户点"检测"：Key 只发往内置地址。所以**不弹确认框**，但会记入审计。
- **结果写入** `mcp-results/sync-<id>.json`：

  ```json
  { "requestId": "...", "status": "done", "secrets": [{ "secretId": "...", "providerId": "deepseek", "modelsSyncStatus": "ok", "count": 12, "added": ["..."], "removed": ["..."] }] }
  ```

  其中 `added`/`removed` 由同步前后的模型集合差得出。
- 新工具 `secrethub_get_sync_status({ request_id })`，返回 `pending | done | not_found`，并附上述结果。
- 频率限制：同一个 Key 60 秒内重复的同步请求直接复用上次结果。

---

## M3. Key 半自动获取（剪贴板捕获）

### M3.1 流程

1. 触发方式有两种：
   - **用户自己**：在服务商卡片或添加表单点"打开官网并自动捕获 Key"；
   - **AI**：调用 `secrethub_start_key_capture({ provider_id })` 入队。Desktop 弹出"AI 请求帮你获取 DeepSeek 的 API Key"，**用户点"开始"后**才进入下一步。读取剪贴板必须经用户同意。
2. Desktop 用系统默认浏览器打开该服务商的 Key 页面（`keyUrl`，内置数据）。用户平时登录过的话会直接进入，没登录则自己登录。
3. Desktop 进入"等待粘贴"状态（最长 3 分钟，可取消），界面显示倒计时和提示："在官网创建 Key 后点复制即可"。
4. 用户在官网创建 Key 并点"复制"。
5. Desktop 检测到剪贴板出现新内容，并且看起来像 Key（M3.2）：
   - 用该服务商的预设直接创建凭据，名称为"<服务商> · 自动捕获 <日期>"，环境变量名、Base URL、默认模型都来自目录；
   - 如果剪贴板内容仍是这个 Key，则清空剪贴板；
   - 立即检测并同步模型；聚合型 Key 接着打开多能力模型表（M1.2）。
6. 结果通过 `key_capture_status()` 返回给前端，只含状态和 secretId，不含值。AI 侧用 `secrethub_get_capture_status({ request_id })` 获取 `waiting | captured | timeout | cancelled | declined`。

**AI 不需要控制浏览器**。页面由 Desktop 在用户自己的浏览器里打开。AI 的工作只有：查公开文档里的最新模型和调用方式、发起捕获请求、等待结果、再发起同步和能力建议。因此不存在"截图截到 Key"的风险。

### M3.2 识别规则

- 开始时记录剪贴板当前内容的哈希，只认此后的**新内容**。
- 去掉首尾空白后，必须满足：单行、长度 16–512、全部为可打印 ASCII、不含空格。
- 服务商有 `keyPrefixes` 时必须匹配其中之一，否则不认。
- 无前缀的服务商：命中基本规则即认。界面提示"已捕获，正在检测"；检测结果为 `unauthorized`/`invalid` 时提示用户检查后删除重试。
- 剪贴板轮询间隔 500 ms；内容只在 Rust 内存里比较，不记日志，比较完立即丢弃。

### M3.3 Skill：`skills/secrethub-key-onboarding`

用户说"帮我配好 DeepSeek 的 Key"时，按以下步骤执行：

1. 确认服务商；
2. 查公开文档，获取最新模型名和调用方式，作为能力建议的依据；
3. 调用 `secrethub_start_key_capture`；
4. 提示用户在 Desktop 点"开始"，然后在官网创建并复制 Key；
5. 轮询捕获状态；
6. 成功后调用 `secrethub_request_model_sync`，并按需提交能力建议；
7. 汇报结果。

skill 明确禁止：让用户把 Key 发进聊天、用浏览器工具截取 Key 页面、代用户输入密码或完成验证码。

---

## M4. 媒体素材库

### M4.1 存储

- 文件**复制**进 SecretHub 管理的目录 `<应用数据>/media/<id>.<ext>`；按 sha256 去重，同一文件不重复存储。
- 素材不加密（不是秘密）。素材库在保险库锁定时仍可浏览；MCP 读取也不要求解锁。
- 数据表：

  ```sql
  CREATE TABLE IF NOT EXISTS media_assets (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,          -- image | video | audio | text | other
    filename TEXT NOT NULL,      -- 原文件名（展示用）
    stored_path TEXT NOT NULL,   -- 绝对路径
    sha256 TEXT NOT NULL UNIQUE,
    size_bytes INTEGER NOT NULL,
    mime TEXT NOT NULL DEFAULT '',
    width INTEGER, height INTEGER, duration_ms INTEGER,
    description TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL DEFAULT 'user',  -- user | ai
    created_at INTEGER NOT NULL
  );
  ```

- 标签沿用现有标签库（`tags` 表及其选择器）。
- 元数据提取：
  - 图片尺寸：读文件头（`imagesize` crate）；
  - 音视频时长和分辨率：本机有 `ffprobe` 时调用，没有则留空，并在界面提示"安装 ffmpeg 可显示时长"。
- 单个文件上限 2 GB。

### M4.2 桌面界面

- 新导航"素材库"：按类型筛选（全部/图片/视频/音频/文本），支持标签筛选和搜索。
- 卡片显示缩略图、名称、尺寸、时长、方向（横/竖/方）。图片缩略图通过 Tauri asset 协议显示，作用域仅限 media 目录。
- 导入方式：按钮多选文件，或拖拽到窗口；"新建文本素材"可直接写提示词或脚本，保存为 `.txt`。
- 卡片操作：编辑描述和标签、复制路径、在文件夹中显示、删除（需确认，删除库内副本，不影响原文件）。
- 顶部显示占用空间。

### M4.3 MCP 工具

- `secrethub_media_search({ kind?, tags?, query?, orientation?, min_duration_ms?, max_duration_ms?, limit })`：返回 `{ id, kind, filename, path, mime, sizeBytes, width, height, durationMs, orientation, description, tags }`。`path` 为绝对路径，可直接用于上传或复制到项目测试目录。
- `secrethub_media_get({ id })`。
- `secrethub_media_add({ path, description?, tags? })`：用于 AI 把生成的测试结果存回库里。
  - 写入 `mcp-pending/media-<id>.json`，Desktop 显示"AI 请求加入素材库：<文件名、大小、路径>"，用户确认后才复制入库（`source = ai`）；
  - 路径必须是存在的普通文件，且不超过大小上限。
- MCP 只读访问 `media_assets` 表，不读写文件内容。

---

## 5. 安全要点汇总

| 风险 | 对策 |
|---|---|
| Key 被 AI 看到 | 捕获全程在 Rust 与剪贴板之间完成；AI 不控制浏览器；所有状态接口不返回值 |
| 未经同意读取剪贴板 | 只在用户点"开始"后读取，最长 3 分钟，可取消；只认新内容 |
| 误把普通文本当 Key 保存 | 格式规则和前缀匹配；保存后立即检测，无效时提示删除 |
| 调用方式把 Key 发到别处 | invocations 只存相对路径，主机固定为内置 baseUrl；只随版本发布 |
| AI 把敏感文件塞进素材库 | 入库需用户确认，并显示完整路径和大小 |
| 频繁触发同步 | 每个 Key 60 秒内去重，Key 之间间隔 ≥ 1 秒 |

## 6. 实施切片

| 切片 | 内容 | 执行者 |
|---|---|---|
| R1 | 调研：逐家整理 36 个服务商各能力的调用方式（kind、path、轮询方式、文档链接），输出 `docs/evidence/provider-invocations-2026-10-28.md`，每条带官方来源，不确定的标 `native` | Haiku 子代理（只读调研） |
| M1a | 能力模型表、表单"多能力模型"区块、导出追加、`find_models` 偏好 | Sonnet 子代理 |
| M1b | `invocations` 格式与校验、按 R1 结果写入内置数据、`secrethub_get_invocation` | Sonnet 子代理（R1 需先经复核） |
| M2 | `request_model_sync` / `get_sync_status`、模型增删差异、Desktop 自动执行、`secrethub-model-refresh` skill | Sonnet 子代理 |
| M3 | 剪贴板捕获（Rust）、界面倒计时、`start_key_capture` / `get_capture_status`、`secrethub-key-onboarding` skill | Sonnet 子代理 |
| M4 | 素材库存储、元数据提取、界面、MCP 三个工具、入库确认队列 | Sonnet 子代理 |

每个切片单独提交，提交前跑全部测试。M1a 到 M4 都会修改 `App.tsx`、`commands.rs`、MCP 的 `index.ts`，因此**按顺序执行，不并行**。R1 是只读调研，可以和 M1a 同时进行。
