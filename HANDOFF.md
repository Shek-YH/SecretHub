# 交接记录（HANDOFF）

> 推送前由 Claude 更新；另一台电脑打开 Claude 会话时会自动读到这里。

## 当前进度
- **macOS 版（2026-11-19，Mac 上完成，未发布）**：Mac 上 `tauri dev` 能启动，`tauri build` 产出 `SecretHub.app` + `SecretHub_<版本>_aarch64.dmg`（ad-hoc 签名，未公证）。改动：
  - MCP 数据目录按平台计算（`apps/mcp/src/data-dir.ts`）：Mac 为 `~/Library/Application Support/com.secrethub.desktop`，Windows 行为不变；
  - SEA sidecar 支持 Mac（`apps/mcp/scripts/sea-platform.mjs`）：自动下载 nodejs.org 官方 Node 22.23.3（校验 SHA-256，Homebrew 的 node 不能用），Mach-O 注入 + ad-hoc 签名；
  - `tauri.macos.conf.json`（只在 Mac 合并）：app+dmg、`entitlements.plist`（allow-jit，**没有它打包后的 sidecar 一启动就 SIGTRAP**）、Mac 独立更新清单 `latest-macos.json`；
  - Rust `platform.rs` 集中平台差异：sidecar 文件名、开发期 sidecar 按目标三元组查找、Mac GUI 启动 PATH 补 `~/.local/bin` `/opt/homebrew/bin` 等以找到 `claude`、Mac 装更新后由应用自己重启（`INSTALLER_RESTARTS_APP`）；
  - 发布脚本 `scripts/release-macos.sh`（用测试密钥 dry-run 跑通）；`docs/USAGE.md` §1.3、§6.2 已补。
  - 验证（Mac）：Rust 302 全过，前端 210 全过，MCP 320 全过（含 import-skill，Mac 是 LF 检出），sidecar smoke（独立二进制与 .app 内均通过）。**界面未由 AI 截图验证**（用户未授权 computer-use 操作该窗口），需用户在 Mac 上看一眼。
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
- **Mac 版收尾**：更新私钥已在 Mac `~/.tauri/secrethub-updater.key`（已验证与应用公钥 ID `5F7FCDD1E5402B44` 一致）；用户已开通 Apple 开发者账号，发布脚本已接入 Developer ID 签名 + 公证（`--adhoc` 为测试用）。**待用户**：① 在 Xcode 创建「Developer ID Application」证书（钥匙串目前只有 Apple Development）；② 在 App Store Connect 建 API Key（.p8）并放到 `~/.appstoreconnect/private_keys/`，把 Issuer ID / Key ID 填进 `~/.config/secrethub-release/macos.env`；③ 在 Mac 上打开 `target/release/bundle/macos/SecretHub.app` 看界面、测「连接 AI 工具」。之后跑一次正式签名构建，确认公证通过。
- Mac 首发：`bash scripts/release-macos.sh --notes "..." --dry-run` → 去掉 `--dry-run`；之后用两个版本在另一台 Mac 实测升级。Intel Mac 暂不支持（只打 aarch64）。
- Windows 已回归（62d6c16）：`build:sea` + SEA smoke 通过、Rust 303 全过、MCP 仅 import-skill CRLF 既有失败。**未做**：Windows `tauri build`（NSIS）和前端测试（该 worktree 未 `npm --prefix apps/desktop ci`；本次未改前端代码）。
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
- Mac：Rust 用 Homebrew 的 rustup（已写入 `~/.zprofile` 的 PATH）；`apps/mcp/build/node-dist/` 缓存了官方 Node，勿删也无需提交。Mac 上数据目录与开发版共用 `~/Library/Application Support/com.secrethub.desktop`（目前只有空测试库）。
- 不要读取用户资料目录里的文件内容；导入一律走 secrethub-import skill。
- `catalog-public-key.txt` 仍是开发用公钥；发布前需 `catalog:keygen`。

<!-- repo-sync:machines:start -->
## 各电脑本地路径
| 电脑 | 本地路径 | 分支 | 最后更新 |
|---|---|---|---|
| Windows | `F:\CCPJ\SecretHub\.claude\worktrees\secrethub-export-autotodo-e27656` | claude/secrethub-export-autotodo-e27656 | 2026-11-19 04:50 |
| Mac | `/Volumes/ZJ/AIPG/SecretHub` | claude/secrethub-export-autotodo-e27656 | 2026-11-19 09:42 |
<!-- repo-sync:machines:end -->
