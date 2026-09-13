# SecretHub — AI/Vibe Coding 本地凭据与开发资源管理中心

> 文档类型：产品需求文档（PRD）  
> 面向对象：Codex / 开发团队 / QA  
> 目标平台：Windows 11 优先，后续兼容 macOS  
> 产品形态：本地优先桌面应用 + CLI + MCP Server  
> 当前版本目标：V1.0 MVP  
> 产品代号：SecretHub

---

## 0. 给 Codex 的执行要求

本项目不是普通“密码管理器”，而是面向 AI Coding / Vibe Coding / 多项目开发场景的本地 Secret 与开发资料管理中心。

Codex 在开始编码前，必须先完成以下事项：

1. 阅读本 PRD 全文。
2. 输出技术选型确认文档。
3. 输出模块拆分与目录结构。
4. 输出威胁模型与安全边界。
5. 输出 V1 任务台账。
6. 先实现一个可运行的最小垂直切片：
   - 添加 Secret
   - 本地加密保存
   - 列表读取
   - 勾选多个 Secret
   - 选择本地项目目录
   - 生成 `.env`
7. 垂直切片通过测试后，再扩展完整 V1。
8. 涉及 UI/UX 的实现，必须同步生成并维护高保真可交互原型。
9. 不得为了追求功能完整而弱化安全要求。
10. 不得把真实 API Key 写入日志、测试快照、Crash Report、遥测、Git 仓库、示例文件或 AI 上下文。

---

# 1. 项目背景

用户频繁创建 AI / Web / Desktop / Agent 项目。

不同项目往往需要不同的：

- API Key
- Access Token
- Base URL
- Project ID
- Org ID
- Database URL
- Webhook
- SSH Key
- License Key
- Supabase 配置
- Cloudflare 配置
- GitHub Token
- OpenAI / Anthropic / Gemini / xAI / OpenRouter / DeepSeek 等模型服务凭据

当前工作流通常是：

1. 创建一个新项目。
2. 回忆项目需要哪些服务。
3. 打开多个服务商网站。
4. 登录。
5. 找 API Key 页面。
6. 找到以前申请的 Key，或者重新创建。
7. 复制 Key。
8. 打开项目目录。
9. 新建 `.env`。
10. 手工写变量名。
11. 复制 Value。
12. 重复若干次。
13. 检查 `.gitignore`。
14. 如果另一个项目也需要同一组凭据，再重复一次。

该流程存在明显问题：

- 重复劳动。
- 容易复制错 Key。
- 容易写错变量名。
- 容易漏掉 Base URL、Project ID 等关联字段。
- 容易将 `.env` 提交到 Git。
- 不方便 AI Agent 自动配置项目环境。
- Key 分散在大量第三方网站中。
- 缺少统一的有效性、用途、标签、备注与项目映射。
- AI Agent 如果直接读取 `.env`，有泄露真实 Secret 到上下文、日志或会话记录的风险。

SecretHub 的目标就是解决以上问题。

---

# 2. 产品定位

SecretHub 是：

> 一个 Local-First、AI-Safe、Project-Aware 的开发凭据与资源中心。

它不是：

- 云端团队密码管理平台。
- 企业级 IAM 替代品。
- AWS Secrets Manager / Vault 的生产环境替代品。
- 单纯保存密码的 Password Manager。
- 单纯的 `.env` 编辑器。

核心价值：

1. **所有开发凭据只维护一次。**
2. **不同项目按需选择。**
3. **一键输出到指定项目。**
4. **AI 可以知道“有哪些资源”，但默认看不到真实 Secret。**
5. **未来可由 AI 自动规划项目需要哪些凭据。**
6. **进一步升级后，可以不生成明文 `.env`，直接在运行时安全注入。**

---

# 3. 产品目标

## 3.1 V1 核心目标

完成一个 Windows 本地桌面应用，使用户可以：

1. 添加、编辑、删除 Secret。
2. 将 Secret 按 Provider / 类型 / 标签 /用途分类。
3. 安全加密保存在本地。
4. 搜索和筛选 Secret。
5. 勾选任意多个 Secret。
6. 选择目标项目目录。
7. 一键生成：
   - `.env`
   - 可选 `.env.example`
8. 在生成前检查冲突与覆盖风险。
9. 记录操作审计，但绝不记录 Secret 明文。
10. 支持常用 Secret 组合 Profile。

## 3.2 V2 目标

加入 AI / MCP 能力：

- AI 查看 Secret Catalog。
- AI 查询 Metadata。
- AI 根据项目技术栈推荐需要哪些 Secret。
- 用户确认后由 SecretHub 写入项目。
- MCP 默认不得返回明文 Secret。

## 3.3 V3 目标

运行时 Secret Injection：

- 不生成明文 `.env`。
- Secret 只在子进程运行时注入。
- 支持 Codex、Claude Code、Cursor、Node、Python、MCP Server 等进程。
- AI Agent 尽可能拿不到真实长期 Key。

## 3.4 V4 目标

与“AI 项目台账 / 自动开发 Skill”联动：

技术选型完成后：

```text
项目需求
↓
AI 判断所需第三方服务
↓
查询 SecretHub Catalog
↓
匹配已有 Secret
↓
提示缺失项
↓
用户确认
↓
自动准备项目环境
↓
开始开发
```

---

# 4. 非目标

V1 暂不做：

- 企业多人协作。
- 云端同步。
- RBAC。
- SSO。
- 审批流。
- 服务端 Secret Rotation。
- Kubernetes Secret Operator。
- CI/CD Secret 注入。
- 手机端。
- 浏览器自动抓取第三方网站 API Key。
- 自动登录服务商后台。
- 自动创建新的 API Key。
- 远程分享 Secret。

这些能力以后可以立项，但不能影响 V1 简洁性。

---

# 5. 开源项目调研与复用策略

## 5.1 Townrain/API-Key-Manager

