# AGENTS.md — ma-harness.rs

> 入口文档。任何 AI agent (或新成员) 打开这个仓库,先读这一页。
> 详细决策请看 [`docs/decision-log.md`](docs/decision-log.md)。

---

## 这是什么

`ma-harness.rs` 是一个 **Rust 重写的 AI agent 编排 harness**,设计参考
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) (下文简称 dsh),
但**不是 dsh 的官方 Rust 端口**——是独立的 Rust 实践,跑分/conformance 对齐 dsh
是为了验证设计选择,不是 fork 也不是 port。

Phase 1 (12 周 PoC) 目标:验证 Cordis 元框架的 Rust 表达力 + Protobuf 单协议 +
append-only 日志 + 6 个 first-party 插件,跑 dsh 现有 benchmark 拿到对比数字。

---

## 仓库结构 (PoC 终态, 2026-11 目标)

```
ma-harness.rs/
├── AGENTS.md                ← 你正在看
├── README.md                ← 给人类看的简介
├── Cargo.toml               ← workspace 根
├── docs/                    ← 决策档案 (宪法层,只增不改)
│   ├── decision-log.md
│   ├── code-mode-deferred.md
│   └── tech-stack.md
├── crates/                  ← 6 个核心 package
│   ├── ma_harness_cordis/   ← 元框架 (Cordis-rs,自主重写, **内部 crate**)
│   ├── ma_harness_core/     ← agent loop / session / event
│   ├── ma_harness_seam/     ← Seam 类型 (Phase 1 做 3-4 个)
│   ├── ma_harness_proto/    ← Protobuf 定义 + Prost codegen
│   ├── ma_harness_cli/      ← `mah` 二进制入口
│   └── ma_harness_server/   ← axum + tonic 起的服务
├── plugins/                 ← first-party 插件 (6 个)
│   ├── ma_harness_plugin_bash/
│   ├── ma_harness_plugin_fs/
│   ├── ma_harness_plugin_web/
│   ├── ma_harness_plugin_subagent/
│   ├── ma_harness_plugin_skill/
│   └── ma_harness_plugin_cordis/
├── ma_harness_plugin_macro/ ← proc-macro crate (#[dsh_tool] 等)
├── tests/                   ← 集成测试 + conformance
│   ├── fixtures/            ← 复用 dsh 的 JSONL fixtures
│   └── conformance.rs
├── benches/                 ← criterion benchmark
└── proto/                   ← .proto 源文件
    └── ma_harness/
        └── v1/
            ├── agent.proto
            ├── session.proto
            └── event.proto
```

> **当前 (2026-08-18)**: 只有 `AGENTS.md` + `docs/`。代码从 Week 1 开始 commit。

---

## 关键约定 (必读)

### 命名

- 项目对外: `ma-harness.rs` (仓库名) / `mah` (二进制) / `MA_HARNESS_*` (env) / `~/.ma-harness/` (配置目录)
- Rust crate: `ma_harness_*` (snake_case,跟 Rust 生态一致)
- **内部宏前缀用 `dsh_`**: `#[dsh_tool]` / `#[dsh_listener]` / `#[dsh_handler]`
  / `#[dsh_service]` / `#[dsh_command]` — 跟 DeepSeek Harness 血统挂钩,
  即使项目改名也保留作为"致敬"。
- Protobuf package: `ma_harness.v1` (semver-versioned,为未来开源预留)

### Ctx key 风格 (跟 dsh 划清的关键差异)

dsh 用 camelCase (`agentLoop` / `sessionId`),**我们统一 snake_case**:
`agent_loop` / `session_id` / `model_visible` / `append_only_log` / `seam_manager` /
`plugin_registry` / `sandbox_config` / `proto_channel`。

任何挂到 ctx 的 key 一律 snake_case。Protobuf 字段也 snake_case (Rust 默认)。
跨语言转换层 (例如给前端暴露的) 在 boundary 加,不在存储层加。

### Sandbox

- Linux: `landlock` 0.4
- macOS: `sandbox-exec` (Phase 1 占位,不深度集成)
- Windows: **Phase 1 不做**,代码层 `#[cfg(windows)]` panic with "Phase 2"

### 日志不变量

> **"model-visible means logged"**

任何 model context 里能看到的字符串,都必须在 `SessionEvent` append-only 日志
里存在对应事件。Runtime 强制这个不变量 (漏了 → 编译错误或启动 panic)。

---

## 当前工作流

### 分支模型

- `main` 是 trunk,受保护
- Feature branch 短命 (< 1 周),命名 `feat/<scope>-<short-desc>` 或 `fix/<scope>-<short-desc>`
- PR 走 review,CI 跑 `cargo test --workspace` + `cargo clippy --workspace -- -D warnings`

### Commit message

- Conventional Commits 风格: `feat(cordis): add ctx.extend for snake_case keys`
- 主题行 ≤ 72 字符,body 解释 *why* 不只 *what*
- 重大决策 (新 ADR / 推翻旧决策) 必须在 commit 里 `Refs: docs/decision-log.md#N`

### 不要做的事

- ❌ 直接 push `main`
- ❌ 引 wasmtime / deno_core / 任何 JS 引擎 (见 `docs/code-mode-deferred.md`)
- ❌ 写 camelCase 的 ctx key
- ❌ 把内部决策 / token / 私钥 commit 进去
- ❌ 跳过 `cargo clippy` warning 当 "小事"

---

## 跟 dsh 的关系

- **不 fork, 不 port, 不 mirror**。代码 0% 复用,设计参考。
- **跑分对齐**: 跑 dsh 现有 benchmark,产出 ma-harness 数字,差分对比。
  PoC 期间不要求超 dsh,差不超过 30% 算合格。
- **Conformance**: 复用 dsh 的 JSONL fixtures,在 `tests/fixtures/` 下,
  加一层格式转换 (`tests/conformance.rs`) 让我们的 proto decoder 能读。

详细对比表见 [`docs/decision-log.md#6`](docs/decision-log.md)。

---

## 给 AI agent 的特别说明

- 这个仓库的"宪法"是 `docs/`,不是 `Cargo.toml`。改 crate 结构前先看
  `docs/decision-log.md` 第 2 节范围,改 crate 公开性前先看 §5.1。
- **Crate 公开性是显式属性** (2026-08-18 锁定):
  `ma_harness_cordis` 是**内部 crate**,API 频繁变,改它不要走 ADR;
  `ma_harness_seam` / `ma_harness_plugin_macro` 是**公开**,改一次要更新 spec 文档。
- 写新 ctx key 之前先查 `docs/decision-log.md#4` (snake_case 规则)。
- 加新依赖前先查 `docs/tech-stack.md` 的版本冻结表和"不引入"清单。
- 任何 "能不能做 Code Mode" 类问题,直接引 `docs/code-mode-deferred.md`,
  不要再问。
- 用户 (yifenma) 偏好:中文沟通、关注测试覆盖、关注代码细节、要求
  解释 (不只是抛代码)。回复按这个风格。

---

## 12 周 PoC 里程碑

| Week | 目标 |
|---|---|
| 1-2 | workspace 初始化 + Cordis-rs 最小可用 |
| 3-4 | Cordis-rs 完整 API (ctx / service / listener) |
| 5-6 | 6 个 first-party 插件 |
| 7-9 | 端到端 demo (Default 模式跑通) |
| 10-12 | conformance test + benchmark 对齐 |

Week 9 端到端 demo = **PoC 成功判据**。Week 11 出 conformance 报告 (≥ 95% 通过率)。

---

最后更新: 2026-08-18
