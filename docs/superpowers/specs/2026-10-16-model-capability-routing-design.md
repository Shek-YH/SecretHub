# 模型能力路由设计（P1 + P2）

> 状态：已确认（2026-10-16）。§14 的 4 个决策均采用推荐方案；按 §12"实施切片"从 S1 开始交给 Sonnet 实现。
> 前置：P0 已完成（commit a25d5b4）——MCP 每次调用重读 vault、计划确认合并已有 `.env`、服务商目录统一到 `packages/provider-definitions/providers.json`。

## 1. 目标

让 AI 在开新项目时"按需取用"模型：先判断项目需要哪些能力（对话、视觉、生图、语音……），再从 SecretHub 里找到**已验证的 Key 实际能调用的**对应模型，最后由 SecretHub Desktop 把 Key、Base URL 和各能力的模型名写进目标项目 `.env`。

一个聚合站 Key（OpenRouter、APIMart 等）可以同时满足多种能力：`.env` 里 Key 只写一次，每种能力各写一个模型变量。

## 2. 非目标

- 不让 AI 用浏览器登录服务商后台抓取页面。模型清单只通过服务商 API（`/models` 一类接口）获取。
- 不做模型效果评测、价格比较或自动选"最便宜"的模型。
- 不改变 Secret 加密、导出安全检查（`.gitignore`、已跟踪 `.env`）等现有机制。
- 常用网站、推广链接清理、Proxy 去重（todo 15–17）不在本文范围。

## 3. 安全边界（不变量）

所有新增能力必须满足：

1. **Key 明文只在 Desktop 的 Rust 进程里出现**。模型同步、能力推断都在 Rust 侧完成；MCP 和 AI 只能看到元数据。
2. **MCP 对 vault 只读**。MCP 需要"写"的东西（计划、能力建议）一律写成待确认文件，由 Desktop 展示并经用户确认后落库或落盘。
3. **模型 ID、Base URL、能力标签不是秘密**，可以出现在 MCP 返回值、确认界面和 `.env.example` 的说明中；Key 值只以掩码显示。
4. 服务商返回的模型名、描述属于不可信数据：只当字符串存储和展示，长度截断，不参与任何命令拼接。

## 4. 能力词表

固定枚举，Rust、TS、skill 共用：

| 能力 | 含义 | 典型判断依据 |
|---|---|---|
| `chat` | 文本对话 / 通用 LLM | 输出 text 的对话模型 |
| `code` | 编程专用或编程优化 | `coder`、`codestral`、`*-code` |
| `reasoning` | 深度推理 / thinking | OpenRouter `supported_parameters` 含 `reasoning`；`r1`、`thinking` |
| `vision` | 图像理解（输入含 image） | `input_modalities` 含 `image`；`-vl`、`vision` |
| `image_gen` | 生成图片 | `output_modalities` 含 `image`；`seedream`、`flux`、`gpt-image` |
| `video_gen` | 生成视频 | `seedance`、`veo`、`kling`、`sora` |
| `tts` | 语音合成 | `output_modalities` 含 `audio`；`tts`、`cosyvoice` |
| `asr` | 语音识别 | `whisper`、`asr`、`paraformer`、`transcribe` |
| `embedding` | 向量嵌入 | `embedding`、`embed`、`bge`、`e5` |
| `rerank` | 重排序 | `rerank` |

一个模型可有多个能力（例如 `["chat", "vision", "reasoning"]`）。判断不了的模型记为 `[]`，`capabilitySource = "unknown"`。

## 5. providers.json 扩展

在 P0 的统一目录上增加字段（全部可选，缺省值保证旧数据可用）：

```jsonc
{
  "id": "openrouter",
  "kind": "aggregator",            // "direct" | "aggregator"，缺省 "direct"
  "protocol": "openai",            // "openai" | "anthropic" | "gemini"，缺省 "openai"
  "validationUrl": "https://openrouter.ai/api/v1/models", // 已有字段，同时作为模型清单地址
  "models": [
    { "id": "openai/gpt-5.6-sol", "capabilities": ["chat", "vision", "reasoning"] }
  ]
}
```