仓库：

https://github.com/Townrain/API-Key-Manager

适合复用/借鉴：

- Provider Registry。
- 45+ AI 服务商识别体系。
- API Key 格式检测。
- Provider 自动识别。
- API Key Validity Check。
- Provider 扩展机制。
- Windows 桌面 GUI 思路。
- React + pywebview 桌面结构思路。
- AES-256-GCM 加密思路。
- Key Masking。
- SSRF / Path Traversal 等安全检查。

许可证：

- MIT。

使用原则：

- 优先复用独立、低耦合模块。
- 不建议整体 Fork 后重改。
- Provider Registry 与 Key Validator 可作为独立 Adapter 层接入。

---

## 5.2 rapg

仓库：

https://github.com/kanywst/rapg

适合复用/借鉴：

- Local-first Secret Vault。
- 项目 Namespace。
- Project Scoped Secrets。
- Secret 白名单。
- 运行时环境变量注入。
- AI Agent 安全边界设计。
- Audit Log。
- Transcript Redaction 思路。
- Provider Proxy 思路。
- 项目配置文件自动发现。

典型思想：

```toml
namespace = "cuecut"

keys = [
  "OPENAI_API_KEY",
  "GEMINI_API_KEY"
]
```

许可证：

- MIT。

重点：

V3 的 Runtime Injection 应优先参考此项目，而不是自行发明整套进程注入安全逻辑。

---

## 5.3 Infisical

仓库：

https://github.com/Infisical/infisical

用途：

- 参考成熟 Secret Manager 的项目/环境/文件夹/标签数据模型。
- 参考 Secret Export。
- 参考 Secret Version / Audit 等设计。
- 参考 Project / Environment 概念。

许可证注意：

- 非 `ee/` 核心部分主要为 MIT Expat。
- `ee/` Enterprise 目录有独立许可证。
- 禁止把 Enterprise 代码误当 MIT 代码迁移。

V1 不建议把 Infisical 整体嵌入。

---

## 5.4 Infisical MCP Server

仓库：

https://github.com/Infisical/infisical-mcp-server

用途：

重点参考 MCP Tool 设计，包括：

- list secrets
- get metadata
- create secret
- update secret
- list projects
- folder / environment 等结构

许可证：

- Apache-2.0。

SecretHub MCP 不得照搬“返回真实 Secret”能力，应加入更严格的 AI 安全边界。

---

## 5.5 dotenvx

仓库：

https://github.com/dotenvx/dotenvx

用途：

- dotenv 解析。
- dotenv 写入。
- `.env` 相关安全设计。
- 环境变量注入。
- 加密 `.env` 思路。

原则：

优先使用成熟 Parser / Writer，避免自行实现复杂 dotenv escaping。

---

## 5.6 Tene

仓库：

https://github.com/tene-ai/tene

用途：

参考：

- AI-safe Secret Manager。
- Claude Code / Cursor / Codex / AI Agent 场景。
- Local-first。
- 加密 Vault。
- Agent Secret Isolation。

许可证：

- MIT。

---

## 5.7 envkeep

仓库：

https://github.com/jackofshadowz/envkeep

用途：

参考：

- Secret + ENV。
- Notes。
- Tags。
- Extra Fields。
- GUI/CLI。
- Agent-friendly workflow。

许可证：

- MIT。

---

## 5.8 PX Secrets

仓库：

https://github.com/pxinnovative/px-secrets

用途：

只参考：

- 本地 Vault UX。
- SOPS + AGE 架构思路。

许可证：

- AGPL-3.0。

要求：

**不要复制其代码进入 SecretHub。**

---

# 6. 总体架构

推荐架构：

```text
┌──────────────────────────────┐
│ SecretHub Desktop App        │
│ React / TypeScript           │
└──────────────┬───────────────┘
               │ IPC
               ▼
┌──────────────────────────────┐
│ Local Core Service           │
│ Rust                         │
├──────────────────────────────┤
│ Vault Service                │
│ Crypto Service               │
│ Provider Registry            │
│ Secret Validation            │
│ Profile Service              │
│ Project Service              │
│ Export Service               │
│ Audit Service                │
│ Policy Engine                │
└──────────────┬───────────────┘
               │
      ┌────────┴────────┐
      ▼                 ▼
Encrypted DB       OS Secure Storage
SQLite             Windows DPAPI /
                   Credential Manager
```

后续：

```text
             ┌───────────────┐
             │ MCP Server    │
             └───────┬───────┘
                     │
                     ▼
                Policy Engine
                     │
      ┌──────────────┴──────────────┐
      ▼                             ▼
Metadata Access               Secret Operation
No plaintext                  User approved
```

---

# 7. 技术选型建议

## 7.1 Desktop

推荐：

- Tauri 2
- React
- TypeScript
- Vite

原因：

- Windows 桌面应用。
- 体积比 Electron 小。
- Rust 后端适合处理安全敏感逻辑。
- 前后端边界明确。
- 文件系统、窗口、系统托盘、原生 Dialog 支持好。
- 未来方便做 CLI 共用 Rust Core。

不建议：

- 纯 Electron + Node 直接处理 Secret 明文。
- Python 桌面端作为长期商业产品核心。

Townrain/API-Key-Manager 的 pywebview 可作为交互参考，但 SecretHub 建议采用 Tauri + Rust 重构长期架构。

---

## 7.2 Storage

推荐：

- SQLite
- SQLCipher 可作为后续增强

但不要单纯依靠数据库加密。

Secret Value 应进行字段级加密。

---

## 7.3 Encryption

建议：

```text
Master Encryption Key
↓
Windows DPAPI / Credential Manager 封装
↓
随机 Data Encryption Key
↓
AES-256-GCM
↓
加密 Secret Value
```

每条 Secret：

- 独立随机 nonce。
- Authenticated Encryption。
- 不重复 nonce。
- Secret 值与普通 Metadata 分开。

