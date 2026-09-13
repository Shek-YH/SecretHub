# SecretHub V1 Design

> 已依据 `SecretHub_Codex_PRD.md` 与用户追加要求确认：界面默认中文，支持中文/English 切换。

## Goal

从零构建 Windows 优先的 Local-First 开发凭据中心，先交付可工作的安全垂直切片，再完成 V1 的 CRUD、搜索、批量选择、项目导出、Profile、审计、自动锁定和加密备份。

## Architecture

SecretHub 使用 Tauri 2 桌面壳、React/TypeScript UI 和 Rust Core。UI 只读取脱敏后的 metadata；解密、校验、复制、Reveal、导出、备份与审计均由 Rust 命令处理。SQLite 保存 metadata 与加密 payload，Master Password 通过 Argon2id 派生密钥，payload 使用 AES-256-GCM 加密。

V1 的“高保真可交互原型”直接由桌面 UI 实现，不额外维护会漂移的静态副本。UI 状态包括首次设置、锁定、解锁、空状态、搜索、编辑、Reveal、批量导出预览、冲突警告、设置和语言切换。

## Modules

- `apps/desktop/src`: React shell、页面、i18n、UI state。
- `apps/desktop/src-tauri`: Tauri commands、native clipboard/dialog、应用生命周期。
- `crates/core`: 用例编排与 policy 边界。
- `crates/crypto`: Argon2id、AES-GCM、zeroize。
- `crates/storage`: SQLite schema、migration、repository。
- `crates/exporter`: dotenv writer、preview、冲突与 `.gitignore` 检查、atomic write。
- `crates/providers`: provider registry 与本地格式检测，V1 先覆盖高频 provider。
- `crates/profiles`: profile 与 project history。
- `crates/audit`: 脱敏审计事件。
- `crates/redaction`: 统一日志/错误/崩溃报告清洗。
- `tests`: integration/security/e2e fixtures，禁止真实凭据。

## Data flow

1. 首次启动创建 vault header 与 master-password verifier。
2. 用户解锁后 Rust 在内存中派生 vault key；UI 仅取得 metadata。
3. Add/Edit 将 secret value 在 Rust 侧加密后写入 SQLite。
4. Copy/Reveal/Export 由 Rust 侧短时解密并立即 zeroize；UI 不保存 plaintext。
5. 所有外部可见事件只包含 secret id、provider、env key、hash 或结果，不包含 value。

## Security boundary

- 不把 secret value 放入 Redux、LocalStorage、URL、console、日志、测试 fixture、快照或 ledger。
- Tauri command 返回值默认是 metadata；Reveal 使用 native dialog，Copy 使用 Rust/native clipboard。
- Export 只允许用户选定且通过 canonicalization 的目录，使用临时文件 + flush + rename。
- `.env` 已被 Git tracked 时 fail-closed；`.gitignore` 缺少 `.env` 时只在明确选择后追加规则。
- 不做未知 provider 全网探测；validator 只访问固定官方域名、限制 redirect、timeout 和内网地址。
- 默认关闭 telemetry；备份只导出加密 `.secrethub-backup`。

## Language policy

`zh-CN` 是默认且持久化的 locale。所有用户可见文案来自 typed translation dictionary；切换立即刷新 UI，不改变数据。English 使用 `en-US`，缺失 key 回退中文并在开发测试中报告。

## Testing strategy

- Rust unit tests：crypto、redaction、mask、dotenv escaping、merge/conflict、policy。
- Rust integration：创建 → 重启 → 解锁 → metadata → export，并扫描 SQLite/temp/log/audit 无 plaintext。
- UI tests：默认中文、语言切换、关键页面交互；真实 Tauri E2E 在 Windows 构建可用后运行。
- 真实集成 readiness：Synthetic fixtures 先行；真实 provider key 只允许用户在本地填入，不进入聊天或仓库。

## Decisions and limits

- V1 不实现 MCP、runtime injection、cloud/team/mobile；它们是后续阶段。
- 暂不引入 SQLCipher；应用层 payload encryption 先满足“直接打开 SQLite 无明文”的验收，并保留 storage interface 供后续替换。
- 不把 Windows Hello 作为首个阻塞项；Master Password 是 V1 必选，Hello 为 V1.5 增强。