- `kind`：用于排序和界面标识。OpenRouter、OpenCode Zen/Go、CC Vibe、APIMart、88API、GrooRoute、BytePlus、千帆、TokenHub、火山方舟这类"一个 Key 多家模型"的标 `aggregator`。
- `protocol`：决定 `/models` 响应的解析方式和请求头。目前 Anthropic、Gemini 的请求头特判写在 `commands.rs` 里，改为读这个字段。
- 模型级 `capabilities`：内置目录里给已知模型手工标注，作为第二优先级来源（见 §7）。
- **模型清单地址**：优先用 `validationUrl`；没有时，`protocol = "openai"` 的服务商回退为 `{baseUrl}/models`。

模型名推断规则单独放在 `packages/provider-definitions/capability-rules.json`（正则 → 能力列表），便于审阅和扩充，不写死在代码里。

## 6. 模型同步（Desktop / Rust）

### 6.1 触发时机

- 用户点"检测"或"全部检测"时，在同一次请求里完成验证和模型同步。不额外发请求。
- 卡片上新增"同步模型"按钮，只重新拉取清单。
- 不做后台定时同步（避免无感知的网络请求）。

### 6.2 请求与状态判定

沿用现有安全约束：不跟随重定向、固定 HTTPS、地址来自目录而非用户输入。新增：

- 超时从 8s 调到 15s；响应体上限 8 MB；最多解析 2000 个模型；Anthropic、Gemini 的分页最多拉 5 页。
- **回退地址的状态修正**：对 `{baseUrl}/models` 这类推导出来的地址，`404/405` 记为 `unsupported`，而不是 `invalid`。否则不提供 `/models` 的服务商会把有效 Key 误判为无效。显式配置的 `validationUrl` 保持现有判定。
- 解析失败不影响验证状态：Key 仍为 `valid`，模型同步记为失败，界面提示"已验证，但无法读取模型清单"。
- 命令改为 `async` + 非阻塞 `reqwest`，避免批量检测卡住命令线程（顺带完成 todo 18）。

### 6.3 解析（纯函数，放 `crates/providers`）

`parse_model_list(protocol, body) -> Vec<DiscoveredModel>`：

| 协议 | 列表位置 | 可用的能力线索 |
|---|---|---|
| openai | `data[].id` | OpenRouter：`architecture.input_modalities` / `output_modalities`、`supported_parameters`；new-api 系网关：`supported_endpoint_types`；其余只有 `id`、`owned_by` |
| anthropic | `data[].id`、`display_name`；分页 `has_more` / `last_id` | 无，走目录和规则 |
| gemini | `models[].name`（去掉 `models/` 前缀）；分页 `nextPageToken` | `supportedGenerationMethods`（如 `embedContent` → `embedding`） |

`DiscoveredModel { id, display_name?, input: Vec<String>, output: Vec<String>, hints: Vec<String> }`。

> 已核实（2026-10-16）：OpenRouter 公开的 `/api/v1/models` 有 465 个模型，含 `architecture.input_modalities/output_modalities`，模态取值有 text/image/video/file/audio。new-api 的 `/v1/models` 每个对象带 `SupportedEndpointTypes`，但具体取值和 Gemini 的字段需在实现时用真实响应核对，并写成测试夹具。

### 6.4 存储

两张新表，都只存元数据：