禁止：

- Key hardcode。
- 将 Master Key 放进 `.env`。
- Base64 当作加密。
- 自己设计密码学算法。

---

# 8. 数据模型

## 8.1 Secret

```ts
Secret {
  id: UUID

  name: string
  envKey: string

  providerId?: string
  category: SecretCategory

  encryptedValue: bytes

  valueType:
    | "api_key"
    | "token"
    | "password"
    | "url"
    | "id"
    | "ssh_key"
    | "certificate"
    | "webhook"
    | "license"
    | "text"

  description?: string
  notes?: string

  tags: string[]

  scope:
    | "global"
    | "project"
    | "profile"

  maskedPreview: string

  status:
    | "unknown"
    | "valid"
    | "invalid"
    | "expired"
    | "unchecked"

  createdAt: datetime
  updatedAt: datetime
  lastValidatedAt?: datetime
  lastUsedAt?: datetime
  lastRotatedAt?: datetime

  favorite: boolean
  archived: boolean
}
```

---

## 8.2 Provider

```ts
Provider {
  id: string
  name: string
  displayName: string

  category:
    | "ai"
    | "database"
    | "cloud"
    | "git"
    | "payment"
    | "storage"
    | "deployment"
    | "other"

  websiteUrl?: string
  docsUrl?: string

  defaultEnvKeys: string[]

  keyPatterns?: Pattern[]

  validationAdapter?: string

  icon?: string
}
```

示例：

```json
{
  "id": "openai",
  "displayName": "OpenAI",
  "defaultEnvKeys": [
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_ORG_ID",
    "OPENAI_PROJECT_ID"
  ]
}
```

---

## 8.3 Secret Group

支持一个 Provider 下多个字段：

```text
OpenAI 主账号
├── OPENAI_API_KEY
├── OPENAI_BASE_URL
├── OPENAI_ORG_ID
└── OPENAI_PROJECT_ID
```

因此 UI 不应只支持单 Key 卡片。

---

## 8.4 Profile

```ts
Profile {
  id: UUID
  name: string
  description?: string
  secretIds: UUID[]
  tags: string[]
  createdAt: datetime
  updatedAt: datetime
}
```

示例：

### 标准 AI 项目

```text
OPENAI_API_KEY
ANTHROPIC_API_KEY
GEMINI_API_KEY
OPENROUTER_API_KEY
```

### Supabase Web App

```text
SUPABASE_URL
SUPABASE_ANON_KEY
SUPABASE_SERVICE_ROLE_KEY
DATABASE_URL
```

---

## 8.5 Project

```ts
Project {
  id: UUID
  name: string
  path: string

  secretIds: UUID[]
  profileIds: UUID[]

  envFilename: string

  exportMode:
    | "plain_env"
    | "encrypted_env"
    | "runtime_injection"

  createdAt: datetime
  updatedAt: datetime
}
```

---

# 9. V1 功能需求

# 9.1 Vault 首页

首页需要展示：

- Secret 总数。
- Provider 数量。
- 已验证数量。
- 无效数量。
- 最近使用。
- 收藏。
- 最近项目。
- Profile。

主区域推荐布局：

```text
左侧：
All Secrets
Favorites
Recent
Providers
Profiles
Projects
Tags
Settings

中间：
Secret 列表

右侧：
详情 / 编辑 / Export Panel
```

---

# 9.2 添加 Secret

点击：

`+ Add Secret`

支持：

### 模式 A：选择 Provider

例如：

- OpenAI
- Anthropic
- Gemini
- DeepSeek
- xAI
- OpenRouter
- Supabase
- GitHub
- Cloudflare

选择 OpenAI 后自动生成字段：

```text
OPENAI_API_KEY
OPENAI_BASE_URL
OPENAI_ORG_ID
OPENAI_PROJECT_ID
```

用户可只填写需要项。

### 模式 B：Custom Secret

字段：

- Name
- Env Key
- Value
- Type
- Category
- Tags
- Description
- Notes

---

# 9.3 Provider 自动识别

用户直接粘贴：

```text
sk-ant-...
```

系统尝试识别：

```text
可能 Provider：Anthropic
```

用户确认后自动填写：

```text
Provider = Anthropic
Env Key = ANTHROPIC_API_KEY
```

自动识别只能作为辅助，不得在低置信度时自动写死 Provider。

---

# 9.4 Secret Masking

列表默认：

```text
OPENAI_API_KEY
sk-proj-•••••••••••7Km2
```

只有用户明确点击 Reveal 才显示。

Reveal：

- 显示 15 秒后自动恢复 Mask。
- 应支持立即隐藏。
- 不在系统日志中记录。
- 尽量避免进入剪贴板历史。

---

# 9.5 Copy Secret

点击 Copy：

- 复制真实 Value。
- 20~60 秒自动清除剪贴板，时间可设置。
- 如果剪贴板内容已被用户修改，不应误清除新内容。
- Audit Log 只记录：

```text
Copied OPENAI_API_KEY
```

不记录 Value。

---

# 9.6 搜索

支持搜索：

- Name
- Env Key
- Provider
- Category
- Tag
- Description

不能用明文 Secret Value 做全文索引。

---

# 9.7 Filter

筛选：

- Provider。
- Type。
- Tag。
- Valid / Invalid。
- Favorite。
- Recently Used。
- Global / Project。
- Archived。

---

# 9.8 Secret Validation

如果 Provider 有 Validation Adapter：

点击：

`Check`

系统：

1. 从 Vault 解密。
2. 在本地进程内使用。
3. 发起 Provider 验证请求。
4. 返回：
   - Valid
   - Invalid
   - Unauthorized
   - Rate Limited
   - Network Error
5. 清理内存。
6. UI 只显示状态。

需要：

- 单个验证。
- 批量验证。
- Provider 批量验证。

避免验证行为造成明显费用。

