# ma-harness.rs 文档索引 (P12-3 文档站 v1)

[English](../README.md) | [简体中文](README.md)

> **入口**: 本 README 是 ma-harness.rs 文档总索引, 按主题分块.
> **P12-3 v1**: Markdown 文档已经齐全, 本 README 提供"按角色 / 按主题"两个维度的导航.
> **v2**: 接 mdbook / mkdocs 静态站 (业务方驱动)

📖 **根 README**: [English](../../README.md) | [简体中文](../../README.zh-CN.md) — 跟 dsh 对比 / 完成度 / 快速开始 / 功能矩阵

🌐 **i18n 规范**: [docs/i18n.md](i18n.md) — 双语 (英文 / 简体中文) 子目录结构跟翻译规则.

---

## 按角色 (Reader's Path)

### 业务方 (用 ma-harness 跑 agent)

- [快速开始](../../crates/mah-py/README.md) — Python SDK `mah-py`
- [快速开始 (CLI)](../../crates/ma-harness-cli/) — `mah` 命令行
- [ACP 互通 (跟 dsh)](../../crates/ma-harness-cli/src/acp.rs) — `mah acp serve` 跟 dsh / Codex 互通
- [Plugin Registry](../../crates/ma-harness-registry/) — 业务方 publish / install 第三方 plugin
- [Bundle 概念](../../crates/ma-harness-bundle/) — 业务方一键装多个 plugin
- [Artifact Viewer](../../crates/ma-harness-artifact/) — 业务方 agent 产物识别 / 渲染
- [Vision tool](../../crates/ma-harness-model/src/vision_tool.rs) — `describe_image` 多模态
- [决策日志](decision-log.md) — 业务方查"为什么这么设计"

### 框架开发者 (改 ma-harness 内部)

- [架构总览](ma-harness-arch-map.md) — 14 个 crate 关系
- [技术栈](tech-stack.md) — 框架依赖 / 工具链
- [conformance 设计](conformance-design.md) — 业务方写 fixture, 框架跑
- [Plugin Schema](../../crates/ma-harness-conformance/src/dsh_format.rs) — dsh 风格 fixture
- [ma-py SDK 设计](../../crates/mah-py/) — 业务方 Python 集成
- [决策日志](decision-log.md) — 历次 design 决策
- [i18n 规范](i18n.md) — 双语文档命名跟翻译规则

### 性能 / 优化 (跑 dsh Terminal Bench 业务方)

- [P11-2 dsh 真实 snapshot 跑分](dsh-benchmark-report.md) — 9/9 = 100% 等价
- [P11 路线图](roadmap-phase-11.md) — 后续 9 任务 (P11-3 到 P11-10)
- [P11 全收官报告](p11-final-report.md) — P11 收官 + 跟 dsh 对照
- [P12 全收官报告](p12-final-report.md) — P12 收官 (发版 / 稳定性 / 文档 / PyPI)
- [benchmark 设计](benchmark-design.md) — bench 体系
- [week 11 benchmark 报告](benchmark-report-week11.md) — perf baseline
- [week 11 conformance 报告](conformance-report-week11.md) — conformance baseline

### 历史 / 决策追溯

- [决策日志](decision-log.md) — § 1-39 历次 design 决策
- [P11 baseline 报告](p11-baseline-report.md) — dsh_synthetic 28.6% → 100%
- [P11 全收官报告](p11-final-report.md) — P11 收官
- [P12 全收官报告](p12-final-report.md) — P12 收官
- [Roadmap Phase 7](roadmap-phase-7.md) — Phase 7-10 路线
- [Roadmap Phase 11](roadmap-phase-11.md) — P11 路线

---

## 按主题 (Topic Index)

### 架构 / 设计

- [架构总览](ma-harness-arch-map.md)
- [技术栈](tech-stack.md)
- [Macro 设计](macro-design.md) — `#[derive(Context)]` 等
- [Plugin Schema v1](plugin-schema-v1.md) — Plugin 协议
- [i18n 规范](i18n.md) — 双语文档规范

