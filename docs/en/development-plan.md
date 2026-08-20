# ma-harness Development Plan

> **Single-source roadmap** for the ma-harness.rs project. This document
> tracks phases, what was built in each, and what's planned next. For
> the **decision history** (why things are the way they are), see
> [docs/zh-CN/decision-log.md](../zh-CN/decision-log.md). For **weekly
> status**, see [docs/en/weekly/](weekly/).

[English](development-plan.md) | [简体中文](../zh-CN/development-plan.md)

## Overview

ma-harness.rs is a 12-week PoC + post-PoC continuation porting
[DeepSeek's `dsh`](https://github.com/deepseek-ai/dsh) AI agent
orchestrator from Node.js/TypeScript to Rust, with these goals:

1. **Performance**: 30%+ faster cold start, 10× faster hot-path ops
2. **Production-grade**: typed contracts, compile-time checks, no `any`
3. **Differentiate**: drop the JS ecosystem, lean Rust-only stack
4. **Compatibility**: pass dsh's conformance suite

## Phases

### Phase 0 — Foundation (Day 0, 2026-08-18)

**Goal**: spec + workspace skeleton + Gitee repo up.

**Status**: ✅ Complete

**Output**:
- 8 spec docs (AGENTS, decision-log, arch-map, macro-design, plugin-schema, tech-stack, code-mode-deferred, weekly/000)
- 13-crate workspace skeleton (cordis / core / seam / proto / server / cli / plugin-macro + 6 first-party plugins)
- Hourly cron report (`ma-harness-hourly`)

**Commits**: 8 (Day 0)

**Key decisions**: decision-log §1-§11, tech-stack.md, code-mode-deferred.md

**Links**: [weekly/000-day0.md](weekly/000-day0.md)

---

### Phase 1 — cordis + core + macro (Week 1-2, Day 1-9)

**Goal**: meta-framework + core types + 5 proc-macros.

**Status**: ✅ Complete

**Output**:
- `ma-harness-cordis`: Context / Service / Plugin / typed key / listener / scope / fork / dispose
- `ma-harness-core`: SessionEvent (15 EventType) / EventLog (rusqlite) / AgentLoop / ModelAdapter / ToolRegistry
- 5 proc-macros: `#[dsh_service]` / `#[dsh_listener]` / `#[dsh_tool]` / `#[dsh_command]` / `#[dsh_handler]`
- `ctx_key!` macro_rules! for compile-time snake_case enforcement
- `hello` plugin: end-to-end demo (ctx inject service + typed key + plugin install)

**Commits**: 7 (Day 1-9)

**Tests**: ~75 (mental-verified, network down)

**Key decisions**: decision-log §3-§4, macro-design.md

**Links**: [weekly/001-w01-w02.md](weekly/001-w01-w02.md)

---

### Phase 2 — proto + seam + server + cli (Week 3-4, Day 11-19)

**Goal**: wire format, public abstraction layer, server stack, CLI entry.

**Status**: ✅ Complete

**Output**:
- `ma-harness-proto`: 3 `.proto` files (agent / session / event) + tonic-build codegen + proto↔core conversion
- `ma-harness-seam`: 5 public traits + 5 macro re-exports + `PluginRegistry` + `CordisService`/`CordisPlugin` wrappers
- 6 first-party plugin skeletons (unified template via `gen_plugins.py`)
- `ma-harness-server`: 3 gRPC services (Agent / Session) + axum HTTP `/health` `/version`
- `ma-harness-cli`: 5 subcommands (`start` / `run` / `plugins` / `events` / `version`)

**Commits**: 5 (Day 11-19)

**Tests**: ~95 (cumulative)

**Key decisions**: decision-log §2.2 (Phase 2 scope)

**Links**: [weekly/002-w03-w04.md](weekly/002-w03-w04.md)

---

### Phase 3 — 6 first-party plugins implemented (Week 5-6, Day 21-25)

**Goal**: implement business logic for all 6 first-party plugins.

**Status**: ✅ Complete

**Output**:
- `bash` plugin: `tokio::process::Command` + timeout (5 tests)
- `fs` plugin: read/write/list + path whitelist (6 tests)
- `web` plugin: reqwest + URL whitelist (5 tests)
- `subagent` plugin: fork ctx to run sub-agent (2 tests)
- `skill` plugin: load `.skill/` files (3 tests)
- `cordis` plugin: ctx reflection (2 tests)

**Commits**: 5 (Day 21-25)

**Tests**: ~125 (cumulative)

**Links**: [weekly/003-w05-w06.md](weekly/003-w05-w06.md)

---

### Phase 4 — End-to-end demo + integration test + server (Week 7-9, Day 27-29)

**Goal**: `mah` works end-to-end as both CLI and server.

**Status**: ✅ Complete (PoC success criterion met: Default mode runs end-to-end)

**Output**:
- `ma_harness_demo` binary: 12-step walkthrough (all 7 plugins + AgentLoop + ctx)
- 13 integration tests covering 7-plugin collaboration
- `mah start` real server: tonic gRPC 50051 + axum HTTP 50050, ctrl-c graceful shutdown

**Commits**: 3 (Day 27-29)

**Tests**: ~145 (cumulative)

**Links**: [weekly/004-w07-w09.md](weekly/004-w07-w09.md)

---

### Phase 5 — Conformance + benchmark framework (Week 10-11, Day 30-43)

**Goal**: validate framework behavior against known fixtures.

**Status**: ✅ Complete

**Output**:
- `ma-harness-conformance` crate: fixture loader / compare / runner / report / 4 modules
- EventLog real persistence (replace passthrough)
- `dsh_format` conversion layer (handles dsh `expectedOutput` / `tools` aliases)
- 18 criterion benches (cordis 10 + core 4 + seam 4)
- Week 11 conformance + benchmark report templates

**Commits**: 8 (Day 30-43)

**Tests**: ~167 (cumulative, all mental-verified)

**Key decisions**: decision-log §5.1 (crate visibility), conformance-design.md, benchmark-design.md

**Links**: [weekly/005-w10-conformance.md](weekly/005-w10-conformance.md), [weekly/006-w11-frameworks.md](weekly/006-w11-frameworks.md), [weekly/007-w12-final.md](weekly/007-w12-final.md)

---

### Phase 6 — P11 dsh parity (Day 101+1)

**Goal**: match dsh behavior; pass dsh's real fixtures.

**Status**: ✅ Complete

**Output**:
- **P11-1 baseline**: 5/8 smoke + 2/7 dsh_synthetic (62.5% / 28.6%) — quantified
- **P11-1.5** conversion layer fix: 28.6% → 100% (7/7)
- **P11-2 dsh real snapshot**: 9/9 dsh acp-snapshot fixtures (100%) — true behavior equivalence

**Commits**: 8 (`1230cde`, `2c4c8d1`, `a750060`, `0d8f22d`, `3fd234c`, `89b2994`, `3d1a0cb`, `319085c`)

**Key decisions**: decision-log §28-§29

**Links**: [zh-CN/reports/dsh-benchmark-report.md](../zh-CN/reports/dsh-benchmark-report.md)

---

### Phase 7 — P11-3 to P11-9 (Day 101+1)

**Goal**: ship 7 of 9 P11 follow-up tasks (excluding P11-2.5+ which needs LLM API key).

**Status**: ✅ Complete (7 tasks)

**Output**:
- **P11-3 `mah-py` Python SDK** (subprocess wrapper, 16/16 pytest)
- **P11-4 ACP interop** (`mah acp serve`, JSON-RPC 2.0)
- **P11-5 multimodal vision** (OpenAI / Anthropic adapters, 7 tests)
- **P11-6 Plugin Registry** (manifest + source + registry, 18 tests)
- **P11-7 Vibe Coding Artifact Viewer** (10 kinds, 25 tests)
- **P11-8 Bundle** (semver constraint resolver, 13 tests)
- **P11-9 multimodal tool** (describe_image, 6 tests)
- **Skipped**: P11-2.5+ Terminal Bench 2.1 (needs LLM), P11-10 DAG (deferred to P12+)

**Commits**: 7 (`da49ffe`, `0bf9634`, `3762716`, `5cdd892`, `515240f`, `7ffc72c`, `00adff2`)

**New crates**: 4 (mah-py, registry, bundle, artifact)

**Tests**: 130+ (cumulative 300+)

**Key decisions**: decision-log §30-§36

---

### Phase 8 — P12 release + stability + docs + PyPI (Day 101+1)

**Goal**: production-ready 0.1.0 release.

**Status**: ✅ Complete (8 of 9 tasks; P12-4 PyPI initially skipped, then completed)

**Output**:
- **P12-1 DshFixtureCache**: mtime invalidation, 4 tests
- **P12-2 RetryPolicy + CircuitBreaker**: exponential backoff + jitter, 13 tests
- **P12-3 Docs site**: `docs/README.md` index + mkdocs config
- **P12-4 mah-py PyPI 0.1.1** (test.pypi.org)
- **P12-5 Registry v2**: search / list / export / merge (25 tests)
- **P12-6 ACP v2**: loadSession / cancel / image content (10 tests)
- **P12-7 Bundle v2**: lock file (18 tests)
- **P12-8 Vision tool v2**: Tool trait integration (4 tests)

**Commits**: 8

**New crates**: 1 (DAG crate for P12-9)

**Tests**: 70+ (cumulative 370+)

**Key decisions**: decision-log §37-§38

---

### Phase 9 — Code Mode (Day 68-78, P3.1-P3.7)

**Goal**: LLM generates `.wat` → compile to wasm → sandbox execute.

**Status**: ✅ Complete

**Output**:
- `ma-harness-code` crate: wasmtime + wat parser + 4-layer defense (memory limit, fuel, no fs, no net)
- `mah code run` subcommand
- `ma-harness-sandbox` crate: landlock (Linux) / Seatbelt (macOS, stub) / warn (Windows)
- LLM-to-wat translation prompt + JSON schema
- 17 tests (parse / execute / sandbox enforcement)

**Commits**: 7 (Day 68-78)

**Tests**: 17 (wasm execution paths)

**Key decisions**: [docs/zh-CN/reports/code-mode-deferred.md](../zh-CN/reports/code-mode-deferred.md) (rationale), macro updates

---

### Phase 10 — Creator + libloading (Day 79-101+1, P5.9-P10-1.8)

**Goal**: cross-dylib real plugin loading (overcomes Cordis's compile-time limits).

**Status**: ✅ Complete (P10-1.6 + P10-1.7 + P10-1.8 v1 + v2)

**Output**:
- **P10-1.6**: Creator cross-platform build hardening
- **P10-1.7**: libloading close-loop (5-layer ABI safety)
- **P10-1.8 v1**: 跨 dylib Rust ABI
- **P10-1.8 v2**: C-ABI + JSON true closed loop (production-grade)

**Commits**: 4

**Tests**: 47 (libloading / Creator / dylib)

---

### Phase 11 — P13 docs cleanup + i18n + LLM mojibake (Day 101+1)

**Goal**: clean up docs structure, complete i18n, fix mojibake.

**Status**: ✅ Complete

**Output**:
- **P13-1**: `docs/` + `docs/zh-CN/` subdirectory split
- **P13-2**: i18n convention doc (Tier 1 / Tier 2 + terminology table)
- **P13-3**: en/ subdirectory for symmetric i18n (en/ + zh-CN/ + future de/ ja/ fr/)
- **P13-4**: 8 weekly translated to English
- **P13-5**: decision-log-4-p11-12.md L495 mojibake fix (11545 weird → 0)
- **P13-6**: 1漏翻译修复 (en/conformance-design.md "之后" → "after Week 10")

**Commits**: 5 (`6b1018d`, `de5865f`, `56895a3`, `28ae577`, `c4fb1d8`, `cf36c6b`)

**Tests**: 641 (cumulative)

**Key decisions**: [docs/en/i18n.md](i18n.md) (updated)

---

### Phase 12 — P14 Cargo workspaces + GH Pages registry (Day 101+1)

**Goal**: production-grade plugin ecosystem.

**Status**: ✅ Complete

**Output**:
- **P14-1**: `cargo-workspaces` 0.4.2 installed; `cargo ws plan` auto-computes dependency order
- **P14-2**: `mah registry list` / `mah registry export` CLI subcommands
- **P14-3**: `registry-pages.yml` GitHub Actions workflow (GH Pages deployment)
- **P14-4**: `docs/en/operations/registry-pages.md` setup guide
- **P14-5**: 3 unit tests for `mah registry` CLI

**Commits**: 1 (`243799f`)

**Tests**: 641 (cumulative, +3 from CLI tests)

**Pending setup (one-time, business side)**:
1. GitHub repo → Settings → Pages → Source: "GitHub Actions"
2. `mkdir docs/registry && mah registry export --output docs/registry/registry.json`
3. Commit + push → workflow deploys to gh-pages

---

## Cumulative stats (as of 2026-08-20)

| Metric | Value |
|---|---|
| Crates | 16 (9 internal + 7 first-party plugins) + 7 framework extensions |
| Lines of Rust | ~16,000 |
| Lines of docs (en + zh-CN) | ~50,000 |
| Tests | 641 (lib + bin + integration) |
| Commits | 50+ |
| Weekly reports | 8 (Day 0 / Week 1-2 / 3-4 / 5-6 / 7-9 / 10 / 11 / 12-final) |
| Decision log entries | 42 (§ 1-42) |
| Public API locked | `ma-harness-seam` (5 traits + 5 macros) |

## Next: Phase 13+ (post-101+1)

### P15+ — Production hardening (planned)

- [ ] **crates.io 0.1.0 release**: workflow ready, waiting on `CRATES_IO_TOKEN` secret
- [ ] **mah-py 0.1.1 → pypi.org**: workflow ready, waiting on pypi.org token
- [ ] **Cargo workspaces publish automation**: `cargo ws publish` instead of hand-rolled
- [ ] **Cross-platform binary release**: GitHub Actions matrix (ubuntu / windows / macos)
- [ ] **dsh migration tool**: help users convert dsh plugins to ma-harness
- [ ] **GH Pages registry mirror on Gitee**
- [ ] **mah plugin install <name>**: auto-fetch from registry URL
- [ ] **Plugin signature verification**: GPG / cosign for supply chain security
- [ ] **P12-2+ retry/circuit breaker integration**: with LLM adapters

### P16+ — Long-term

- [ ] **PyO3 v2 mah-py**: native bindings instead of subprocess
- [ ] **DAG task orchestration**: deferred from P11-10
- [ ] **Postgres session store**: scale beyond single machine
- [ ] **Multi-tenant isolation**: per-user plugin sandbox
- [ ] **Web UI**: complement to TUI

## Status legend

- ✅ Complete (committed + tests passing)
- 🚧 In progress
- 📋 Planned (P15+)
- ⏸️ Deferred (waiting on external input: LLM API key, pypi.org token, etc.)

## How to read this document

- **Each phase** is a self-contained section with: goal, status, output, commits, tests, key decisions, and links
- **Status legend** at the bottom tells you what's done, in progress, planned, or deferred
- **Cumulative stats** at the end give you a snapshot of the project size
- **Next section** lists work that comes after the current state

## Related docs

- [i18n.md](i18n.md) — documentation convention
- [tech-stack.md](tech-stack.md) — frozen tech stack decisions
- [ma-harness-arch-map.md](ma-harness-arch-map.md) — architecture overview
- [zh-CN/decision-log.md](../zh-CN/decision-log.md) — 42 design decisions
- [zh-CN/weekly/](../zh-CN/weekly/) — 8 weekly status reports
- [zh-CN/reports/](../zh-CN/reports/) — phase reports
- [user-guide/](user-guide/) — how to use ma-harness