```sql
-- 某个 Key 实际能看到的模型（同步结果，按 Key 存）
CREATE TABLE IF NOT EXISTS provider_models (
  secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL,
  display_name TEXT NOT NULL DEFAULT '',
  input_json TEXT NOT NULL DEFAULT '[]',
  output_json TEXT NOT NULL DEFAULT '[]',
  capabilities_json TEXT NOT NULL DEFAULT '[]',
  capability_source TEXT NOT NULL DEFAULT 'unknown', -- provider|catalog|heuristic|unknown
  synced_at INTEGER NOT NULL,
  PRIMARY KEY (secret_id, model_id)
);

-- 人工或 AI 确认过的能力标签（按服务商+模型存，跨 Key 复用，同步不覆盖）
CREATE TABLE IF NOT EXISTS model_capability_overrides (
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  capabilities_json TEXT NOT NULL,
  source TEXT NOT NULL,          -- user|ai
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (provider_id, model_id)
);
```

`secrets` 表通过现有的 `add_column_if_missing` 增加 `models_synced_at INTEGER NOT NULL DEFAULT 0` 和 `models_sync_status TEXT NOT NULL DEFAULT ''`（`ok` / `failed` / `unsupported`）。

同步时整批替换该 Key 的 `provider_models` 行（事务内先删后插）；`model_capability_overrides` 永不被同步覆盖。

## 7. 能力推断

在 Rust 侧同步时计算，结果写进 `provider_models.capabilities_json`。查询时如果存在 override，以 override 为准。优先级从高到低：

1. **override**（`user` / `ai`，用户已确认）
2. **provider**：服务商响应里的模态和参数字段
3. **catalog**：`providers.json` 里手工标注的模型能力
4. **heuristic**：`capability-rules.json` 的模型名规则
5. **unknown**：`[]`

只有一个来源时直接采用。provider 来源会和 heuristic 合并（例如 OpenRouter 只给出 text→text，但名字里有 `coder`，就补上 `code`）。

### AI 辅助标注

对 `unknown` 或 `heuristic` 的模型，AI 可以根据**模型 ID**（不是秘密）联网查文档，再调用新 MCP 工具提交建议：

- `secrethub_suggest_model_capabilities({ provider_id, suggestions: [{ model_id, capabilities, evidence_url }] })`
- MCP 写入 `mcp-pending/caps-<id>.json`，不碰数据库。
- Desktop 显示"AI 建议了 N 个模型的能力标签"，用户逐条或一键确认后写入 overrides（`source = 'ai'`），拒绝则丢弃。

## 8. MCP 工具

### 8.1 `secrethub_find_models`（新增）

输入：

```json
{ "capabilities": ["chat", "image_gen"], "provider": "openrouter", "include_unverified": false, "limit": 50 }
```

每个能力分别返回候选，候选项结构：

```json
{
  "secretId": "s1", "secretName": "OpenRouter 主账号", "providerId": "openrouter",
  "providerKind": "aggregator", "protocol": "openai",
  "baseUrl": "https://openrouter.ai/api/v1",
  "apiKeyEnvKey": "OPENROUTER_API_KEY", "baseUrlEnvKey": "OPENROUTER_BASE_URL",
  "modelId": "google/gemini-3.8-flash", "capabilities": ["chat", "vision"],
  "capabilitySource": "provider", "keyStatus": "valid", "modelsSyncedAt": 1760600000
}
```

- 默认只返回 `keyStatus = valid` 且已同步模型的 Key。`include_unverified: true` 时附带未验证的，并标出原因。
- 排序：Key 已验证优先 → 能力来源可信度（override > provider > catalog > heuristic）→ 能一次覆盖最多所请求能力的 Key 优先（聚合站因此会自然排前）。
- 同时返回 `uncovered: ["video_gen"]`，列出没有任何候选的能力，供 skill 告知用户。
- todo 9（按能力推荐）由本工具覆盖；`secrethub_recommend_secrets_for_project` 保持现状，只负责按技术栈推荐 Key。

### 8.2 `secrethub_prepare_project_env` v2

输入新增 `bindings`，旧的 `secret_ids` 继续支持：

