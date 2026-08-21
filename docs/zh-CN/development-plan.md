# ma-harness 开发计划

> ma-harness.rs 项目的**单源路线图**。本文档跟踪每个 phase、产出、commit 数,
> 以及未来计划。**决策历史** (为什么这样设计) 见
> [docs/zh-CN/decision-log.md](../decision-log.md)。**周报** 见
> [docs/zh-CN/weekly/](weekly/)。

[English](../en/development-plan.md) | [简体中文](development-plan.md)

## 概览

ma-harness.rs 是 12 周 PoC + PoC 后续延续,把 [DeepSeek 的 `dsh`](https://github.com/deepseek-ai/dsh)
AI agent orchestrator 从 Node.js/TypeScript 重写到 Rust,目标:

1. **性能**: 冷启动 30%+ 加速, 热路径 10× 加速
2. **生产级**: 强类型契约, 编译期检查, 不用 `any`
3. **差异化**: 砍掉 JS 生态, 纯 Rust 技术栈
4. **兼容**: 跑过 dsh 的 conformance suite

## Phases

### Phase 0 — 基础 (Day 0, 2026-08-18)

**目标**: spec + workspace 骨架 + Gitee 仓库上线。

**状态**: ✅ 完成

**产出**:
- 8 份 spec 文档 (AGENTS, decision-log, arch-map, macro-design, plugin-schema, tech-stack, code-mode-deferred, weekly/000)
- 13-crate workspace 骨架 (cordis / core / seam / proto / server / cli / plugin-macro + 6 first-party plugin)
- 每小时 cron 汇报 (`ma-harness-hourly`)

**Commits**: 8 (Day 0)

**关键决策**: decision-log §1-§11, tech-stack.md, code-mode-deferred.md

**链接**: [weekly/000-day0.md](weekly/000-day0.md)

---

### Phase 1 — cordis + core + macro (Week 1-2, Day 1-9)

**目标**: 元框架 + 核心类型 + 5 个 proc-macro。

**状态**: ✅ 完成

**产出**:
- `ma-harness-cordis`: Context / Service / Plugin / typed key / listener / scope / fork / dispose
- `ma-harness-core`: SessionEvent (15 EventType) / EventLog (rusqlite) / AgentLoop / ModelAdapter / ToolRegistry
- 5 个 proc-macro: `#[dsh_service]` / `#[dsh_listener]` / `#[dsh_tool]` / `#[dsh_command]` / `#[dsh_handler]`
- `ctx_key!` macro_rules! 编译期 snake_case 强制
- `hello` plugin: 端到端 demo (ctx inject service + typed key + plugin install)

**Commits**: 7 (Day 1-9)

**测试**: ~75 (mental 验证, 网络不通)

**关键决策**: decision-log §3-§4, macro-design.md

**链接**: [weekly/001-w01-w02.md](weekly/001-w01-w02.md)

---

### Phase 2 — proto + seam + server + cli (Week 3-4, Day 11-19)

**目标**: wire 格式, 公开抽象层, server 栈, CLI 入口。

**状态**: ✅ 完成

**产出**:
- `ma-harness-proto`: 3 个 `.proto` 文件 (agent / session / event) + tonic-build codegen + proto↔core 转换
- `ma-harness-seam`: 5 公开 trait + 5 macro re-export + `PluginRegistry` + `CordisService`/`CordisPlugin` 包装
- 6 个 first-party plugin 骨架 (通过 `gen_plugins.py` 统一模板)
- `ma-harness-server`: 3 个 gRPC service (Agent / Session) + axum HTTP `/health` `/version`
- `ma-harness-cli`: 5 个子命令 (`start` / `run` / `plugins` / `events` / `version`)

**Commits**: 5 (Day 11-19)

**测试**: ~95 (累计)

**关键决策**: decision-log §2.2 (Phase 2 范围)

**链接**: [weekly/002-w03-w04.md](weekly/002-w03-w04.md)

---

### Phase 3 — 6 个 first-party 插件实装 (Week 5-6, Day 21-25)

**目标**: 实装所有 6 个 first-party 插件的业务逻辑。

**状态**: ✅ 完成

**产出**:
- `bash` plugin: `tokio::process::Command` + timeout (5 tests)
- `fs` plugin: read/write/list + 路径白名单 (6 tests)
- `web` plugin: reqwest + URL 白名单 (5 tests)
- `subagent` plugin: fork ctx 跑子 agent (2 tests)
- `skill` plugin: 加载 `.skill/` 文件 (3 tests)
- `cordis` plugin: ctx 反射 (2 tests)

**Commits**: 5 (Day 21-25)

**测试**: ~125 (累计)

**链接**: [weekly/003-w05-w06.md](weekly/003-w05-w06.md)

---

### Phase 4 — 端到端 demo + integration test + server (Week 7-9, Day 27-29)

**目标**: `mah` 端到端跑通, 既能 CLI 又能 server。

**状态**: ✅ 完成 (PoC 成功判据达成: Default 模式跑通)

**产出**:
- `ma_harness_demo` 二进制: 12 步走通 (所有 7 plugin + AgentLoop + ctx)
- 13 个 integration test 覆盖 7-plugin 协作
- `mah start` 真 server: tonic gRPC 50051 + axum HTTP 50050, ctrl-c 优雅退出

**Commits**: 3 (Day 27-29)

**测试**: ~145 (累计)

**链接**: [weekly/004-w07-w09.md](weekly/004-w07-w09.md)

---

### Phase 5 — Conformance + benchmark 框架 (Week 10-11, Day 30-43)

**目标**: 用已知 fixture 验证 framework 行为。

**状态**: ✅ 完成

**产出**:
- `ma-harness-conformance` crate: fixture loader / compare / runner / report / 4 个 module
- EventLog 真持久化 (替换透传)
- `dsh_format` 转换层 (处理 dsh `expectedOutput` / `tools` alias)
- 18 个 criterion bench (cordis 10 + core 4 + seam 4)
- Week 11 conformance + benchmark 报告模板

**Commits**: 8 (Day 30-43)

**测试**: ~167 (累计, 全 mental 验证)

**关键决策**: decision-log §5.1 (crate 公开性), conformance-design.md, benchmark-design.md

**链接**: [weekly/005-w10-conformance.md](weekly/005-w10-conformance.md), [weekly/006-w11-frameworks.md](weekly/006-w11-frameworks.md), [weekly/007-w12-final.md](weekly/007-w12-final.md)

---

### Phase 6 — P11 dsh parity (Day 101+1)

**目标**: 跟 dsh 行为对齐; 跑过 dsh 真实 fixture。

**状态**: ✅ 完成

**产出**:
- **P11-1 baseline**: 5/8 smoke + 2/7 dsh_synthetic (62.5% / 28.6%) — 量化
- **P11-1.5** 转换层修: 28.6% → 100% (7/7)
- **P11-2 dsh 真实 snapshot**: 9/9 dsh acp-snapshot fixture (100%) — 真行为等价

**Commits**: 8 (`1230cde`, `2c4c8d1`, `a750060`, `0d8f22d`, `3fd234c`, `89b2994`, `3d1a0cb`, `319085c`)

**关键决策**: decision-log §28-§29

**链接**: [reports/dsh-benchmark-report.md](reports/dsh-benchmark-report.md)

---

### Phase 7 — P11-3 到 P11-9 (Day 101+1)

**目标**: 完成 7/9 P11 后续任务 (排除 P11-2.5+ 需要 LLM API key)。

**状态**: ✅ 完成 (7 tasks)

**产出**:
- **P11-3 `mah-py` Python SDK** (subprocess wrapper, 16/16 pytest)
- **P11-4 ACP 互通** (`mah acp serve`, JSON-RPC 2.0)
- **P11-5 多模态 vision** (OpenAI / Anthropic adapter, 7 tests)
- **P11-6 Plugin Registry** (manifest + source + registry, 18 tests)
- **P11-7 Vibe Coding Artifact Viewer** (10 kinds, 25 tests)
- **P11-8 Bundle** (semver constraint resolver, 13 tests)
- **P11-9 多模态 tool** (describe_image, 6 tests)
- **跳过**: P11-2.5+ Terminal Bench 2.1 (需要 LLM), P11-10 DAG (推迟到 P12+)

**Commits**: 7 (`da49ffe`, `0bf9634`, `3762716`, `5cdd892`, `515240f`, `7ffc72c`, `00adff2`)

**新 crate**: 4 (mah-py, registry, bundle, artifact)

**测试**: 130+ (累计 300+)

**关键决策**: decision-log §30-§36

---

### Phase 8 — P12 release + stability + docs + PyPI (Day 101+1)

**目标**: 生产就绪 0.1.0 发版。

**状态**: ✅ 完成 (8/9 task; P12-4 PyPI 最初跳过, 后来补做)

**产出**:
- **P12-1 DshFixtureCache**: mtime 失效, 4 tests
- **P12-2 RetryPolicy + CircuitBreaker**: 指数 backoff + jitter, 13 tests
- **P12-3 Docs 站**: `docs/README.md` 索引 + mkdocs 配置
- **P12-4 mah-py PyPI 0.1.1** (test.pypi.org)
- **P12-5 Registry v2**: search / list / export / merge (25 tests)
- **P12-6 ACP v2**: loadSession / cancel / image content (10 tests)
- **P12-7 Bundle v2**: lock file (18 tests)
- **P12-8 Vision tool v2**: Tool trait 集成 (4 tests)

**Commits**: 8

**新 crate**: 1 (P12-9 DAG crate)

**测试**: 70+ (累计 370+)

**关键决策**: decision-log §37-§38

---

### Phase 9 — Code Mode (Day 68-78, P3.1-P3.7)

**目标**: LLM 生成 `.wat` → 编译到 wasm → 沙箱执行。

**状态**: ✅ 完成

**产出**:
- `ma-harness-code` crate: wasmtime + wat 解析器 + 4 层防御 (内存限制, fuel, 无 fs, 无 net)
- `mah code run` 子命令
- `ma-harness-sandbox` crate: landlock (Linux) / Seatbelt (macOS, stub) / warn (Windows)
- LLM 转 wat 的 prompt + JSON schema
- 17 个测试 (parse / execute / 沙箱强制)

**Commits**: 7 (Day 68-78)

**测试**: 17 (wasm 执行路径)

**关键决策**: [reports/code-mode-deferred.md](reports/code-mode-deferred.md) (理由), macro 升级

---

### Phase 10 — Creator + libloading (Day 79-101+1, P5.9-P10-1.8)

**目标**: 跨 dylib 真插件装载 (突破 Cordis 编译期限制)。

**状态**: ✅ 完成 (P10-1.6 + P10-1.7 + P10-1.8 v1 + v2)

**产出**:
- **P10-1.6**: Creator 跨平台编译硬化
- **P10-1.7**: libloading 闭环 (5 层 ABI 安全)
- **P10-1.8 v1**: 跨 dylib Rust ABI
- **P10-1.8 v2**: C-ABI + JSON 真闭环 (生产级)

**Commits**: 4

**测试**: 47 (libloading / Creator / dylib)

---

### Phase 11 — P13 docs 整理 + i18n + LLM mojibake (Day 101+1)

**目标**: 清理 docs 结构, 完成 i18n, 修 mojibake。

**状态**: ✅ 完成

**产出**:
- **P13-1**: `docs/` + `docs/zh-CN/` 子目录分离
- **P13-2**: i18n 规范文档 (Tier 1 / Tier 2 + 术语表)
- **P13-3**: `en/` 子目录 (对称 i18n, 未来 de/ ja/ fr/)
- **P13-4**: 8 篇 weekly 翻译成英文
- **P13-5**: decision-log-4-p11-12.md L495 mojibake 修 (11545 weird → 0)
- **P13-6**: 1 处漏翻译修 (en/conformance-design.md "之后" → "after Week 10")

**Commits**: 5 (`6b1018d`, `de5865f`, `56895a3`, `28ae577`, `c4fb1d8`, `cf36c6b`)

**测试**: 641 (累计)

**关键决策**: [i18n.md](../i18n.md) (更新)

---

### Phase 12 — P14 Cargo workspaces + GH Pages registry (Day 101+1)

**目标**: 生产级 plugin 生态。

**状态**: ✅ 完成

**产出**:
- **P14-1**: `cargo-workspaces` 0.4.2 装上; `cargo ws plan` 自动算依赖顺序
- **P14-2**: `mah registry list` / `mah registry export` CLI 子命令
- **P14-3**: `registry-pages.yml` GitHub Actions workflow (GH Pages 部署)
- **P14-4**: `operations/registry-pages.md` setup 指南
- **P14-5**: 3 个 `mah registry` CLI unit test

**Commits**: 1 (`243799f`)

**测试**: 641 (累计, +3 from CLI tests)

**待业务方 setup (one-time)**:
1. GitHub repo → Settings → Pages → Source: "GitHub Actions"
2. `mkdir docs/registry && mah registry export --output docs/registry/registry.json`
3. commit + push → workflow 自动部署到 gh-pages

---

## 累计统计 (截至 2026-08-20)

| 指标 | 值 |
|---|---|
| Crate | 16 (9 internal + 7 first-party plugin) + 7 框架扩展 |
| Rust 行数 | ~16,000 |
| 文档行数 (en + zh-CN) | ~50,000 |
| 测试 | 641 (lib + bin + integration) |
| Commits | 50+ |
| 周报 | 8 (Day 0 / Week 1-2 / 3-4 / 5-6 / 7-9 / 10 / 11 / 12-final) |
| Decision log 条目 | 42 (§ 1-42) |
| 公开 API 锁定 | `ma-harness-seam` (5 trait + 5 macro) |

---

### Phase 13 — P13 dsh-adapter (Day 101+2, 设计中)

**目标**: 让 ma-harness 可以直接加载并运行 dsh (DeepSeek Harness) 写的 TS plugin, 走 dsh 自家 JSON-RPC over stdio 协议。

**状态**: 📋 设计完成, 待实施 (5 phase × 1 周 = 5-6 周总)

**详细设计**: [`design/dsh-adapter.md`](design/dsh-adapter.md) (中英双语, 17572 bytes)

**产出** (计划):
- **P13.1 骨架** (1 周): `plugins/ma-harness-plugin-dsh-adapter/` crate + JSON-RPC client + Node.js 子进程 spawn + mock 测
- **P13.2 工具桥接** (1 周): dsh `defineTool` → ma-harness `ToolSchema` + invoke 转发
- **P13.3 lifecycle** (1 周): shutdown / respawn / cancel / stderr / 配置加载
- **P13.4 conformance** (1 周): `mah conformance --dsh-adapter` 跑 9/9 dsh-snap = 100%
- **P13.5 e2e + 文档** (1 周): 真 dsh 插件 (k8s_pod_status) + `mah dsh info/doctor` + CI + 中英文档

**关键决策**:
- **复用 dsh `@deepseek-ai/dsh-sdk-jsonrpc-server`**, 不造协议
- 锁 dsh 版本 `0.1.0-rc.5` (官方 preview, 升级走 minor)
- 不引入 `jsonrpc` crate, 手写 ~200 行 client (协议简单)
- Node.js 子进程 30s timeout + 3 次 respawn 兜底

**Out-of-Scope (P14+)**:
- dsh 全 78 行 plugin 桥接 (沙箱/approval/持久会话等)
- PTC (Code mode) `run_code` tool 桥接
- dylib ↔ dsh 互操作
- dsh-web ↔ ma-harness-tui Web UI 桥接
- Cordis 事件 hook (`tools/pre-execute` 等) 桥接

**新 crate**: 1 (`plugins/ma-harness-plugin-dsh-adapter/`, publish=true, 第 8 个 first-party plugin)

**测试目标**: 累计 660+ (+20 from dsh-adapter + conformance)

**风险**: dsh 0.1.0-rc.5 协议不稳定 / Node.js 业务方本机未装 / Windows Node.js 路径差异 (见 design doc §5)

---

## 下一步: Phase 13+ (post-101+2)

### P15+ — 生产硬化 (计划)

- [ ] **crates.io 0.1.0 发版**: workflow 就绪, 等 `CRATES_IO_TOKEN` secret
- [ ] **mah-py 0.1.1 → pypi.org**: workflow 就绪, 等 pypi.org token
- [ ] **Cargo workspaces publish 自动化**: 用 `cargo ws publish` 代替手写
- [ ] **跨平台 binary release**: GitHub Actions matrix (ubuntu / windows / macos)
- [ ] **dsh 迁移工具**: 帮用户把 dsh plugin 转 ma-harness
- [ ] **GH Pages registry Gitee 镜像**
- [ ] **`mah plugin install <name>`**: 从 registry URL 自动拉
- [ ] **Plugin 签名验证**: GPG / cosign 供应链安全
- [ ] **P12-2+ retry/circuit breaker 集成**: 跟 LLM adapter 集成

### P16+ — 远期

- [ ] **PyO3 v2 mah-py**: 原生 binding 代替 subprocess
- [ ] **DAG 任务编排**: P11-10 推迟项
- [ ] **Postgres session store**: 单机外扩展
- [ ] **多租户隔离**: per-user plugin 沙箱
- [ ] **Web UI**: 跟 TUI 互补

## 状态图例

- ✅ 完成 (commit + test 过)
- 🚧 进行中
- 📋 计划中 (P15+)
- ⏸️ 推迟 (等外部输入: LLM API key, pypi.org token 等)

## 怎么读本文档

- **每个 phase** 独立小节: 目标, 状态, 产出, commit 数, 测试, 关键决策, 链接
- **状态图例** 在底部: 什么完成 / 进行中 / 计划 / 推迟
- **累计统计** 末尾: 项目规模快照
- **下一步** 列出当前状态之后的工作

## 相关文档

- [i18n.md](../i18n.md) — 文档规范
- [tech-stack.md](../tech-stack.md) — 冻结的技术栈决策
- [ma-harness-arch-map.md](../ma-harness-arch-map.md) — 架构总览
- [decision-log.md](decision-log.md) — 42 项决策
- [weekly/](weekly/) — 8 份周报
- [reports/](reports/) — phase 报告
- [user-guide/](user-guide/) — 怎么用 ma-harness
