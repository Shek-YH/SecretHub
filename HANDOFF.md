# 交接记录（HANDOFF）

> 推送前由 Claude 更新；另一台电脑打开 Claude 会话时会自动读到这里。

## 当前进度
- 分支 `feat/simplify-entry-and-providers`（基于 PR #1 的分支 `fix/p0-mcp-env-merge-provider-catalog`），已完成并全部测试通过（Rust 280、前端 196、MCP 268、`npm run e2e:import` clean）：
  - 常用 10 家服务商、一屏录入、按类型保存（账号密码/银行卡/笔记/开发密钥）、修改主密码、可恢复的备份 SHB2；
  - 配置组（服务器/数据库/云服务）、混合勾选导出合并 `.env`、方案变量子集；
  - 导入助手、浏览器密码 CSV、银行卡模板、智能粘贴；
  - AI 整理导入（打码视图 + 自动写入护栏 + 批次撤销）、导出带中文注释。
- 完整使用说明见 `docs/USAGE.md`（安装、桌面功能、MCP 工具、5 个 skill 的场景与调用）。
- Windows 本机已：MCP 以 user scope 注册；5 个 skill 链接到 `~/.claude/skills`；用户资料已用 AI 整理导入进本机保险库；三台服务器配了 SSH 别名（只在 Windows 的 `~/.ssh/config`）。

## 下一步
- 用户真机试：按类型添加、配置组导出、AI 整理导入的通知与撤销。
- 已知小问题：撤销导入后，在非 API Key 页面看不到“撤销成功”提示。
- 合并顺序：先合 PR #1，再为本分支开 PR。

## 注意事项
- 新电脑先执行 `npm --prefix apps/desktop ci`、`npm --prefix apps/mcp ci`、`npm --prefix apps/mcp run build`（`node_modules`/`dist` 不进 git，缺失会导致 MCP `Connection closed`）。
- Mac 上需要自己注册 MCP、链接 skill、配置 SSH 别名；保险库是本机文件，不随 git 同步（用 设置 → 导出备份 / 从备份恢复 迁移）。
- 不要读取用户资料目录里的文件内容；导入一律走 secrethub-import skill。
- `catalog-public-key.txt` 仍是开发用公钥；发布前需 `catalog:keygen`。

<!-- repo-sync:machines:start -->
## 各电脑本地路径
| 电脑 | 本地路径 | 分支 | 最后更新 |
|---|---|---|---|
| Windows | `F:\CCPJ\SecretHub` | feat/simplify-entry-and-providers | 2026-11-14 03:05 |
<!-- repo-sync:machines:end -->
