# ma-harness.rs Documentation Index (P12-3 docs site v1)

[English](README.md) | [简体中文](zh-CN/README.md)

> **Entry point**: This README is the master index of all `ma-harness.rs`
> documentation, organized by topic.
> **P12-3 v1**: Markdown docs are now complete; this README provides
> navigation along two dimensions: by **role** and by **topic**.
> **v2**: Static site via mdbook / mkdocs (business-driven).

📖 **Root README**: [English](../README.md) | [简体中文](../README.zh-CN.md) — dsh comparison / completion / quick start / feature matrix

🌐 **i18n convention**: [docs/i18n.md](i18n.md) — bilingual (English / Simplified Chinese) subdirectory structure and translation rules.

---

## By Role (Reader's Path)

### Application developers (using ma-harness to run an agent)

- [Quick start](../crates/mah-py/README.md) — Python SDK `mah-py`
- [Quick start (CLI)](../crates/ma-harness-cli/) — `mah` command line
- [ACP (Agent Communication Protocol)](../crates/ma-harness-cli/src/acp.rs) — `mah acp serve` interoperates with dsh / Codex
- [Plugin Registry](../crates/ma-harness-registry/) — publish / install third-party plugins
- [Bundle concept](../crates/ma-harness-bundle/) — install a set of plugins at once
- [Artifact viewer](../crates/ma-harness-artifact/) — detect / render agent output
- [Vision tool](../crates/ma-harness-model/src/vision_tool.rs) — `describe_image` multi-modal tool
- [Decision log (zh-CN)](zh-CN/decision-log.md) — "why was it designed this way?"

### Framework developers (modifying ma-harness internals)

- [Architecture map](ma-harness-arch-map.md) — 14 crate relationships
- [Tech stack](tech-stack.md) — framework dependencies / toolchain
- [Conformance design](conformance-design.md) — write fixtures, run the framework
- [Plugin schema](../crates/ma-harness-conformance/src/dsh_format.rs) — dsh-style fixtures
- [mah-py SDK design](../crates/mah-py/) — Python integration
- [Decision log (zh-CN)](zh-CN/decision-log.md) — historical design decisions

### Performance / optimization (running dsh Terminal Bench)

- [P11-2 dsh real snapshot benchmark](zh-CN/reports/dsh-benchmark-report.md) — 9/9 = 100% equivalent
- [P11 roadmap](zh-CN/reports/roadmap-phase-11.md) — 9 follow-up tasks (P11-3 to P11-10)
- [P11 final report](zh-CN/reports/p11-final-report.md) — P11 wrap-up + dsh comparison
- [Benchmark design](benchmark-design.md) — bench methodology
- [Week 11 benchmark report](zh-CN/reports/benchmark-report-week11.md) — perf baseline
- [Week 11 conformance report](zh-CN/reports/conformance-report-week11.md) — conformance baseline

### History / decision tracing

- [Decision log](zh-CN/decision-log.md) — § 1-39 historical design decisions
- [P11 baseline report](zh-CN/reports/p11-baseline-report.md) — dsh_synthetic 28.6% → 100%
- [P11 final report](zh-CN/reports/p11-final-report.md) — P11 wrap-up
- [P12 final report](zh-CN/reports/p12-final-report.md) — P12 wrap-up (release / stability / docs / PyPI)
- [Roadmap Phase 7](zh-CN/reports/roadmap-phase-7.md) — Phase 7-10 (Code Mode / TUI / ACP / Vision)
- [Roadmap Phase 11](zh-CN/reports/roadmap-phase-11.md) — P11 (dsh parity)

---

## By Topic (Topic Index)

### Architecture / design

- [Architecture map](ma-harness-arch-map.md)
- [Tech stack](tech-stack.md)
- [Macro design](macro-design.md) — `#[derive(Context)]` etc.
- [Plugin schema v1](plugin-schema-v1.md) — plugin protocol
- [i18n convention](i18n.md) — bilingual doc standard

### Conformance / benchmark

