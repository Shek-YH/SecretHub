# 交接记录（HANDOFF）

> 推送前由 Claude 更新；另一台电脑打开 Claude 会话时会自动读到这里。

## 当前进度
- 分支 `claude/secrethub-export-autotodo-e27656`（基于 main `43fac4f`）：MCP 同名变量冲突改为**分批导出**。`secrethub_prepare_project_env` 遇到同名变量来自不同来源时，自动拆成多个计划（返回 `requestIds`），每个计划在 Desktop 单独确认。MCP 测试 155 项通过（仅 import-skill CRLF 用例失败，既有问题），tsc 通过。
- 已知缺口：普通 API Key 走 `secret_ids` 路径时没有做同名检查，同名 Key 会在同一个 `.env` 里重复写两行（dotenv 取最后一行）。待修。
- **已发布 0.1.1**（2026-11-17，main `9ae27d6`，PR #3 已合并）：**安全录入表单**。新 MCP 工具 `secrethub_request_input` / `secrethub_get_input_status`；Desktop 新模块 `secure_input.rs` + 前端 `SecureInput.tsx`，验证（登录接口 / 大模型 `/models`）通过才写库，走 AI 导入批次可撤销；新 skill `secrethub-secure-input`（「连接 AI 工具」现复制 6 个 skill）。测试：Rust 全过、前端 210、MCP 新增 24 条 + smoke；dev 模式端到端（锁定横幅 → 解锁自动弹表单 → 假密码 401 提示 → 取消得 cancelled）已跑通。
- 分支 `feat/simplify-entry-and-providers`（PR #2，基于 PR #1 的分支 `fix/p0-mcp-env-merge-provider-catalog`），全部测试通过（Rust 289、前端 200、MCP 268、smoke）：
  - 常用 10 家服务商、按类型录入、配置组导出、AI 整理导入（打码视图 + 护栏 + 批次撤销）、带注释导出；
  - **可分发安装包**：MCP 打成单文件 exe（Node SEA，sidecar `secrethub-mcp.exe`），用户无需装 Node；设置页「连接 AI 工具」一键注册 Claude Code / Claude Desktop / Cursor 的 MCP 并复制 5 个 skill；
  - **静默自动更新**（tauri-plugin-updater）：启动 3 秒后 + 每小时检查，静默下载，顶栏「更新」按钮，退出时静默安装；NSIS 当前用户安装、中文、安装前结束 sidecar 进程。
  - 更新地址：`https://updates.9aizhuan.com/secrethub/latest.json`（腾讯宝塔服务器 `tx-bt` 的静态站点 `/www/wwwroot/updates.9aizhuan.com`，每个产品一个子目录；DNSPod A 记录 + acme.sh Let's Encrypt 证书自动续期，已验证 HTTPS 200）。
- 使用说明 `docs/USAGE.md`（§6.1 发布安装包与自动更新）。

## 下一步
- **Mac 上开发 macOS 版**：先 `git fetch` 后切到 `claude/secrethub-export-autotodo-e27656`（或等合并后用 main），按 `docs/` 与本文件注意事项操作。Mac 版当前只有 sidecar 的 Windows 构建，需补 Mac 的 sidecar 与打包。
- 修复 MCP 普通 API Key 的同名检查缺口（`apps/mcp/src/plan.ts` 的 `precheckPlan`，对 `secret_ids` 中非配置组的 `envKey` 也 claim），并补测试。
- 0.1.1 已上线：把全局 CLAUDE.md 第 18 节改成调用 `secrethub_request_input`（由主会话处理），再停用 `F:\AIrules\secure-input` 临时方案。
- 本机 SecretHub 跑的是 `F:\CCPJ\SecretHub\target\release\secrethub-desktop.exe`（不是安装版，不会自动更新）；要用上新表单需在主目录拉 main 后重新构建，或在另一台机器装 0.1.1。
- 偶发失败：`organize_fixture::organize_fixture_exports_a_commented_example_without_values` 全量并行跑时偶尔失败、单跑和重跑都通过，待查。
- 真机试一次有效大模型 Key 的「获取模型列表」（单测用的是本地假服务）。
- 已发布 0.1.0（2026-11-16）、0.1.1（2026-11-17）：安装包固定下载链接 https://updates.9aizhuan.com/secrethub/SecretHub-setup-latest.exe（发布脚本每次自动更新这个软链接），已写入飞书「线上网站与管理后台」表格。
- 升级链路要连发两个版本，在**另一台干净机器或虚拟机**上装旧版再测升级；本机正在使用 SecretHub，不要在本机装安装包。
- 真机测「连接 AI 工具」按钮、按类型添加、AI 导入通知与撤销。
- 已知小问题：撤销导入后，在非 API Key 页面看不到“撤销成功”提示。
- 合并顺序：先合 PR #1，再合 PR #2（普通 merge，不要 squash）。

## 注意事项
- 新电脑先执行 `npm --prefix apps/desktop ci`、`npm --prefix apps/mcp ci`、`npm --prefix apps/mcp run build`；打包前再 `npm --prefix apps/mcp run build:sea`。
- **更新签名私钥** `C:\Users\Administrator\.tauri\secrethub-updater.key`（无密码，不进 git）：丢了老版本就收不到更新，需用户离线备份；在 Mac 上发版也要同一个文件。
- 在 PowerShell 手动 `tauri build` 会因空的 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 卡住，用发布脚本。
- 安装包未做代码签名（用户决定先不签），会有 SmartScreen 提示；SEA 注入后 node.exe 原签名失效，可能被杀软误报。
- `npm test`（apps/mcp）里 import-skill 的 front matter 用例在 Windows（autocrlf=true，CRLF 检出）下必失败，与功能无关；可用 `npx vitest run src` 并忽略这一条，或以后把用例的正则改成兼容 CRLF。
- Mac 版尚未打包（sidecar 目前只构建 x86_64-pc-windows-msvc）。
- 不要读取用户资料目录里的文件内容；导入一律走 secrethub-import skill。
- `catalog-public-key.txt` 仍是开发用公钥；发布前需 `catalog:keygen`。

<!-- repo-sync:machines:start -->
## 各电脑本地路径
| 电脑 | 本地路径 | 分支 | 最后更新 |
|---|---|---|---|
| Windows | `F:\CCPJ\SecretHub\.claude\worktrees\secrethub-export-autotodo-e27656` | claude/secrethub-export-autotodo-e27656 | 2026-11-19 04:50 |
<!-- repo-sync:machines:end -->