优先使用低成本 Endpoint。

---

# 9.9 Export `.env`

这是 V1 最重要功能。

用户流程：

```text
Secret 列表
↓
勾选多个 Secret
↓
点击 Export
↓
选择项目目录
↓
选择输出类型
↓
检查冲突
↓
确认
↓
生成 .env
```

例如：

```text
☑ OPENAI_API_KEY
☑ GEMINI_API_KEY
☑ SUPABASE_URL
☑ SUPABASE_ANON_KEY
```

选择：

```text
D:\Projects\CueCut
```

生成：

```text
D:\Projects\CueCut\.env
```

---

# 9.10 Export Dialog

UI：

```text
Export Secrets

Selected: 4

Target Folder:
D:\Projects\CueCut
[Browse]

Filename:
.env

Options:
☑ Create .env
☑ Create .env.example
☑ Ensure .gitignore contains .env
☐ Backup existing .env

Conflict:
● Merge
○ Replace
○ Cancel on conflict

[Preview] [Export]
```

---

# 9.11 `.env` Merge

如果 `.env` 已存在：

例如：

```env
PORT=3000
OPENAI_API_KEY=old
```

SecretHub 要添加：

```env
OPENAI_API_KEY=new
GEMINI_API_KEY=xxx
```

必须弹出 Diff：

```diff
 PORT=3000
-OPENAI_API_KEY=old
+OPENAI_API_KEY=••••••••
+GEMINI_API_KEY=••••••••
```

UI 不显示完整 Secret Value。

用户可选择：

- Replace conflicted only
- Skip conflicts
- Replace all
- Cancel

---

# 9.12 `.env.example`

生成：

```env
OPENAI_API_KEY=
GEMINI_API_KEY=
SUPABASE_URL=
SUPABASE_ANON_KEY=
```

允许可选：

```env
OPENAI_API_KEY=<required>
```

---

# 9.13 `.gitignore` 检查

Export 前检查项目根目录：

```text
.gitignore
```

如果 `.env` 未被 ignore：

弹窗：

```text
检测到 .env 尚未加入 .gitignore。

[自动添加]
[继续导出]
[取消]
```

默认推荐：

`自动添加`

可以添加：

```gitignore
.env
.env.*
!.env.example
```

但必须避免破坏现有 `.gitignore`。

---

# 9.14 Profile

用户可以将已选 Secret 保存为 Profile：

```text
Save as Profile
```

例如：

`AI Standard`

下一次：

```text
Profiles
↓
AI Standard
↓
Apply to Project
```

直接进入 Export 流程。

---

# 9.15 Project History

记录：

```text
CueCut
D:\Projects\CueCut

Last export:
2026-09-13

Secrets:
OPENAI_API_KEY
GEMINI_API_KEY
SUPABASE_URL
```

只记录 Secret ID / Env Key，不记录明文值。

---

# 9.16 Audit Log

记录：

- Secret created。
- Secret edited。
- Secret deleted。
- Secret revealed。
- Secret copied。
- Secret validated。
- Profile applied。
- `.env` exported。
- Runtime injected。
- MCP requested。

示例：

```text
2026-09-13 08:30
Exported 4 secrets to D:\Projects\CueCut\.env
```

绝对禁止：

```text
OPENAI_API_KEY=sk-...
```

---

# 10. V2 — MCP / AI Integration

MCP Server 必须默认采用：

> Metadata first, plaintext never.

---

## 10.1 AI 可查看内容

AI 可以读取：

```json
{
  "id": "secret_001",
  "name": "OpenAI Main",
  "envKey": "OPENAI_API_KEY",
  "provider": "OpenAI",
  "status": "valid",
  "tags": ["AI", "LLM"],
  "hasValue": true
}
```

不能读取：

```json
{
  "value": "sk-proj-..."
}
```

---

# 10.2 MCP Tools

V2 建议：

```text
list_secret_catalog
search_secrets
get_secret_metadata
list_profiles
list_projects
recommend_secrets_for_project
prepare_project_env
validate_secret
apply_profile
```

---

## 10.3 list_secret_catalog

返回：

- Secret ID。
- Name。
- Provider。
- Env Key。
- Tags。
- Status。
- HasValue。

不得返回 Value。

---

## 10.4 recommend_secrets_for_project

输入：

```json
{
  "stack": [
    "Next.js",
    "Supabase",
    "OpenAI"
  ]
}
```

返回：

```json
{
  "recommended": [
    "OPENAI_API_KEY",
    "SUPABASE_URL",
    "SUPABASE_ANON_KEY"
  ],
  "available": [
    "OPENAI_API_KEY",
    "SUPABASE_URL"
  ],
  "missing": [
    "SUPABASE_ANON_KEY"
  ]
}
```

---

## 10.5 prepare_project_env

输入：

```json
{
  "projectPath": "D:\\Projects\\CueCut",
  "secretIds": [
    "secret_001",
    "secret_002"
  ]
}
```

必须经过 Policy Engine。

策略：

### Safe Mode

每次写入 `.env` 都要求用户确认。

### Trusted Project

用户可以设置：

```text
Always allow SecretHub to prepare env for CueCut
```

但 Trusted 权限只绑定：

- 指定项目路径。
- 指定 Secret。
- 指定操作类型。

---

# 10.6 禁止 MCP Tool

绝对不要提供：

```text
dump_all_secrets
get_all_secret_values
show_master_key
export_everything_plaintext
```

默认 MCP 不能读取 Secret Value。

---

# 11. V3 — Runtime Injection

目标：

最终不必把 Secret 写到磁盘。

CLI：

```bash
secrethub run -- npm run dev
```

或者：

```bash
secrethub run -- codex
```

系统：

```text
Encrypted Vault
↓
解密 Secret
↓
构造子进程 env
↓
启动 Child Process
↓
Secret 仅存在进程环境
↓
Process Exit
↓
释放内存
```

---

