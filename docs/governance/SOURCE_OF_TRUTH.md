# SecretHub Source of Truth

- 最新用户指令：从 PRD 从零开发 SecretHub；界面默认中文，支持中文/English 切换；持续执行至 PRD 要求完成。
- 活跃 PRD：`SecretHub_Codex_PRD.md`。
- 当前 UI 原型：暂无独立原型；按设计文档直接在 `apps/desktop/src` 实现并维护可交互 UI。
- 架构决策：`docs/superpowers/specs/2026-09-13-secrethub-design.md`。
- 实现真源：`apps/desktop`、`crates` 与 `tests`，以当前代码和测试结果为准。
- 安全优先级：P0 Security > P1 Correctness > P2 UX > P3 Convenience。
