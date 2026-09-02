# DeepSeek-Harness (dsh) 功能对比

> **ma-harness.rs 跟 [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) 的功能对比**
>
> 最后验证: 2026-09-02 · `ma-harness v0.1.1` (24 个 crate 已发 crates.io)

本文件是 **dsh ↔ ma-harness 详尽功能对比**，给想知道 "dsh 有啥功能，ma-harness 有没有" 的人。简要表格见 [README.md#status-vs-deepseek-harness](../../README.md#status-vs-deepseek-harness) 和 [README.zh-CN.md#跟-deepseek-harness-对比](../../zh-CN/README.md#跟-deepseek-harness-对比)。

dsh 的设计哲学: **"Everything is a Plugin"**。框架核心是 [Cordis](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/cordis-primer.md) — plugins 贡献 services + typed events + reversible effects 到共享 `ctx`。每个子系统 (model adapter / tool registry / session log / agent loop 本身) 都是 plugin，所以每个都可替换。

ma-harness.rs 是 dsh v0.1 的**从头 Rust 重写**，目标 100% 行为对齐 (snapshot/fixture 层) + 生产级扩展。Cordis 哲学 1:1 保存在 `ma-harness-cordis` crate。

---

## 目录

1. [dsh 核心包](#1-dsh-核心包) — 8 个贡献到 Cordis tree 的包
2. [Capability Seams](#2-capability-seams) — 14 个可替换能力点
3. [事件](#3-事件) — 3 个事件域 (Session / Agent / Capability)
4. [Turn Flow](#4-turn-flow) — 13 步 turn 模型
5. [Profiles & Bundles](#5-profiles--bundles) — 5 个 shipped profiles, 层级组合
6. [Session Log](#6-session-log) — append-only event log + projection
7. [工具执行管道](#7-工具执行管道) — 4-event waterfall
8. [分发面](#8-分发面) — Web UI / ACP / SDK / Python SDK / Headless
9. [CLI 模式](#9-cli-模式) — `dsh web|headless|sdk|sdk-minimal|acp`
10. [Conformance / 行为对齐](#10-conformance--行为对齐) — 9+7+8 fixture suites
11. [ma-harness 扩展 (vs dsh)](#11-ma-harness-扩展-vs-dsh) — dsh 没有的功能
12. [计划 / 暂未实现](#12-计划--暂未实现) — 暂缓项

---

## 1. dsh 核心包

dsh 有 8 个核心包贡献 services 到 Cordis `ctx`。每行链接到 dsh 文档和 ma-harness 实现。

| dsh 包 | Owns | `ctx` key | dsh 文档 | ma-harness crate | ma-harness 源码 | Status |
|---|---|---|---|---|---|---|
| `core/session` | append-only `SessionEvent` log + in-memory store | `ctx.sessions` | [subsystems/session.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/session.md) | `ma-harness-core` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) | ✅ done (P11-1) |
| `core/system-prompt` | prompt-section + tool-schema 装配 | `ctx.systemPrompt` | [subsystems/system-prompt.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/system-prompt.md) | `ma-harness-core` | [crates/ma-harness-core/src/lib.rs](../../crates/ma-harness-core/src/lib.rs) | ✅ done (P7-1) |
| `core/tools` | scoped tool registry + 受保护执行 | `ctx.tools` | [subsystems/tools.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/tools.md) | `ma-harness-core` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) | ✅ done (P7-2) |
| `core/agent` | `Agent` interface, live registry, `agent/*` events | `ctx.agents` | [subsystems/core.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/core.md) | `ma-harness-core` | [crates/ma-harness-core/src/agent.rs](../../crates/ma-harness-core/src/agent.rs) | ✅ done (P7-1) |
| `core/agent-loop` | 默认 driver 实现 `Agent` | `ctx.agentLoop` | [subsystems/core.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/core.md) | `ma-harness-core` | [crates/ma-harness-core/src/agent.rs#L240](../../crates/ma-harness-core/src/agent.rs#L240) | ✅ done (P7-1) |
| `core/scope` | per-agent scoped-registration primitive | library, no key | [subsystems/scope.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/scope.md) | `ma-harness-cordis` | [crates/ma-harness-cordis/src/disposable.rs](../../crates/ma-harness-cordis/src/disposable.rs) | ✅ done (P7-0) |
| `llm/llm` | message + stream vocab + adapter seam | `ctx.llm` | [subsystems/llm-streaming.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/llm-streaming.md) | `ma-harness-model` | [crates/ma-harness-model/src/](../../crates/ma-harness-model/src/) | ✅ done (P8) |
| `webhook/webhook` | authenticated-delivery dispatch + Workspace Session 创建 | `ctx.webhookRuntime` | [subsystems/webhook.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/webhook.md) | n/a | n/a | ⏳ planned (P15+) |

**行为对齐已验证**: `crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl` — 9/9 dsh acp-snapshot fixtures 通过 100%。见 [docs/zh-CN/reports/dsh-benchmark-report.md](../zh-CN/reports/dsh-benchmark-report.md)。

---

## 2. Capability Seams

dsh 的 "Everything is a Plugin" 模型暴露 14 个可替换能力 seam。每个 seam 有 **Service Definition** (interface), **Service Provider** (实现), 和 **Consumer** (用户，通常是 model-facing tool)。

| Seam (`ctx` key) | 用途 | dsh 文档 | ma-harness | Status |
|---|---|---|---|---|
| `ctx.llm` | model provider adapter (Deepseek / OpenAI-compat) | [llm-streaming.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/llm-streaming.md) | `ma-harness-model` 4 个 backend (OpenAI / Anthropic / Deepseek / Stub) | ✅ done (P8) |
| `ctx.tools` | model-facing tool registry | [subsystems/tools.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/tools.md) | `ma-harness-core::ToolSchema` + `ma-harness-seam` + 8 个 first-party plugin | ✅ done (P7-2) |
| `ctx.sessions` | session log + lifecycle (create / fork / resume) | [subsystems/session.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/session.md) | `ma-harness-core::SessionStore` (InMemory + Sqlite) | ✅ done (P11-1) |
| `ctx.agents` | live agent registry + `agent/*` events | [subsystems/core.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/core.md) | `ma-harness-cordis::Context` agent lifecycle | ✅ done (P7-1) |
| `ctx.shell` | shell execution backend | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | n/a (deferred — `ma-harness-plugin-bash` 跑 shell 但不通过 `ctx.shell` seam) | ⚠️ partial |
| `ctx.subprocess` | subprocess spawn (used by shell + PTY + LSP) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `tokio::process::Command` 直接用 | ⚠️ 未抽象 |
| `ctx.terminals` | persistent terminal (PTY) backend | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | n/a | ⏳ planned (P15+) |
| `ctx.sandbox` | 隔离 spawn 的进程 (Docker / nsjail / etc.) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `ma-harness-sandbox` (Landlock Linux, Seatbelt macOS, Stub elsewhere) | ✅ done (P10) |
| `ctx.fs` | filesystem provider (local / remote) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `ma-harness-plugin-fs` (local only) | ✅ done (P11-2) |
| `ctx.commands` | human command dispatch (无 model turn) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `#[dsh_command]` macro via `ma-harness-plugin-macro` | ✅ done (P7-2) |
| `ctx.jobs` | background work (`job_*` tools) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `tokio::spawn` + `ma-harness-plugin-subagent` 委托 | ✅ done (P12-8) |
| `ctx.webhookRuntime` | authenticated webhook delivery → Workspace Session | [subsystems/webhook.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/webhook.md) | n/a | ⏳ planned (P15+) |
| `ctx.systemPrompt` | prompt-section + tool-schema 装配 | [subsystems/system-prompt.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/system-prompt.md) | `ma-harness-core::SystemPrompt` | ✅ done (P7-1) |
| `ctx.goals` | manage same-session objectives via `agent/*` | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | n/a (用 `#[dsh_command]` ad-hoc) | ⏳ planned (P15+) |

**也见**: [architecture.md "Capability seams" 章节](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md#capability-seams) 关于 dsh 的设计。

---

## 3. 事件

dsh 有 3 个事件域，每个有不同语义。选对域是大多数改动的第一决定。

| 域 | 语义 | 示例 | ma-harness | Status |
|---|---|---|---|---|
| **Session events** | durable facts 追加到 log + 通过 `session/event` 广播。跨 reload 存活。 | `user/message`, `assistant/chunk`, `tool/result`, `step/start`, `turn/end` | `ma-harness-core::SessionEvent` enum (P11-1) | ✅ done |
| **Agent events** (`agent/*`) | live `Agent` events for in-flight work | `agent/inbox`, `agent/step`, `agent/status`, `agent/request`, `agent/validation`, `agent/continuation` | `ma-harness-cordis` typed event stream | ✅ done (P7-1) |
| **Capability events** | policy + adapters 附到 seam | `fs/*`, `tools/*`, `telemetry/*` | `ma-harness-cordis` event macros + `ma-harness-core` typed events | ✅ done |

**不变量** (dsh): **"Model-visible means logged"** — 任何到达 model request 的东西必须能从 log 重构，运行时断言。ma-harness 在 `ma-harness-core::derive_messages` 保留这个 (replay `SessionEvent` log 构造 LLM context)。

dsh 完整 event map: [event-producer-consumer.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/event-producer-consumer.md)。

---

## 4. Turn Flow

**step** 是一次 model request + 它调用的 tools。**turn** 是 0+ 个 steps。13 步 turn 模型：

```text
turn/start
  claim next-step input + one queued message
  assemble prompt sections + tool schemas
  -> agent/pre-step                  reject | enter(messages, startsRequestSeries?)
     reject, or empty first claim   -> close turn with no step
     step/start
     append entered messages as user/message
     derive model history from log
     agent/request -> llm/stream -> assistant/chunk* -> assistant/message
     tool/call* -> tools/pre-execute -> tools/execute -> tools/post-execute -> tool/result*
     step/end
     tools owe another request, or next-step input arrived -> next step
  -> agent/turn-stopping
turn/end
```

| dsh phase | ma-harness 等价 | 源码 |
|---|---|---|
| `turn/start` / `turn/end` | `ma_harness_core::agent_loop::run_turn` | [crates/ma-harness-core/src/agent.rs#L240](../../crates/ma-harness-core/src/agent.rs#L240) |
| `agent/pre-step` | prompt 装配步骤在 `SystemPrompt::assemble` | [crates/ma-harness-core/src/lib.rs](../../crates/ma-harness-core/src/lib.rs) |
| `agent/request` | `ModelAdapter::complete_stream` | [crates/ma-harness-model/src/](../../crates/ma-harness-model/src/) |
| `llm/stream` | `ma_harness_core::Stream` (futures::Stream<Item=StreamChunk>) | [crates/ma-harness-model/src/lib.rs#L368](../../crates/ma-harness-model/src/lib.rs#L368) |
| `assistant/chunk*` | `SessionEvent::AssistantChunk` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |
| `tools/pre-execute` | `ToolRegistry::validate_args` + approval service | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| `tools/execute` | `ToolRegistry::invoke` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| `tools/post-execute` | (result transformation in `ToolRegistry::invoke`) | (同上) |
| `agent/turn-stopping` | cancellation token + drain | [crates/ma-harness-core/src/agent.rs#L240](../../crates/ma-harness-core/src/agent.rs#L240) |

**行为对齐已验证**: 9 个 dsh acp-snapshot fixtures 通过 100% (匹配 turn flow)。见 [docs/zh-CN/reports/dsh-benchmark-report.md](../zh-CN/reports/dsh-benchmark-report.md)。

---

## 5. Profiles & Bundles

dsh 发了 **5 个 profiles** (命名 plugin-tree 组合) + 5 个 bundles (Cordis config rows + 代码的 distribution 格式)。

### 5.1 Shipped Profiles

| Profile | 用途 | ma-harness 等价 | Status |
|---|---|---|---|
| `web` | 浏览器 Web UI 在 `:3080` | `mah serve --port 3080` (TUI + HTTP) | ⚠️ partial — 只有 TUI, 无 web UI |
| `headless` | one-shot runner, 无 server | `mah run "task"` (one-shot mode) | ✅ done |
| `sdk` | SDK JSON-RPC server | `mah acp serve` (JSON-RPC 2.0 over stdio) | ✅ done (P11-4) |
| `sdk-minimal` | standalone SDK bundle (无 `dsh-base`) | n/a | ⏳ planned |
| `acp` | automation-only ACP server | `mah acp serve` (同 sdk) | ✅ done |

**分层 patch** 顺序 (dsh):
1. profile 列出的每个 bundle 按顺序
2. profile 的 `cordis.patch.yml`
3. home-level patch
4. 任何 `--patch` CLI overlay

ma-harness 还没有 profile 系统 (P12-5 `ma-harness-registry` 最接近)。所有 plugin 从 `~/.ma-harness/plugins/` 通过 [`ma-harness-registry`](../../crates/ma-harness-registry/) 加载。

### 5.2 Shipped Bundles

| Bundle | 提供 | ma-harness 等价 |
|---|---|---|
| `dsh-base` | model adapters, tools, persistence, sandbox, approval, settings, credentials, telemetry | `ma-harness-cordis` (DI) + `ma-harness-core` (session + tool) + `ma-harness-model` (LLM) + `ma-harness-sandbox` |
| `dsh-web-app` | browser application | n/a (只有 TUI) |
| `dsh-headless` | one-shot runner | `ma-harness-cli` headless mode |
| `dsh-sdk-app` | SDK JSON-RPC server | `ma-harness-cli acp` |
| `dsh-acp-app` | automation-only ACP server | (同 sdk) |
| `dsh-sdk-minimal` | standalone SDK bundle | n/a |

查看你机器的 dsh 启动 tree: `dsh --profile web --dump-config`

---

## 6. Session Log

dsh 的 `dsh-session-projection` 是规范模式: 注册的单元增量地 fold 已 committed events, host consumers 通过 `stateOf()` 读一个 typed state, carriers 通过 `snapshot()` 批 cropped client views。

**ma-harness 等价**: `ma-harness-core::SessionEvent` 是 durable event log。`derive_messages()` 从它 project model history。原始 `AssistantChunk` events 保留 replay + UI 真实性。

- **源码**: [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166)
- **存储**: `InMemoryStore` (默认) + `SqliteStore` (Phase 2.6 持久)
- **Replay**: 每次 `derive_messages()` 调用重构相同的 model history
- **Schema**: `SessionEvent` enum 有 13 个变体 (UserMessage, AssistantChunk, AssistantMessage, ToolCall, ToolResult, StepStart, StepEnd, TurnStart, TurnEnd, AgentInbox, AgentStatus, AgentContinuation, Error)

**不变量**: "Model-visible means logged" — 任何到达 model request 的东西都能从 log 重构。

dsh 完整 session subsystem: [subsystems/session.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/session.md)

---

## 7. 工具执行管道

dsh 的 4-event waterfall：

```text
tool/call        (in-session event, recorded)
  -> tools/pre-execute    (live, can reject / modify args)
    -> tools/execute       (live, runs the actual implementation)
      -> tools/post-execute (live, can transform result)
        -> tool/result    (in-session event, recorded)
```

| Phase | dsh event | ma-harness 等价 | 源码 |
|---|---|---|---|
| 记录 call | `tool/call` | `SessionEvent::ToolCall` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |
| Pre-execute | `tools/pre-execute` | `ToolRegistry::validate_args` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| Approval | `ctx.approvals` | `ma-harness-cordis::approval::ApprovalService` (P7-2/3) | [crates/ma-harness-cordis/src/approval.rs](../../crates/ma-harness-cordis/src/approval.rs) |
| Execute | `tools/execute` | `ToolRegistry::invoke` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| Post-execute | `tools/post-execute` | (result transformation in `ToolRegistry::invoke`) | (同上) |
| 记录 result | `tool/result` | `SessionEvent::ToolResult` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |

dsh 完整管道: [tool-execution-pipeline.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/tool-execution-pipeline.md)

---

## 8. 分发面

dsh 有 5 个分发面。ma-harness 实现了 3 个。

| Surface | dsh | ma-harness | Status |
|---|---|---|---|
| **Web UI** | browser app 在 `:3080` | n/a (只有 TUI) | ⏳ planned (P15+) |
| **ACP (JSON-RPC 2.0 stdio)** | `dsh --profile acp` | `mah acp serve` | ✅ done (P11-4) — 互通 |
| **Python SDK** | `pip install deepseek-harness-sdk` | `pip install mah-py` | ✅ done (P11-3) |
| **Node.js SDK** | `npx @deepseek-ai/dsh web` | n/a | n/a (Rust-native) |
| **Headless runner** | `dsh --profile headless` | `mah run "task"` | ✅ done |
| **Bundles 分发** | `package.json#dsh.bundle` | `Cargo.toml#ma-harness-registry.bundle` (P12-5) | ✅ done |

dsh 的 ACP 和 ma-harness 的 ACP **互通**，因为都讲 JSON-RPC 2.0 over stdio 用同样的 message schema (由 `crates/ma-harness-conformance/fixtures/dsh-snap-converted/` 验证)。

---

## 9. CLI 模式

dsh CLI:

```sh
npx @deepseek-ai/dsh web               # default = --profile web
dsh --profile headless                # one-shot
dsh --profile sdk                     # SDK JSON-RPC server
dsh --profile sdk-minimal             # standalone SDK bundle
dsh --profile acp                      # automation-only ACP server
dsh --profile <custom> --patch foo.yml --patch bar.yml
```

ma-harness CLI (`mah`):

```sh
mah version                            # 版本
mah run "task"                         # one-shot (headless 等价)
mah acp serve                          # ACP JSON-RPC 2.0 stdio (acp + sdk profiles)
mah plugins                            # 列出已装 plugin
mah plugin install <name>              # 装 plugin
mah load-plugin <name>                 # 加载 plugin (in-process)
mah conformance --fixtures path.jsonl  # replay dsh-format fixtures
mah info / mah doctor                  # plugin health checks (P13.5)
mah tui                                # ratatui TUI dashboard
mah open-api export                    # 生成 OpenAPI spec (HTTP)
```

| dsh 命令 | ma-harness 等价 | 备注 |
|---|---|---|
| `dsh web` | `mah tui` (无 web) | TUI 代替 web; web planned |
| `dsh --profile headless <task>` | `mah run <task>` | 同样语义 |
| `dsh --profile sdk` | `mah acp serve` | JSON-RPC 2.0 stdio |
| `dsh --profile acp` | `mah acp serve` | 同上 |
| `dsh plugin <cmd>` | `mah plugin <cmd>` | 类似 (registry-based) |
| `dsh --dump-config` | n/a (registry-based config) | 不同模型 |

---

## 10. Conformance / 行为对齐

dsh v0.1 有 4 个 conformance suites。ma-harness 跑 3 个 + 1 个自己的 (smoke)。

| Suite | dsh v0.1 | ma-harness.rs | Status | Fixture 文件 |
|---|---|---|---|---|
| **dsh acp-snapshot** (9 fixtures) | 100% | **100% (9/9)** | ✅ parity | [crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl](../../crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl) |
| **dsh_synthetic** (7 fixtures) | n/a | **100% (7/7)** | ✅ parity | [crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_synthetic.jsonl](../../crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_synthetic.jsonl) |
| **smoke** (8 fixtures) | n/a | 62.5% (5/8) | ✅ by design (3 个预期 fail) | [crates/ma-harness-conformance/fixtures/smoke.jsonl](../../crates/ma-harness-conformance/fixtures/smoke.jsonl) |
| Terminal Bench 2.1 | 87.9% | not run | ⏳ business-driven (P11-2.5+) | 外部 (Deepseek API key + dataset) |
| Toolathlon-Verified | 74.1% | not run | ⏳ business-driven | 外部 |
| DSBench-FullStack | 71.1% | not run | ⏳ business-driven | 外部 |

端到端验证：
```bash
$ mah.exe conformance --fixtures crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl
Loaded 9 fixtures from dsh_snap.jsonl
Conformance: 9 / 9 passed (100.0%) in 1ms
```

完整报告: [docs/zh-CN/reports/dsh-benchmark-report.md](../zh-CN/reports/dsh-benchmark-report.md)

---

## 11. ma-harness 扩展 (vs dsh)

ma-harness.rs **不只 port** — 加了 dsh 没有的生产级扩展。

| 扩展 | 描述 | Phase | 文档 |
|---|---|---|---|
| **Plugin Registry** (npm-style) | 中央 registry 来 publish / search / export / merge plugin packages | P11-6 / P12-5 | [crates/ma-harness-registry/](../../crates/ma-harness-registry/) |
| **Bundle** (lockfile install) | 可复现的 plugin install with version pinning | P11-8 / P12-7 | [crates/ma-harness-bundle/](../../crates/ma-harness-bundle/) |
| **Vibe Coding Artifact viewer** | auto-detect + render 10 种 artifact (HTML, SVG, JSON, Mermaid 等) | P11-7 | [crates/ma-harness-artifact/](../../crates/ma-harness-artifact/) |
| **DAG 编排** | Kahn topological sort + 失败短路 | P12-9 | [crates/ma-harness-dag/](../../crates/ma-harness-dag/) |
| **Multi-modal vision** | OpenAI + Anthropic vision adapters | P11-5/9, P12-8 | [crates/ma-harness-model/src/multimodal.rs](../../crates/ma-harness-model/src/multimodal.rs) |
| **Retry + Circuit Breaker** | exponential backoff + jitter | P12-2 | [crates/ma-harness-model/src/retry.rs](../../crates/ma-harness-model/src/retry.rs) |
| **Wasm sandbox** (Code Mode) | wasmtime + 4 层防御 (fuel / epoch / memory+table / fs whitelist) | P2.6 | [crates/ma-harness-code/](../../crates/ma-harness-code/) |
| **Landlock sandbox** (Linux kernel) | ABI V1 (kernel ≥ 5.13) + Seatbelt macOS + Stub | P10 | [crates/ma-harness-sandbox/](../../crates/ma-harness-sandbox/) |
| **TUI dashboard** | ratatui + multi-panel + approval flow | P3.9 | [crates/ma-harness-tui/](../../crates/ma-harness-tui/) |
| **HTTP server** (salvo) | OpenAPI export, SSE, gRPC | P6 | [crates/ma-harness-server/](../../crates/ma-harness-server/) |
| **4 个 LLM backend** | OpenAI / Anthropic / Deepseek / Stub (dsh: 1) | P8 | [crates/ma-harness-model/src/](../../crates/ma-harness-model/src/) |
| **crates.io publish** | 24 个 crate 0.1.1 已发 crates.io | P12-5 | [docs/en/release-process.md](../en/release-process.md) (计划) |
| **dsh-adapter** (P13) | 通过 JSON-RPC over stdio 直接加载 dsh (TypeScript) plugins — 不需要 port | P13 | [design/dsh-adapter.md](design/dsh-adapter.md) |

---

## 12. 计划 / 暂未实现

| Item | 为何暂缓 | 计划 |
|---|---|---|
| Web UI | dsh 有 browser app; ma-harness 只有 TUI | P15+ (低优先) |
| `ctx.shell` seam | 没抽象 (bash plugin 直接跑 shell) | P14+ (refactor) |
| `ctx.subprocess` seam | 直接用 `tokio::process::Command` | P14+ (refactor) |
| `ctx.terminals` (PTY) backend | dsh 支持 persistent terminals | P15+ |
| `ctx.webhookRuntime` | 未实现 | P15+ |
| `ctx.goals` (session objectives) | 未实现 | P15+ |
| Terminal Bench 2.1 (87.9%) | 需真 LLM API key + dataset | 业务方驱动, P11-2.5+ |
| Toolathlon-Verified (74.1%) | 外部 | 业务方驱动 |
| DSBench-FullStack (71.1%) | 外部 | 业务方驱动 |
| `dsh → ma-harness` 迁移工具 | 改为 **dsh-adapter** (直接加载 dsh plugins) | 通过 P13 完成 |
| ACP v3 (dsh 发布后) | 等 dsh v0.2 协议 spec | dsh 发布时 |
| WASI preview2 支持 | 需 wasmtime 28+ 发布 | P15+ (低优先) |
| PyO3 v2 (替换 subprocess) | v1 (subprocess) 可用, v2 提速 10-100x | P15+ (低优先) |
| Cross-platform binary 发布 (Win/Mac/Linux) | 需 cross-compile + GH release workflow | P15+ |
| Plugin Registry 公开部署 | P12-5 `export` 可用, 需 GH Pages hosting | P15+ (30-min 任务) |
| mah-py → pypi.org production | 当前只在 test.pypi.org | 业务方先验 test |

---

## 参见

- [README.zh-CN.md#跟-deepseek-harness-对比](../../README.zh-CN.md#跟-deepseek-harness-对比) — 简要状态表格
- [docs/zh-CN/reports/dsh-benchmark-report.md](../zh-CN/reports/dsh-benchmark-report.md) — 9/9 dsh acp-snapshot parity
- [docs/zh-CN/design/dsh-adapter.md](design/dsh-adapter.md) — dsh TypeScript plugin adapter (P13)
- [docs/architecture.md (dsh)](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) — dsh 自己的 architecture 文档
- [docs/en/ma-harness-arch-map.md](../../docs/en/ma-harness-arch-map.md) — ma-harness 依赖图

---

**最后更新**: 2026-09-02 · 作为 v0.1.x 发布线的一部分维护