# 11.1 Project Config

可引入：

```text
.secrethub.toml
```

示例：

```toml
project = "cuecut"

keys = [
  "OPENAI_API_KEY",
  "GEMINI_API_KEY",
  "SUPABASE_URL"
]

inherit_global = false
```

该文件只包含 Secret 名称，不包含 Secret Value，因此允许进入 Git。

---

# 11.2 Agent Proxy

V3.5 可参考 rapg：

```text
Real API Key
↓
SecretHub Local Proxy
↓
Short-lived localhost token
↓
AI Agent
```

这样 AI Agent 即使读取自己的环境变量，也无法拿到真正的长期 API Key。

---

# 12. V4 — AI Autonomous Project Setup

最终目标：

用户：

```text
我要开发一个 AI 图片生成网站
```

项目 Agent：

```text
技术栈：

Next.js
Supabase
OpenAI Image
Cloudflare
```

调用：

```text
recommend_secrets_for_project
```

SecretHub 返回：

```text
OPENAI_API_KEY       available
SUPABASE_URL         available
SUPABASE_ANON_KEY    available
CLOUDFLARE_API_TOKEN missing
```

Agent：

```text
项目需要 4 个凭据。
其中 3 个已存在，1 个缺失。

是否配置已有凭据？
```

用户确认。

SecretHub：

```text
prepare_project_env
```

配置完毕。

Codex 开始真正开发。

---

# 13. UI / UX 设计要求

本项目必须生成高保真可交互原型。

设计参考库：

https://github.com/VoltAgent/awesome-design-md

不要机械复制固定 Design。

应根据：

- Developer Tool
- Security Tool
- Desktop Utility
- AI Tool

选择合适 Design System。

---

# 13.1 视觉方向

关键词：

- Local-first
- Professional
- Developer Tool
- Clean
- Secure
- Dense but readable
- Dark / Light
- Windows native feeling

避免：

- Cyberpunk 过度视觉化。
- 巨大卡片。
- 过多渐变。
- Dashboard SaaS 化。
- 过度动画。

目标体验参考：

```text
1Password
Raycast
Linear
VS Code
GitHub Desktop
Supabase Dashboard
```

但不得复制其受保护 UI 素材。

---

# 13.2 主界面

建议：

```text
┌─────────────────────────────────────────────────────┐
│ SecretHub                           Search      + Add │
├────────────┬──────────────────────┬─────────────────┤
│ All        │ Secrets              │ Details         │
│ Favorites  │                      │                 │
│ Recent     │ OpenAI Main          │ OPENAI_API_KEY  │
│            │ Anthropic Main       │ ●●●●●●●●Km2     │
│ Providers  │ Gemini               │                 │
│ Profiles   │ Supabase CueCut      │ Provider OpenAI │
│ Projects   │                      │ Status Valid    │
│ Tags       │                      │                 │
│            │                      │ [Copy] [Reveal] │
│ Settings   │                      │ [Validate]      │
└────────────┴──────────────────────┴─────────────────┘
```

---

# 13.3 Multi Select

列表增加 Checkbox。

选中后底部出现：

```text
4 Selected

[Export]
[Add to Profile]
[Validate]
[Archive]
```

---

# 13.4 Security UX

Reveal：

```text
Click Reveal
↓
Confirm biometric / Windows Hello（后续）
↓
15 seconds visible
↓
Auto mask
```

V1 Windows Hello 可作为可选增强，不作为阻塞项。

---

# 14. Settings

设置页面：

## General

- Theme。
- Language。
- Default export filename。
- Default export mode。
- Clipboard timeout。

## Security

- Auto lock。
- Vault timeout。
- Require master auth before Reveal。
- Require auth before Export。
- Clipboard clearing。
- Audit retention。

## AI / MCP

V2：

- MCP enabled。
- Metadata access。
- Export permission。
- Trusted projects。
- AI confirmation policy。

## Provider

- Enable provider。
- Validation timeout。
- Custom provider。

---

# 15. Custom Provider

允许新增：

```yaml
name: My LLM
id: my_llm

defaultEnvKeys:
  - MY_LLM_API_KEY
  - MY_LLM_BASE_URL

patterns:
  - prefix: my-
```

V1 可仅 UI 添加简单 Provider。

复杂 Validator Adapter V1.5。

---

# 16. Security Requirements

这是项目最高优先级。

Priority：

```text
P0 Security
P1 Correctness
P2 UX
P3 Convenience
```

---

# 16.1 Secret 不得进入日志

实现统一日志 Sanitizer。

以下字段自动 Redact：

```text
api_key
token
password
secret
authorization
cookie
private_key
```

---

# 16.2 Crash Report

Crash Report：

不得上传：

- Vault。
- `.env`。
- Secret Value。
- Clipboard。
- MCP Payload 中的敏感字段。

V1 默认关闭网络遥测。

---

# 16.3 Memory Hygiene

Rust 层：

尽量：

- Secret 生命周期尽可能短。
- 解密后立即使用。
- 不复制到不必要 String。
- 不长期 Cache 明文 Secret。
- 使用 Zeroizing / secrecy crate 等成熟方案。

---

# 16.4 Clipboard

Copy：

记录：

```text
clipboardToken = random
valueHash = hash(secret)
```

到时间时：

只有 Clipboard 仍然等于原 Secret 时才清除。

避免用户后来复制其他内容却被 SecretHub 清空。

---

# 16.5 File Permission

Vault 文件：

- 当前用户可读。
- 避免 Everyone 权限。
- Windows ACL 做合理限制。

---

# 16.6 Path Validation

用户选择 Export 目录时：

- Canonicalize。
- 防 Path Traversal。
- 不允许通过文件名写到用户未选择目录。
- Symlink / Junction 做安全处理。

---

# 16.7 Backup

V1 支持：

Export Vault Backup：

必须是加密格式。

禁止默认导出 plaintext JSON。