### Conformance / 跑分

- [Conformance 设计](conformance-design.md)
- [P11-2 dsh 真实 snapshot 跑分](dsh-benchmark-report.md)
- [P11 baseline 报告](p11-baseline-report.md)
- [P11 全收官报告](p11-final-report.md)
- [P12 全收官报告](p12-final-report.md)
- [week 11 conformance 报告](conformance-report-week11.md)
- [week 11 benchmark 报告](benchmark-report-week11.md)
- [Benchmark 设计](benchmark-design.md)

### Roadmap / 路线

- [Roadmap Phase 7](roadmap-phase-7.md) — Phase 7-10 (Code Mode / TUI / ACP / Vision)
- [Roadmap Phase 11](roadmap-phase-11.md) — P11 (跟 dsh 对齐)
- [P11 baseline 报告](p11-baseline-report.md)
- [P11 全收官报告](p11-final-report.md)
- [P12 全收官报告](p12-final-report.md)

### 实验 / 评估

- [PyO3 评估](pyo3-evaluation.md) — Python binding 选型
- [Code Mode deferred](code-mode-deferred.md) — wasm 模式延后决策

### Crate 内部文档 (P11 收官)

- [mah-py Python SDK](../../crates/mah-py/README.md) — 16 tests + 5 examples
- [ma-harness-registry](../../crates/ma-harness-registry/) — Plugin Registry 18 tests
- [ma-harness-bundle](../../crates/ma-harness-bundle/) — Bundle 13 tests
- [ma-harness-artifact](../../crates/ma-harness-artifact/) — Vibe Coding 25 tests
- [ACP (CLI 模块)](../../crates/ma-harness-cli/src/acp.rs) — JSON-RPC 2.0 server

### API 参考

- [OpenAPI spec (English)](api/openapi.json) — REST API 表面 (英文)
- [OpenAPI spec (简体中文)](api/openapi.json) — REST API 表面 (简体中文)

### 决策

- [决策日志](decision-log.md) — § 1-39 完整记录

---

## 文档完整性 (P12-3 v1 收官时)

| 类别 | 数量 | 状态 |
|---|---|---|
| 架构 / 设计 | 5 (+i18n) | ✅ |
| Conformance / 跑分 | 8 | ✅ |
| Roadmap / 路线 | 5 | ✅ |
| 实验 / 评估 | 2 | ✅ |
| Crate 内部 README | 8 + 8 (.zh-CN) | ✅ |
| API 参考 (OpenAPI) | 1 + 1 (zh-CN) | ✅ |
| 决策日志 | 1 (~80 KB, 39 章节) | ✅ |
| **总计** | **30 markdown + 2 OpenAPI** | **✅** |

## 业务方读法

1. **新业务方**: [Python SDK README](../../crates/mah-py/README.md) → 5 examples → [ACP 模块](../../crates/ma-harness-cli/src/acp.rs)
2. **改 ma-harness 内部**: [架构总览](ma-harness-arch-map.md) → [conformance 设计](conformance-design.md) → [决策日志](decision-log.md)
3. **跑 Terminal Bench**: [dsh-benchmark-report](dsh-benchmark-report.md) → [P11 全收官报告](p11-final-report.md) → [P12 全收官报告](p12-final-report.md)

## 给后来人

- 改 ma-harness 时, 跑 `cargo test --workspace` 全过 (638+ tests)
- 加新 doc 章节时, 在本 README 加 link
- 决策日志 (decision-log.md) 是 single source of truth, 改设计前查一下
- 更新文档时, 遵守 [i18n 规范](i18n.md): 同时更新 `name.md` (英文) 跟 `name.zh-CN.md` (中文)
- 业务方反馈 / issue / PR, 优先更新本 README