- [Conformance design](conformance-design.md)
- [P11-2 dsh real snapshot benchmark](zh-CN/reports/dsh-benchmark-report.md)
- [P11 baseline report](zh-CN/reports/p11-baseline-report.md)
- [P11 final report](zh-CN/reports/p11-final-report.md)
- [P12 final report](zh-CN/reports/p12-final-report.md)
- [Week 11 conformance report](zh-CN/reports/conformance-report-week11.md)
- [Week 11 benchmark report](zh-CN/reports/benchmark-report-week11.md)
- [Benchmark design](benchmark-design.md)

### Roadmap

- [Roadmap Phase 7](zh-CN/reports/roadmap-phase-7.md) — Phase 7-10 (Code Mode / TUI / ACP / Vision)
- [Roadmap Phase 11](zh-CN/reports/roadmap-phase-11.md) — P11 (dsh parity)
- [P11 baseline report](zh-CN/reports/p11-baseline-report.md)
- [P11 final report](zh-CN/reports/p11-final-report.md)
- [P12 final report](zh-CN/reports/p12-final-report.md)

### Experiments / evaluations

- [PyO3 evaluation](zh-CN/reports/pyo3-evaluation.md) — Python binding technology choice
- [Code Mode deferred](zh-CN/reports/code-mode-deferred.md) — wasm mode postponement decision

### Crate-internal docs (P11 wrap-up)

- [mah-py Python SDK](../crates/mah-py/README.md) — 16 tests + 5 examples
- [ma-harness-registry](../crates/ma-harness-registry/) — Plugin Registry 18 tests
- [ma-harness-bundle](../crates/ma-harness-bundle/) — Bundle 13 tests
- [ma-harness-artifact](../crates/ma-harness-artifact/) — Vibe Coding 25 tests
- [ACP (CLI module)](../crates/ma-harness-cli/src/acp.rs) — JSON-RPC 2.0 server

### API reference

- [OpenAPI spec (English)](api/openapi.json) — REST API surface (English)
- [OpenAPI spec (Chinese)](zh-CN/api/openapi.json) — REST API surface (简体中文)

### Decisions

- [Decision log](zh-CN/decision-log.md) — § 1-39 complete record

---

## Doc Completeness (as of P12-3 v1 wrap-up + i18n pass)

| Category                   | Count              | Status |
|----------------------------|--------------------|--------|
| Architecture / design      | 5 (+i18n)          | ✅     |
| Conformance / benchmark    | 8                  | ✅     |
| Roadmap                    | 5                  | ✅     |
| Experiments / evaluations  | 2                  | ✅     |
| Crate-internal README      | 8 + 8 (.zh-CN)     | ✅     |
| API reference (OpenAPI)    | 1 + 1 (zh-CN)      | ✅     |
| Decision log               | 1 (~80 KB, 39 sections) | ✅ |
| **Total**                  | **30 markdown + 2 OpenAPI** | **✅** |

## Reading paths

1. **New application developer**: [Python SDK README](../crates/mah-py/README.md) → 5 examples → [ACP module](../crates/ma-harness-cli/src/acp.rs)
2. **Modifying ma-harness internals**: [Architecture map](ma-harness-arch-map.md) → [Conformance design](conformance-design.md) → [Decision log](zh-CN/decision-log.md)
3. **Running Terminal Bench**: [dsh benchmark report](zh-CN/reports/dsh-benchmark-report.md) → [P11 final report](zh-CN/reports/p11-final-report.md) → [P12 final report](zh-CN/reports/p12-final-report.md)

## Notes for future contributors

- When modifying `ma-harness`, run `cargo test --workspace` and ensure all 638+ tests pass.
- When adding a new doc, add a link to this README.
- The [decision log](zh-CN/decision-log.md) is the single source of truth; check it before changing design.
- When updating a doc, follow the [i18n convention](i18n.md): update both `name.md` (English) and `name.zh-CN.md` (Chinese).
- For application feedback / issues / PRs, prioritize updating this README.