---

# 17. Git 安全

生成 `.env` 时：

必须执行：

1. 查找 `.git`。
2. 检查 `.gitignore`。
3. 提示 `.env` ignore。
4. 如果 `.env` 已被 Git tracked：

必须显示高危警告：

```text
WARNING

.env is already tracked by Git.

SecretHub will not export secrets until you confirm remediation.
```

未来可以提供：

```bash
git rm --cached .env
```

但 V1 默认只给建议，不自动执行 Git 删除命令。

---

# 18. API Key 自动检测

可以复用 Townrain/API-Key-Manager Provider Registry 思路。

检测优先级：

```text
Unique Prefix
↓
Pattern
↓
User confirmation
↓
Optional provider validation
```

避免：

为了识别 Provider 自动把 Key 发给几十个平台。

默认不能对未知 Key 做全网并发探测。

---

# 19. Provider Validation 安全限制

Provider Adapter 必须：

- 明确固定官方域名。
- 阻止任意内网 IP。
- 防 SSRF。
- Timeout。
- Limited redirects。
- 不把 Response Body 中敏感数据写日志。

---

# 20. 推荐目录结构

```text
secrethub/
├─ apps/
│  ├─ desktop/
│  │  ├─ src/
│  │  ├─ src-tauri/
│  │  └─ package.json
│  │
│  └─ mcp/
│     └─ src/
│
├─ crates/
│  ├─ core/
│  ├─ vault/
│  ├─ crypto/
│  ├─ storage/
│  ├─ providers/
│  ├─ exporter/
│  ├─ profiles/
│  ├─ projects/
│  ├─ audit/
│  ├─ policy/
│  └─ redaction/
│
├─ packages/
│  ├─ ui/
│  ├─ shared-types/
│  └─ provider-definitions/
│
├─ docs/
│  ├─ architecture/
│  ├─ security/
│  ├─ prd/
│  └─ adr/
│
├─ tests/
│  ├─ integration/
│  ├─ security/
│  └─ fixtures/
│
├─ LICENSE
├─ README.md
└─ SECURITY.md
```

---

# 21. Core Interfaces

建议核心接口：

```rust
trait Vault {
    fn create_secret(...)
    fn update_secret(...)
    fn delete_secret(...)
    fn get_metadata(...)
    fn decrypt_secret(...)
}
```

```rust
trait ProviderValidator {
    fn validate(secret: SecretMaterial) -> ValidationResult;
}
```

```rust
trait Exporter {
    fn preview(...)
    fn export(...)
}
```

```rust
trait PolicyEngine {
    fn authorize(operation, project, secret_ids) -> PolicyDecision;
}
```

---

# 22. `.env` Writer 要求

必须正确处理：

- 空格。
- `#`。
- 引号。
- 多行值。
- URL。
- JSON。
- Private Key。
- Windows 换行。
- Unicode。

不要使用：

```text
key + "=" + value
```

这种简单拼接实现完整 dotenv。

必须采用成熟 dotenv escaping 逻辑。

---

# 23. Secret Import

V1.5：

支持：

- `.env`
- JSON

导入 `.env`：

```env
OPENAI_API_KEY=...
SUPABASE_URL=...
```

SecretHub：

- 解析 Env Key。
- 尝试识别 Provider。
- Preview。
- 用户确认后导入。

---

# 24. 迁移 / 导出

支持：

### Secure Backup

```text
SecretHub Vault Backup
.secrethub-backup
```

必须加密。

### Plaintext Export

如果以后支持：

必须：

- 二次确认。
- 高风险提示。
- 明确选择目录。
- 记录审计。
- 默认关闭。

---

# 25. 错误处理

例如 Export 失败：

```text
Could not write .env

Reason:
Permission denied

No secret was written partially.

[Retry]
[Choose Another Folder]
```

写文件建议采用 Atomic Write：

```text
.env.tmp
↓
fsync
↓
rename
```

避免生成半截 `.env`。

---

# 26. Auto Lock

设置：

```text
Lock after:
5 min
15 min
30 min
Never
```

锁定后：

- UI 可显示 Metadata。
- Secret Reveal 不可用。
- Export 不可用。
- Copy 不可用。
- Runtime Injection 不可用。

---

# 27. Master Unlock

V1：

- Master Password。

V1.5：

- Windows Hello。
- Windows Credential Manager。

不允许：

“忘记 Master Password 后通过服务器找回 Vault”。

Local-first 模式下应明确：

无法恢复 Master Key 就无法解密 Secret。

---

# 28. 开发阶段划分

# Phase 0 — Architecture

输出：

- `ARCHITECTURE.md`
- `SECURITY_MODEL.md`
- `OPEN_SOURCE_REUSE.md`
- `DATA_MODEL.md`
- `TASK_LEDGER.md`

---

# Phase 1 — Core Vault

实现：

- SQLite。
- Encryption。
- CRUD。
- Mask。
- Search Metadata。

验收：

Secret 保存后直接打开 SQLite 文件，不应找到明文。

---

# Phase 2 — Desktop MVP

实现：

- Add Secret。
- List。
- Detail。
- Copy。
- Reveal。
- Delete。
- Search。

---

# Phase 3 — Export

实现：

- Multi select。
- Folder Picker。
- `.env` preview。
- `.env` export。
- `.env.example`。
- `.gitignore` check。
- Conflict diff。

这是 V1 Release Gate。

---

# Phase 4 — Provider

实现：

- Provider Registry。
- Auto detection。
- OpenAI。
- Anthropic。
- Gemini。
- DeepSeek。
- xAI。
- OpenRouter。
- GitHub。
- Supabase。
- Cloudflare。

不要一开始就做 45+。

先做高频 Provider。

---

# Phase 5 — Profile / Project

实现：

- Profile。
- Recent Project。
- Apply Profile。
- Project history。

---

# Phase 6 — Security Hardening

