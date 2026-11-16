# 交接记录（HANDOFF）

> 推送前由 Claude 更新；另一台电脑打开 Claude 会话时会自动读到这里。

## 当前进度
- 分支 `feat/simplify-entry-and-providers`（PR #2，基于 PR #1 的分支 `fix/p0-mcp-env-merge-provider-catalog`），全部测试通过（Rust 289、前端 200、MCP 268、smoke）：
  - 常用 10 家服务商、按类型录入、配置组导出、AI 整理导入（打码视图 + 护栏 + 批次撤销）、带注释导出；
  - **可分发安装包**：MCP 打成单文件 exe（Node SEA，sidecar `secrethub-mcp.exe`），用户无需装 Node；设置页「连接 AI 工具」一键注册 Claude Code / Claude Desktop / Cursor 的 MCP 并复制 5 个 skill；
  - **静默自动更新**（tauri-plugin-updater）：启动 3 秒后 + 每小时检查，静默下载，顶栏「更新」按钮，退出时静默安装；NSIS 当前用户安装、中文、安装前结束 sidecar 进程。
  - 更新地址：`https://updates.9aizhuan.com/secrethub/latest.json`（腾讯宝塔服务器 `tx-bt` 的静态站点 `/www/wwwroot/updates.9aizhuan.com`，每个产品一个子目录；DNSPod A 记录 + acme.sh Let's Encrypt 证书自动续期，已验证 HTTPS 200）。
- 使用说明 `docs/USAGE.md`（§6.1 发布安装包与自动更新）。

## 下一步
- 已发布 0.1.0（2026-11-16）：安装包固定下载链接 https://updates.9aizhuan.com/secrethub/SecretHub-setup-latest.exe（发布脚本每次自动更新这个软链接），已写入飞书「线上网站与管理后台」表格。
- 升级链路要连发两个版本，在**另一台干净机器或虚拟机**上装旧版再测升级；本机正在使用 SecretHub，不要在本机装安装包。
- 真机测「连接 AI 工具」按钮、按类型添加、AI 导入通知与撤销。
- 已知小问题：撤销导入后，在非 API Key 页面看不到“撤销成功”提示。
- 合并顺序：先合 PR #1，再合 PR #2（普通 merge，不要 squash）。

## 注意事项
- 新电脑先执行 `npm --prefix apps/desktop ci`、`npm --prefix apps/mcp ci`、`npm --prefix apps/mcp run build`；打包前再 `npm --prefix apps/mcp run build:sea`。
- **更新签名私钥** `C:\Users\Administrator\.tauri\secrethub-updater.key`（无密码，不进 git）：丢了老版本就收不到更新，需用户离线备份；在 Mac 上发版也要同一个文件。
- 在 PowerShell 手动 `tauri build` 会因空的 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 卡住，用发布脚本。
- 安装包未做代码签名（用户决定先不签），会有 SmartScreen 提示；SEA 注入后 node.exe 原签名失效，可能被杀软误报。
- Mac 版尚未打包（sidecar 目前只构建 x86_64-pc-windows-msvc）。
- 不要读取用户资料目录里的文件内容；导入一律走 secrethub-import skill。
- `catalog-public-key.txt` 仍是开发用公钥；发布前需 `catalog:keygen`。

<!-- repo-sync:machines:start -->
## 各电脑本地路径
| 电脑 | 本地路径 | 分支 | 最后更新 |
|---|---|---|---|
| Windows | `F:\CCPJ\SecretHub` | main | 2026-11-16 03:47 |
<!-- repo-sync:machines:end -->
