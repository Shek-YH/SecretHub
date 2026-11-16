# AI 整理导入设计（自动写入，带护栏）

> 状态：已确认（2026-11-10）。用户确认：用户资料格式不统一且可能有错，由 AI 根据“打码结构”整理并**自动写入**，不设人工审核；资料错误由后续调用/连接失败暴露，AI 再请用户补充。
> 前置：配置组与导入助手（提交 47d0b85–37835d6）。本方案是 `secrethub-import` 流程的新主路径；原有“规则解析 + 审核弹窗”保留作为手动导入。

## 0. 不变量

1. 机密值不进入 AI 上下文。AI 只看到打码视图，方案里只引用占位编号。
2. 自动写入不需要审核，但有两条程序护栏（§3），保证不会泄露、不会破坏已有数据。
3. 真实值的解析与写入只在 Desktop 的 Rust 进程内完成。

## 1. 打码视图（Rust 生成）

- **MCP 工具**：`secrethub_import_inspect({ paths })` 写入队列 `inspect-<id>`。Desktop 扫描（沿用 scan.rs 的范围限制）后生成打码视图，写到结果文件；`secrethub_get_import_inspection({ request_id })` 读取。扫描本身不需要解锁保险库。
- **视图内容**：按文件列出每一行。
  - KV 行：变量名，加值的打码表示；
  - 自由文字行：逐词打码；
  - 私钥文件：整体替换为一个占位。
- **哪些原样显示**：IPv4/IPv6、`http(s)://host[:port]` 网址（去掉其中的 userinfo、query 里的 token/key/secret 参数）、1–5 位端口号、常见云区域名、邮箱或用户名样式的短词（前面不能紧跟“密码 / password / pwd / pass / 口令 / 密钥 / key / token / secret”）。
- **哪些替换为占位** `«S<n>:<描述>»`（描述如“32 位随机串”“多行私钥”“12 位”）：
  - 紧跟上述关键词之后的值；
  - 熵高、长度 ≥ 12 的串；
  - 私钥块；
  - 名称按 group-templates.json 规则判为机密的变量值；
  - 拿不准的一律打码（保守）。
- **占位映射**（编号 → 真实值）只保存在 Desktop 内存中，最多 30 分钟，过期即失效。

## 2. AI 提交整理方案

- **MCP 工具**：`secrethub_apply_import_plan({ inspection_id, items })`，写入队列 `organize-<id>`。条目格式：

  ```jsonc
  { "type": "env_group" | "api_key" | "login" | "note",
    "name": "阿里宝塔服务器", "category": "server",
    "providerId": "qwen",                       // api_key 时
    "variables": [ { "name": "ALI_BT_SSH_HOST", "value": { "literal": "8.0.0.1" }, "secret": false,
                     "description": "阿里宝塔服务器 SSH 主机" },
                   { "name": "ALI_BT_SSH_PASSWORD", "value": { "token": "S3" }, "description": "SSH 登录密码" } ],
    "notes": "…（只能引用占位或视图中的明文）" }
  ```

- 变量值只能是 `{ token }`（引用占位）或 `{ literal }`。`literal` 必须是打码视图里出现过的明文片段，或由这些片段组成，例如把主机和端口拼成 URL；否则拒绝该变量。
- **`get_import_status`**：在原有返回之外，增加每个条目的结果（`created` / `added_vars` / `kept_both` / `skipped`）和被拒绝的变量（附原因），不含任何值。

## 3. 自动写入的护栏（Desktop，保险库解锁时自动执行，无弹窗）

1. **机密不可降级**：值来自 token 的变量一律以机密保存，忽略方案中的 `secret: false`。literal 值按方案的 `secret` 标记，缺省时按名称规则判断。
2. **只增不改不删**：
   - 已有同名配置组：只追加该组缺少的变量。同名变量值相同则跳过；值不同则不覆盖，把这些变量另存为新组“<名称>（AI 导入 YYYY-MM-DD）”。
   - 已有相同值的 API Key / 账号密码：跳过；值不同则新建一条（保留两份），不覆盖已有数据。
3. **批次与撤销**：每次写入生成一个批次 id，新增的条目和变量都记录所属批次。桌面端弹出通知“AI 导入了 N 条（新增组 x、补充变量 y、另存 z）”，附“查看”和“撤销此次导入”。撤销只删除本批次新增的内容。
4. 审计记录批次 id 和数量，不记录任何值。
5. 保险库锁定时请求保持 pending，横幅提示“解锁后自动写入”。

## 4. 变量说明与导出注释

- `group_variables` 加列 `description TEXT NOT NULL DEFAULT ''`（明文，AI 可见，不能含值）。AI 整理方案里的 `description` 写入此列；配置组表单里也可以编辑。
- 导出 `.env` 和 `.env.example` 时，每个配置组前加一行注释 `# <组名>（<分类中文名>）`，每个变量前加 `# <description>`（description 为空则不加）。API Key 前加 `# <服务商名> API Key`。只写注释，不写值。

## 5. Skill（更新 `skills/secrethub-import/SKILL.md`）

- **默认流程**：
  1. 调 `import_inspect` 获取打码视图；
  2. 按以下要求整理：
     - 一台服务器 / 一个服务一组；
     - 变量名统一大写、加该组前缀，避免多组同时导出时撞名；
     - 识别宝塔面板、SSH、数据库、对象存储等常见结构；
     - 每个变量写一句用途说明；
     - 指出疑点，如重复、像是填错的值；
  3. 调 `apply_import_plan`；
  4. 轮询状态，向用户简要汇报：新增了什么、哪些疑点、如何撤销。
- 绝不读取文件内容（沿用现有禁令）；方案里只出现占位编号和视图明文。
- 之后在项目中调用或连接失败时，提示用户“某某信息可能不对，请在 SecretHub 中更新或重新提供文件”，再重新整理。

## 6. AI 理解度测试

- 用 §1 的打码规则对设计 2026-11-09 §5 的假数据夹具生成视图，并准备一份“理想整理方案”的测试夹具；跑自动写入，再导出到临时项目，得到带注释的 `.env.example`。
- 派一个 Haiku 子代理，只给它这份 `.env.example`（变量名和注释），要求：
  1. 逐个说明变量用途；
  2. 写出用 SSH 连接“阿里宝塔服务器”的部署配置示例（只用变量名）。
- 主会话据此评估命名和注释是否让 AI 看得懂，并把结论写进 `docs/evidence/`。

## 7. 测试要点

- 打码规则：每类明文与机密的正反例；userinfo 和 query 中 token 的剥离；拿不准时打码。
- `literal` 合法性校验；token 过期；token 值强制为机密。
- 只增不改：同名组追加、值不同时另存、API Key 保留两份。
- 撤销只删除本批次内容。
- 导出注释格式；description 不含值。
- MCP 工具的 schema 和队列隔离；任何 MCP 响应中都不出现假机密值。