测试：

- DB plaintext scan。
- Log scan。
- Crash log scan。
- Clipboard。
- `.gitignore`。
- Path traversal。
- SSRF。
- Symlink。
- Concurrent write。
- Master Password brute-force protection。

---

# Phase 7 — MCP

进入 V2。

---

# 29. MVP 范围

MVP 必须只包含：

```text
Vault
Add Secret
Edit Secret
Delete Secret
Mask / Reveal
Copy
Search
Tags
Provider
Multi Select
Folder Picker
Export .env
.env.example
.gitignore check
Profile
Audit Log
```

MVP 不包含：

```text
Cloud Sync
Team
RBAC
Rotation
Runtime Proxy
MCP
CI/CD
Mobile
Browser Extension
```

---

# 30. 测试要求

## Unit Test

覆盖：

- Crypto。
- Env Parser。
- Env Writer。
- Merge。
- Conflict Detection。
- Mask。
- Metadata Search。
- Policy。

---

## Integration Test

完整流程：

```text
Create Secret
↓
Restart App
↓
Unlock
↓
Select Secret
↓
Export
↓
Verify .env
```

---

## Security Test

扫描：

```text
SQLite
Log files
Temp files
Crash files
Audit files
```

确保不存在测试 Secret：

```text
TEST_SECRET_123456
```

---

# 31. E2E 测试

Playwright / Tauri compatible E2E。

关键路径：

### E2E-01

```text
首次启动
→ 设置 Master Password
→ 添加 OpenAI Key
→ 保存
→ 重启
→ Unlock
→ Secret 存在
```

### E2E-02

```text
选择 OpenAI
→ Export
→ Folder Picker
→ 生成 .env
```

### E2E-03

```text
.env 已存在
→ 冲突
→ Diff
→ Replace selected
```

### E2E-04

```text
.gitignore 未包含 .env
→ Warning
→ Auto Fix
```

### E2E-05

```text
Save Profile
→ Apply Profile
→ Export
```

---

# 32. 性能要求

V1：

- 5,000 Secret Metadata 列表可正常工作。
- 搜索反馈 < 200ms。
- App 启动 < 3 秒为目标。
- Unlock < 1 秒为目标。
- Export < 500ms，不含网络 Validation。

---

# 33. 隐私要求

默认：

```text
No Cloud
No Telemetry
No Account
No Analytics
```

所有数据：

```text
Local Device Only
```

后续如果增加 Crash Reporting：

必须 Opt-in。

---

# 34. 可观测性

Log Level：

```text
ERROR
WARN
INFO
DEBUG
```

生产版本：

默认 INFO。

Logger 必须经过 Secret Redaction。

---

# 35. 商业化考虑

虽然 V1 为个人工具，但架构不能阻塞未来：

- Pro Version。
- Team。
- Cloud Sync。
- Enterprise。
- Cross-device。
- Mobile companion。
- Shared Vault。

因此：

UI 与 Core 分离。

Storage Interface 抽象。

Crypto Interface 抽象。

Policy Engine 独立。

---

# 36. Open Source Compliance

Codex 在复用任何第三方代码前必须生成：

```text
THIRD_PARTY_NOTICES.md
```

记录：

- 项目名。
- Repository。
- License。
- 复用文件。
- Modification。
- Copyright。

不得复制：

- AGPL 代码。
- Enterprise-only 代码。
- License 不明确代码。
- 商业限制代码。

---

# 37. Definition of Done — V1

只有全部满足才算完成：

- [ ] Windows 可安装。
- [ ] Master Unlock。
- [ ] Secret 本地加密。
- [ ] DB 无 Secret 明文。
- [ ] 添加 Secret。
- [ ] 编辑 Secret。
- [ ] 删除 Secret。
- [ ] Search。
- [ ] Provider。
- [ ] Tags。
- [ ] Mask。
- [ ] Reveal。
- [ ] Copy。
- [ ] Multi Select。
- [ ] Folder Picker。
- [ ] `.env` Preview。
- [ ] `.env` Export。
- [ ] `.env.example`。
- [ ] Conflict Diff。
- [ ] `.gitignore` 检查。
- [ ] Profile。
- [ ] Project History。
- [ ] Audit Log。
- [ ] Audit 无 Secret 明文。
- [ ] Log 无 Secret 明文。
- [ ] 自动锁定。
- [ ] Backup 为加密格式。
- [ ] Unit Tests。
- [ ] Integration Tests。
- [ ] Security Tests。
- [ ] E2E Tests。
- [ ] README。
- [ ] SECURITY.md。
- [ ] THIRD_PARTY_NOTICES.md。
- [ ] 高保真可交互原型。
- [ ] 与实际实现保持一致。

---

# 38. Codex 开发规则

1. 不允许跳过安全模型直接写 UI。
2. 不允许把 Secret 放 Redux / LocalStorage。
3. 不允许在浏览器 DevTools 可直接获取 Secret。
4. 不允许真实 Secret 出现在测试 Fixture。
5. 不允许真实 Secret 写入 Git。
6. 不允许提交 `.env`。
7. 不允许在 Console 打印 Secret。
8. 不允许把 Secret 发送给 AI。
9. 不允许为了 Debug 临时关闭加密后忘记恢复。
10. 每个涉及 Secret 的 PR 都需要安全检查。
11. 每完成一个阶段更新 `TASK_LEDGER.md`。
12. 每个关键架构决策写 ADR。
13. 每次 UI 改动更新交互原型。
14. 发现 PRD 与安全要求冲突时，安全要求优先。
15. 遇到需要真实第三方 API Key 测试时，必须请求用户提供测试凭据或让用户在本地手动填入，禁止索要用户在聊天中粘贴真实 Key。

---

# 39. 第一阶段建议任务台账