```json
{
  "project_path": "D:/work/demo",
  "bindings": [
    { "secret_id": "s1", "capability": "chat", "model_id": "google/gemini-3.8-flash",
      "env": { "api_key": "OPENROUTER_API_KEY", "base_url": "OPENROUTER_BASE_URL", "model": "LLM_MODEL" } },
    { "secret_id": "s1", "capability": "image_gen", "model_id": "<该 Key 清单里的某个生图模型 ID>",
      "env": { "model": "IMAGE_MODEL" } }
  ]
}
```

计划文件（`mcp-pending/plan-*.json`）增加 `"version": 2` 和 `bindings`，仍然不含任何 Key 值。

### 8.3 `secrethub_get_plan_status`（新增）

输入 `request_id`，返回 `pending | written | declined | failed | not_found`。`written` 时附带 `writtenKeys`、`skippedKeys`，`failed` 时附带脱敏的错误码。

Desktop 处理完计划后写入 `mcp-results/<requestId>.json`，再删除对应的 pending 文件；结果文件只保留最近 50 个。

## 9. Desktop 计划审核与写入

### 9.1 绑定展开规则（Rust，纯函数，可单测）

- 每个绑定的 `env.api_key`、`env.base_url` 按 `(secret_id, 变量名)` 去重，Key 只写一次。
- 每个绑定的 `env.model` 各写一行，值为 `model_id`。
- `env` 中省略的字段不写。例如项目不需要 Base URL，就不传 `base_url`。
- 校验：变量名合法；同一变量名对应两个不同的值时，整个计划拒绝；`secret_id` 存在且有值；`model_id` 不在该 Key 已同步的清单里时，不拒绝，但在审核界面标黄"未在该 Key 的模型清单中"。
- v1 计划（只有 `secret_ids`）继续走现有的 `selected_entries` 逻辑。

### 9.2 审核界面

现在的横幅改为"查看"按钮，打开审核弹窗：

| 列 | 内容 |
|---|---|
| 变量名 | `OPENROUTER_API_KEY` / `LLM_MODEL` … |
| 来源 | Secret 名称 + 用途（Key / Base URL / 某能力的模型） |
| 值 | Key 显示掩码；模型 ID 和 Base URL 明文显示 |
| 状态 | 新增 / 覆盖已有值 / 模型未在清单中 |
| 操作 | 冲突项可选"覆盖"或"跳过"（默认覆盖） |

底部有"批准写入"和"拒绝"两个按钮。批准后把选择"跳过"的变量从条目中去掉，再调用 P0 的 `merge_env` / `merge_example`。

## 10. 环境变量命名

AI 负责命名，Desktop 只校验格式：

1. 项目代码或 `.env.example` 里已有的变量名优先（skill 只搜变量名，不读值）。
2. 项目只用一种能力时，使用目录里的默认名（如 `OPENROUTER_MODEL`）。
3. 多种能力共用一个服务商时，用按能力命名的默认名：`LLM_MODEL`、`CODE_MODEL`、`VISION_MODEL`、`IMAGE_MODEL`、`VIDEO_MODEL`、`TTS_MODEL`、`ASR_MODEL`、`EMBEDDING_MODEL`、`RERANK_MODEL`。

## 11. Skill 流程更新

`skills/secrethub-project-env/SKILL.md` 改为：

1. 读项目：技术栈、已有的环境变量名（不读值）、代码里用到的 SDK。
2. **列出能力需求**，例如"chat + vision + image_gen"，并标注依据（哪个文件或需求描述）。
3. 调用 `secrethub_find_models`。有 `uncovered` 的能力时，告诉用户需要在 SecretHub 添加或验证哪类 Key，然后暂停。
4. 有 `heuristic` 或 `unknown` 候选时，可以联网核实模型 ID 对应的能力，并用 `secrethub_suggest_model_capabilities` 提交建议（需用户在 Desktop 确认）。
5. **给用户看选型表**（能力 → Key → 模型 → 变量名），用户确认后再生成计划。
6. 调用 `secrethub_prepare_project_env` v2。
7. 用户在 Desktop 审核后，调用 `secrethub_get_plan_status` 汇报结果（只报变量名和状态）。

