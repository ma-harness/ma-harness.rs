# ma-harness.rs — 决策档案 (Decision Log)

> 项目内部代号: **ma-harness.rs** (Rust 重写 DeepSeek Harness)
> 文档目的: 把分散在多轮对话里的关键决策落成"宪法",任何后续修改都要回头对账
> 最后更新: 2026-08-18
>
> **配套文档**: 完整路线图 / 各 phase 进展 / P15+ 未来见 [development-plan.md](development-plan.md).
> **使用手册** (终端用户视角) 见 [user-guide/](user-guide/).

---


## 目录 (Table of Contents)

- [Part 1 — Design (设计阶段)](decision-log-1-design.md) — § 1-11
  - 设计原则 / 技术栈锁定 / 命名规范 / 仓库 / 协作模式 / 跟 dsh 关系 / 待办
- [Part 2 — Phase 4-6 (HTTP + Streaming)](decision-log-2-phase6.md) — § 12-21
  - axum→salvo / pyo3 评估 / `mah run-stream` / OpenAI/Anthropic SSE / perf bench / TUI 增强 / salvo 0.93+0.95
- [Part 3 — Phase 7-10 (Code/ACP/Vision/Creator)](decision-log-3-phase7-10.md) — § 22-27
  - Phase 7-10 全收官 + P10-1.6 编译硬化 + P10-1.7 libloading 闭环
- [Part 4 — P11 + P12 + P13 + P14 (dsh parity + release + docs i18n + registry)](decision-log-4-p11-12.md) — § 28-38
  - P11-1 baseline / P11-2 dsh snapshot / P12 全收官 / PyPI / P13 docs 整理 (待补) / P14 registry (待补)

---