```text
P0
├─ R01 Architecture
│  ├─ ADR Desktop framework
│  ├─ ADR Storage
│  ├─ ADR Crypto
│  └─ Threat Model
│
├─ R02 Core Security
│  ├─ Key derivation
│  ├─ DPAPI
│  ├─ AES-GCM
│  ├─ zeroize
│  └─ redaction
│
├─ R03 Storage
│  ├─ SQLite schema
│  ├─ migrations
│  └─ repository
│
├─ R04 Vault
│  ├─ CRUD
│  ├─ mask
│  ├─ reveal
│  └─ copy
│
├─ R05 Desktop
│  ├─ shell
│  ├─ secret list
│  ├─ detail
│  ├─ add/edit
│  └─ search
│
├─ R06 Export
│  ├─ selection
│  ├─ folder picker
│  ├─ env writer
│  ├─ preview
│  ├─ merge
│  ├─ conflict
│  └─ gitignore
│
├─ R07 Profile
│  ├─ create
│  ├─ edit
│  └─ apply
│
├─ R08 Provider
│  ├─ registry
│  ├─ detection
│  └─ validation adapters
│
└─ R09 QA
   ├─ unit
   ├─ integration
   ├─ e2e
   └─ security
```

---

# 40. 最小垂直切片

Codex 必须优先完成：

```text
启动 App
↓
创建 Master Password
↓
Add Secret
↓
AES-GCM 加密
↓
SQLite 保存
↓
重启 App
↓
Unlock
↓
看到 Secret Metadata
↓
勾选 Secret
↓
选择 Folder
↓
Preview
↓
Export .env
```

完成这个闭环以前：

不要：

- 开始 MCP。
- 开始 Agent Proxy。
- 做大量 Provider。
- 做 Team。
- 做 Cloud。

---

# 41. 推荐 V1 首批 Provider

AI：

1. OpenAI
2. Anthropic
3. Google Gemini
4. DeepSeek
5. xAI
6. OpenRouter

Developer：

7. GitHub
8. Supabase
9. Cloudflare

Custom：

10. Generic API Key

其他 Provider 后续通过 Registry 增加。

---

# 42. 未来能力预留

数据模型应预留：

- Secret Version。
- Expiration。
- Rotation。
- Usage Policy。
- Project Trust。
- Agent Trust。
- Temporary Credential。
- Service Account。
- Dynamic Secret。
- Shared Vault。

但 V1 UI 不必暴露。

---

# 43. 用户最终理想工作流

### 手动模式

```text
打开 SecretHub
↓
选择 AI Standard
↓
选择项目文件夹
↓
Export
↓
完成
```

目标时间：

< 10 秒。

---

### AI 模式

```text
用户：
“开发一个 AI 图片工具”

AI：
分析技术栈

AI：
调用 SecretHub Catalog

SecretHub：
列出可用资源，不暴露 Value

AI：
规划需要：
OPENAI_API_KEY
SUPABASE_URL

用户：
确认

SecretHub：
写入项目

AI：
继续开发
```

用户不再需要：

```text
打开 OpenAI
登录
进入 API 页面
找 Key
复制
打开项目
编辑 .env
```

---

# 44. 产品成功指标

V1：

### 核心指标 1

从：

```text
新项目
→
配置 5 个 Secret
```

原本：

5~15 分钟。

目标：

< 30 秒。

### 核心指标 2

Profile：

目标：

< 10 秒。

### 核心指标 3

安全：

- 0 明文 Secret 出现在 Log。
- 0 明文 Secret 出现在 DB。
- 0 Secret 被意外 Commit。

---

# 45. 最终产品原则

SecretHub 必须坚持：

```text
Store once.
Use everywhere.

Metadata for AI.
Secrets for runtime.

Local first.
Secure by default.

Project aware.
Agent safe.
```

---

# 46. Codex 启动提示

开始开发时，Codex 第一条执行指令应为：

```text
请完整阅读当前 PRD。

先不要写功能代码。

第一阶段请完成：

1. 对 PRD 做需求完整性检查。
2. 根据 Windows Local-First Desktop App 的目标确认技术栈。
3. 输出 ARCHITECTURE.md。
4. 输出 SECURITY_MODEL.md。
5. 输出 OPEN_SOURCE_REUSE.md。
6. 输出 DATA_MODEL.md。
7. 输出 TASK_LEDGER.md。
8. 根据产品类型选择适合的 DESIGN.md / Design System。
9. 生成完整高保真可交互原型。
10. 拆分 V1 最小垂直切片。

随后开始开发最小垂直切片：

Master Unlock
→
Create Secret
→
Encrypted Storage
→
Read Metadata
→
Select Secret
→
Choose Project Folder
→
Preview .env
→
Export .env

在该闭环通过真实测试之前，不要进入 MCP、Runtime Injection、Cloud、Team 等后续能力。
```

---

# 47. 交付物

V1 最终至少包含：

```text
SecretHub.exe / Installer

README.md
SECURITY.md
ARCHITECTURE.md
DATA_MODEL.md
OPEN_SOURCE_REUSE.md
THIRD_PARTY_NOTICES.md
TASK_LEDGER.md

Interactive Prototype

Source Code
Unit Tests
Integration Tests
Security Tests
E2E Tests
```

---

# 48. 结论

SecretHub 的 V1 应保持非常聚焦：

> 安全地保存开发凭据，并让用户在任何新项目中用几秒钟生成正确的环境配置。

不要把 V1 做成企业级 Vault。

真正形成长期竞争力的是：

1. Local-first。
2. Developer-first。
3. AI-safe。
4. Project-aware。
5. 一键 `.env`。
6. Profile。
7. MCP Metadata Catalog。
8. Runtime Injection。
9. 与 AI 项目台账联动。

开发顺序必须严格遵循：

```text
Secure Vault
→
Project Export
→
Profiles
→
Provider Intelligence
→
MCP
→
Runtime Injection
→
Autonomous Project Setup
```

这是 SecretHub 的正确演进路径。