`evals/evals.json` 新增 3 个用例：

- 多模态项目（对话 + 生图）只有一个已验证的聚合站 Key → 应生成一个 Key、两个模型变量的计划。
- 项目需要 TTS，但没有任何 Key 覆盖 → 应报告 `uncovered` 并暂停，不生成计划。
- 用户点名的模型不在 Key 的同步清单里 → 应提示风险，并让用户在选型表上确认。

## 12. 实施切片

每个切片单独提交，并且可独立验证。

| 切片 | 内容 | 对应 todo | 验收 |
|---|---|---|---|
| S1 | `providers.json` 增加 `kind` / `protocol` / 模型能力标注；新建 `capability-rules.json`；Rust 侧 `parse_model_list` + `infer_capabilities` 纯函数 | 4、7 | 三种协议和 OpenRouter 各有真实响应夹具（去掉敏感字段），单测覆盖解析、分页截断、推断优先级 |
| S2 | 两张新表和 `secrets` 新列；验证命令改为 async，同时同步模型；回退地址把 404 判为 `unsupported`；卡片显示同步状态和"同步模型"按钮 | 5、6、18 | storage 迁移测试；回退判定单测；Desktop 手动验证一个真实 Key |
| S3 | MCP `secrethub_find_models`，排序和 `uncovered` 输出 | 8、9 | vitest：以 SQLite 夹具覆盖排序、过滤、uncovered |
| S4 | 计划 v2：展开规则、校验、结果文件；MCP `prepare_project_env` v2 + `get_plan_status` | 10、12 | Rust 单测覆盖展开去重、冲突拒绝、v1 兼容；MCP 协议冒烟测试 |
| S5 | Desktop 审核弹窗（逐项覆盖/跳过、拒绝按钮）；能力建议确认队列 + `suggest_model_capabilities` | 11、7 | 前端测试；用测试项目在应用里走一遍完整流程 |
| S6 | 更新 skill 文档和 evals | 13、14 | 跑 skill evals |

S1 → S2 → S3 是 P1；S4 → S5 → S6 是 P2。S3 和 S4 只依赖 S2 的表结构，S2 完成后可以并行。

## 13. 风险与待定问题

| 风险 / 问题 | 处理 |
|---|---|
| 编程套餐类地址（智谱 coding、火山 coding/v3、千帆 coding）的 `/models` 可能只列出套餐内模型，或与通用接口不同 | 以实际返回为准；`validationUrl` 已为部分服务商指向通用接口，实现 S2 时逐个核对 |
| 多数 OpenAI 兼容网关只返回 `id`，能力只能靠规则猜，会有误判 | 来源字段透明展示；选型表需用户确认；AI 建议走确认队列 |
| 聚合站返回几百个模型，MCP 输出过长 | `find_models` 只按请求的能力过滤并 `limit`；仍受 25k 字符上限约束 |
| 部分服务商不提供 `/models` | 回退地址 404 判为 `unsupported`；仍可用内置目录模型手动绑定（标 `catalog`） |
| new-api 的 `supported_endpoint_types` 取值、Gemini 的字段名尚未用真实响应核对 | S1 实现前先抓真实响应做夹具；不确定的字段只作为线索，不作为唯一依据 |

## 14. 已确认的决策

以下 4 项均于 2026-10-16 按推荐方案确认。

1. **AI 能力建议必须经 Desktop 确认后才生效**（§7）。另一种做法是直接写入、仅标记来源，更省事但 AI 误判会直接影响选型。本文推荐需要确认。
2. **v2 计划继续兼容 v1 的 `secret_ids`**（§8.2）。推荐兼容，成本很低。
3. **验证与模型同步合并为一次请求，并把检测命令改为 async**（§6）。推荐合并，不额外消耗请求。
4. **不做后台定时同步**，只在用户点击检测或同步时拉取（§6.1）。
