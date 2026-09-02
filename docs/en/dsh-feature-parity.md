# DeepSeek-Harness (dsh) Feature Parity

> **Status of ma-harness.rs against [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)**
>
> Last verified: 2026-09-02 · `ma-harness v0.1.1` (24 crates on crates.io)

This document is the **complete dsh ↔ ma-harness feature comparison** for everyone who wants to know "what does dsh do, and does ma-harness.rs have it?". For high-level status + tables, see [README.md#status-vs-deepseek-harness](../../README.md#status-vs-deepseek-harness) and [README.zh-CN.md#跟-deepseek-harness-对比](../../zh-CN/README.md#跟-deepseek-harness-对比).

dsh's design philosophy: **"Everything is a Plugin"**. The framework is [Cordis](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/cordis-primer.md) — plugins contribute services, typed events, and reversible effects to a shared `ctx`. Every subsystem (model adapter, tool registry, session log, agent loop itself) is a plugin, so each is replaceable from configuration.

ma-harness.rs is a **from-scratch Rust rewrite of dsh v0.1** that targets 100% behavioral parity at the snapshot/fixture level, plus production extensions. The Cordis philosophy is preserved 1:1 in the `ma-harness-cordis` crate.

---

## Table of Contents

1. [dsh Core Packages](#1-dsh-core-packages) — 8 packages that contribute to the Cordis tree
2. [Capability Seams](#2-capability-seams) — 14 swappable capability points
3. [Events](#3-events) — 3 event domains (Session / Agent / Capability)
4. [Turn Flow](#4-turn-flow) — 13-step turn model
5. [Profiles & Bundles](#5-profiles--bundles) — 5 shipped profiles, layer composition
6. [Session Log](#6-session-log) — append-only event log + projection
7. [Tool Execution Pipeline](#7-tool-execution-pipeline) — 4-event waterfall
8. [Distribution Surfaces](#8-distribution-surfaces) — Web UI / ACP / SDK / Python SDK / Headless
9. [CLI Modes](#9-cli-modes) — `dsh web|headless|sdk|sdk-minimal|acp`
10. [Conformance / Behavioral Parity](#10-conformance--behavioral-parity) — 9+7+8 fixture suites
11. [ma-harness Extensions (vs dsh)](#11-ma-harness-extensions-vs-dsh) — features dsh does NOT have
12. [Planned / Not Yet Implemented](#12-planned--not-yet-implemented) — deferred items

---

## 1. dsh Core Packages

dsh ships 8 core packages that contribute services to the Cordis `ctx`. Each row links to the dsh doc and the ma-harness implementation.

| dsh Package | Owns | `ctx` key | dsh doc | ma-harness crate | ma-harness source | Status |
|---|---|---|---|---|---|---|
| `core/session` | append-only `SessionEvent` log + in-memory store | `ctx.sessions` | [subsystems/session.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/session.md) | `ma-harness-core` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) | ✅ done (P11-1) |
| `core/system-prompt` | prompt-section + tool-schema assembly | `ctx.systemPrompt` | [subsystems/system-prompt.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/system-prompt.md) | `ma-harness-core` | [crates/ma-harness-core/src/lib.rs](../../crates/ma-harness-core/src/lib.rs) | ✅ done (P7-1) |
| `core/tools` | scoped tool registry + guarded execution | `ctx.tools` | [subsystems/tools.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/tools.md) | `ma-harness-core` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) | ✅ done (P7-2) |
| `core/agent` | `Agent` interface, live registry, `agent/*` events | `ctx.agents` | [subsystems/core.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/core.md) | `ma-harness-core` | [crates/ma-harness-core/src/agent.rs](../../crates/ma-harness-core/src/agent.rs) | ✅ done (P7-1) |
| `core/agent-loop` | default driver implementing `Agent` | `ctx.agentLoop` | [subsystems/core.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/core.md) | `ma-harness-core` | [crates/ma-harness-core/src/agent.rs#L240](../../crates/ma-harness-core/src/agent.rs#L240) | ✅ done (P7-1) |
| `core/scope` | per-agent scoped-registration primitive | library, no key | [subsystems/scope.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/scope.md) | `ma-harness-cordis` | [crates/ma-harness-cordis/src/disposable.rs](../../crates/ma-harness-cordis/src/disposable.rs) | ✅ done (P7-0) |
| `llm/llm` | message + stream vocabulary + adapter seam | `ctx.llm` | [subsystems/llm-streaming.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/llm-streaming.md) | `ma-harness-model` | [crates/ma-harness-model/src/](../../crates/ma-harness-model/src/) | ✅ done (P8) |
| `webhook/webhook` | authenticated-delivery dispatch + Workspace Session creation | `ctx.webhookRuntime` | [subsystems/webhook.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/webhook.md) | n/a | n/a | ⏳ planned (P15+) |

**Behavioral parity verified**: `crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl` — 9/9 dsh acp-snapshot fixtures pass 100%. See [docs/dsh-benchmark-report.md](../dsh-benchmark-report.md).

---

## 2. Capability Seams

dsh's "Everything is a Plugin" model exposes 14 swappable capability seams. Each seam has a **Service Definition** (interface), **Service Provider** (implementation), and **Consumer** (user, usually a model-facing tool).

| Seam (`ctx` key) | Purpose | dsh doc | ma-harness | Status |
|---|---|---|---|---|
| `ctx.llm` | model provider adapter (Deepseek / OpenAI-compat) | [llm-streaming.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/llm-streaming.md) | `ma-harness-model` 4 backends (OpenAI / Anthropic / Deepseek / Stub) | ✅ done (P8) |
| `ctx.tools` | model-facing tool registry | [subsystems/tools.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/tools.md) | `ma-harness-core::ToolSchema` + `ma-harness-seam` + 8 first-party plugins | ✅ done (P7-2) |
| `ctx.sessions` | session log + lifecycle (create / fork / resume) | [subsystems/session.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/session.md) | `ma-harness-core::SessionStore` (InMemory + Sqlite) | ✅ done (P11-1) |
| `ctx.agents` | live agent registry + `agent/*` events | [subsystems/core.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/core.md) | `ma-harness-cordis::Context` agent lifecycle | ✅ done (P7-1) |
| `ctx.shell` | shell execution backend | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | n/a (deferred — `ma-harness-plugin-bash` runs shell but not via `ctx.shell` seam) | ⚠️ partial |
| `ctx.subprocess` | subprocess spawn (used by shell + PTY + LSP) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `tokio::process::Command` direct | ⚠️ not abstracted |
| `ctx.terminals` | persistent terminal (PTY) backend | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | n/a | ⏳ planned (P15+) |
| `ctx.sandbox` | confine spawned processes (Docker / nsjail / etc.) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `ma-harness-sandbox` (Landlock Linux, Seatbelt macOS, Stub elsewhere) | ✅ done (P10) |
| `ctx.fs` | filesystem provider (local / remote) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `ma-harness-plugin-fs` (local only) | ✅ done (P11-2) |
| `ctx.commands` | human command dispatch (no model turn) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `#[dsh_command]` macro via `ma-harness-plugin-macro` | ✅ done (P7-2) |
| `ctx.jobs` | background work (`job_*` tools) | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | `tokio::spawn` + `ma-harness-plugin-subagent` for delegation | ✅ done (P12-8) |
| `ctx.webhookRuntime` | authenticated webhook delivery → Workspace Session | [subsystems/webhook.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/webhook.md) | n/a | ⏳ planned (P15+) |
| `ctx.systemPrompt` | prompt-section + tool-schema assembly | [subsystems/system-prompt.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/system-prompt.md) | `ma-harness-core::SystemPrompt` | ✅ done (P7-1) |
| `ctx.goals` | manage same-session objectives via `agent/*` | [architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) | n/a (uses `#[dsh_command]` for ad-hoc) | ⏳ planned (P15+) |

**See also**: [architecture.md "Capability seams" section](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md#capability-seams) for dsh's design.

---

## 3. Events

dsh has 3 event domains, each with distinct semantics. Picking the right domain is the first decision in most changes.

| Domain | Semantics | Examples | ma-harness | Status |
|---|---|---|---|---|
| **Session events** | durable facts appended to log + broadcast via `session/event`. Survive reload. | `user/message`, `assistant/chunk`, `tool/result`, `step/start`, `turn/end` | `ma-harness-core::SessionEvent` enum (P11-1) | ✅ done |
| **Agent events** (`agent/*`) | live `Agent` events for in-flight work | `agent/inbox`, `agent/step`, `agent/status`, `agent/request`, `agent/validation`, `agent/continuation` | `ma-harness-cordis` typed event stream | ✅ done (P7-1) |
| **Capability events** | policy + adapters attached to a seam | `fs/*`, `tools/*`, `telemetry/*` | `ma-harness-cordis` event macros + `ma-harness-core` typed events | ✅ done |

**Invariant** (dsh): **"Model-visible means logged"** — anything that reaches a model request must be reconstructable from the log, asserted at runtime. ma-harness preserves this in `ma-harness-core::derive_messages` (replays `SessionEvent` log to construct LLM context).

dsh full event map: [event-producer-consumer.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/event-producer-consumer.md).

---

## 4. Turn Flow

A **step** is one model request + the tools it calls. A **turn** is zero or more steps. The 13-step turn model:

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

| dsh phase | ma-harness equivalent | Source |
|---|---|---|
| `turn/start` / `turn/end` | `ma_harness_core::agent_loop::run_turn` | [crates/ma-harness-core/src/agent.rs#L240](../../crates/ma-harness-core/src/agent.rs#L240) |
| `agent/pre-step` | prompt assembly step in `SystemPrompt::assemble` | [crates/ma-harness-core/src/lib.rs](../../crates/ma-harness-core/src/lib.rs) |
| `agent/request` | `ModelAdapter::complete_stream` | [crates/ma-harness-model/src/](../../crates/ma-harness-model/src/) |
| `llm/stream` | `ma_harness_core::Stream` (futures::Stream<Item=StreamChunk>) | [crates/ma-harness-model/src/lib.rs#L368](../../crates/ma-harness-model/src/lib.rs#L368) |
| `assistant/chunk*` | `SessionEvent::AssistantChunk` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |
| `tools/pre-execute` | `ToolRegistry::validate_args` + approval service | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| `tools/execute` | `ToolRegistry::invoke` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| `tools/post-execute` | `SessionEvent::ToolResult` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |
| `agent/turn-stopping` | cancellation token + drain | [crates/ma-harness-core/src/agent.rs#L240](../../crates/ma-harness-core/src/agent.rs#L240) |

**Behavioral parity verified**: 9 dsh acp-snapshot fixtures pass 100% (matching turn flow). See [docs/dsh-benchmark-report.md](../dsh-benchmark-report.md).

---

## 5. Profiles & Bundles

dsh ships **5 profiles** (named plugin-tree compositions) + 5 bundles (distribution format for Cordis config rows + code).

### 5.1 Shipped Profiles

| Profile | Purpose | ma-harness equivalent | Status |
|---|---|---|---|
| `web` | browser Web UI at `:3080` | `mah serve --port 3080` (TUI + HTTP) | ⚠️ partial — TUI only, no web UI |
| `headless` | one-shot runner, no server | `mah run "task"` (one-shot mode) | ✅ done |
| `sdk` | SDK JSON-RPC server | `mah acp serve` (JSON-RPC 2.0 over stdio) | ✅ done (P11-4) |
| `sdk-minimal` | standalone SDK bundle (no `dsh-base`) | n/a | ⏳ planned |
| `acp` | automation-only ACP server | `mah acp serve` (same as sdk) | ✅ done |

**Layered patches** order (dsh):
1. each bundle in profile's listed order
2. profile's `cordis.patch.yml`
3. home-level patch
4. any `--patch` CLI overlay

ma-harness doesn't have a profile system yet (P12-5 `ma-harness-registry` is the closest). All plugins are loaded from `~/.ma-harness/plugins/` via [`ma-harness-registry`](../../crates/ma-harness-registry/).

### 5.2 Shipped Bundles

| Bundle | Provides | ma-harness equivalent |
|---|---|---|
| `dsh-base` | model adapters, tools, persistence, sandbox, approval, settings, credentials, telemetry | `ma-harness-cordis` (DI) + `ma-harness-core` (session + tool) + `ma-harness-model` (LLM) + `ma-harness-sandbox` |
| `dsh-web-app` | browser application | n/a (TUI only) |
| `dsh-headless` | one-shot runner | `ma-harness-cli` headless mode |
| `dsh-sdk-app` | SDK JSON-RPC server | `ma-harness-cli acp` |
| `dsh-acp-app` | automation-only ACP server | (same as sdk) |
| `dsh-sdk-minimal` | standalone SDK bundle | n/a |

Inspect the dsh boot tree on your machine: `dsh --profile web --dump-config`

---

## 6. Session Log

dsh's `dsh-session-projection` is the canonical pattern: registered units fold committed events incrementally, host consumers read one typed state via `stateOf()`, carriers batch cropped client views via `snapshot()`.

**ma-harness equivalent**: `ma-harness-core::SessionEvent` is the durable event log. `derive_messages()` projects model history from it. Raw `AssistantChunk` events preserve replay + UI fidelity.

- **Source**: [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166)
- **Storage**: `InMemoryStore` (default) + `SqliteStore` (Phase 2.6 persistent)
- **Replay**: every `derive_messages()` call reconstructs the same model history
- **Schema**: `SessionEvent` enum has 13 variants (UserMessage, AssistantChunk, AssistantMessage, ToolCall, ToolResult, StepStart, StepEnd, TurnStart, TurnEnd, AgentInbox, AgentStatus, AgentContinuation, Error)

**Invariant**: "Model-visible means logged" — anything reaching a model request is reconstructable from the log.

dsh full session subsystem: [subsystems/session.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/subsystems/session.md)

---

## 7. Tool Execution Pipeline

dsh's 4-event waterfall:

```text
tool/call        (in-session event, recorded)
  -> tools/pre-execute    (live, can reject / modify args)
    -> tools/execute       (live, runs the actual implementation)
      -> tools/post-execute (live, can transform result)
        -> tool/result    (in-session event, recorded)
```

| Phase | dsh event | ma-harness equivalent | Source |
|---|---|---|---|
| Call recorded | `tool/call` | `SessionEvent::ToolCall` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |
| Pre-execute | `tools/pre-execute` | `ToolRegistry::validate_args` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| Approval | `ctx.approvals` | `ma-harness-cordis::approval::ApprovalService` (P7-2/3) | [crates/ma-harness-cordis/src/approval.rs](../../crates/ma-harness-cordis/src/approval.rs) |
| Execute | `tools/execute` | `ToolRegistry::invoke` | [crates/ma-harness-core/src/tool.rs](../../crates/ma-harness-core/src/tool.rs) |
| Post-execute | `tools/post-execute` | (result transformation in `ToolRegistry::invoke`) | (same) |
| Result recorded | `tool/result` | `SessionEvent::ToolResult` | [crates/ma-harness-core/src/event.rs#L166](../../crates/ma-harness-core/src/event.rs#L166) |

dsh full pipeline: [tool-execution-pipeline.md](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture/tool-execution-pipeline.md)

---

## 8. Distribution Surfaces

dsh has 5 distribution surfaces. ma-harness implements 3 of them.

| Surface | dsh | ma-harness | Status |
|---|---|---|---|
| **Web UI** | browser app at `:3080` | n/a (TUI only) | ⏳ planned (P15+) |
| **ACP (JSON-RPC 2.0 stdio)** | `dsh --profile acp` | `mah acp serve` | ✅ done (P11-4) — interoperable |
| **Python SDK** | `pip install deepseek-harness-sdk` | `pip install mah-py` | ✅ done (P11-3) |
| **Node.js SDK** | `npx @deepseek-ai/dsh web` | n/a | n/a (Rust-native) |
| **Headless runner** | `dsh --profile headless` | `mah run "task"` | ✅ done |
| **Bundles for distribution** | `package.json#dsh.bundle` | `Cargo.toml#ma-harness-registry.bundle` (P12-5) | ✅ done |

dsh's ACP and ma-harness's ACP are **interoperable** because both speak JSON-RPC 2.0 over stdio with the same message schema (verified by `crates/ma-harness-conformance/fixtures/dsh-snap-converted/`).

---

## 9. CLI Modes

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
mah version                            # version
mah run "task"                         # one-shot (headless equivalent)
mah acp serve                          # ACP JSON-RPC 2.0 stdio (acp + sdk profiles)
mah plugins                            # list installed plugins
mah plugin install <name>              # install plugin
mah load-plugin <name>                 # load plugin (in-process)
mah conformance --fixtures path.jsonl  # replay dsh-format fixtures
mah info / mah doctor                  # plugin health checks (P13.5)
mah tui                                # ratatui TUI dashboard
mah open-api export                    # generate OpenAPI spec (HTTP)
```

| dsh command | ma-harness equivalent | Notes |
|---|---|---|
| `dsh web` | `mah tui` (no web) | TUI instead of web; web planned |
| `dsh --profile headless <task>` | `mah run <task>` | same semantics |
| `dsh --profile sdk` | `mah acp serve` | JSON-RPC 2.0 stdio |
| `dsh --profile acp` | `mah acp serve` | same |
| `dsh plugin <cmd>` | `mah plugin <cmd>` | similar (registry-based) |
| `dsh --dump-config` | n/a (registry-based config) | different model |

---

## 10. Conformance / Behavioral Parity

dsh v0.1 ships 4 conformance suites. ma-harness runs 3 of them + 1 own suite (smoke).

| Suite | dsh v0.1 | ma-harness.rs | Status | Fixture file |
|---|---|---|---|---|
| **dsh acp-snapshot** (9 fixtures) | 100% | **100% (9/9)** | ✅ parity | [crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl](../../crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl) |
| **dsh_synthetic** (7 fixtures) | n/a | **100% (7/7)** | ✅ parity | [crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_synthetic.jsonl](../../crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_synthetic.jsonl) |
| **smoke** (8 fixtures) | n/a | 62.5% (5/8) | ✅ by design (3 expected failures) | [crates/ma-harness-conformance/fixtures/smoke.jsonl](../../crates/ma-harness-conformance/fixtures/smoke.jsonl) |
| Terminal Bench 2.1 | 87.9% | not run | ⏳ business-driven (P11-2.5+) | external (Deepseek API key + dataset) |
| Toolathlon-Verified | 74.1% | not run | ⏳ business-driven | external |
| DSBench-FullStack | 71.1% | not run | ⏳ business-driven | external |

End-to-end verification:
```bash
$ mah.exe conformance --fixtures crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl
Loaded 9 fixtures from dsh_snap.jsonl
Conformance: 9 / 9 passed (100.0%) in 1ms
```

Full report: [docs/zh-CN/reports/dsh-benchmark-report.md](../zh-CN/reports/dsh-benchmark-report.md)

---

## 11. ma-harness Extensions (vs dsh)

ma-harness.rs is **not just a port** — it adds production extensions that dsh does not have.

| Extension | Description | Phase | Doc |
|---|---|---|---|
| **Plugin Registry** (npm-style) | central registry to publish / search / export / merge plugin packages | P11-6 / P12-5 | [crates/ma-harness-registry/](../../crates/ma-harness-registry/) |
| **Bundle** (lockfile install) | reproducible plugin install with version pinning | P11-8 / P12-7 | [crates/ma-harness-bundle/](../../crates/ma-harness-bundle/) |
| **Vibe Coding Artifact viewer** | auto-detect + render 10 artifact kinds (HTML, SVG, JSON, Mermaid, etc.) | P11-7 | [crates/ma-harness-artifact/](../../crates/ma-harness-artifact/) |
| **DAG orchestration** | Kahn topological sort + short-circuit on failure | P12-9 | [crates/ma-harness-dag/](../../crates/ma-harness-dag/) |
| **Multi-modal vision** | OpenAI + Anthropic vision adapters | P11-5/9, P12-8 | [crates/ma-harness-model/src/multimodal.rs](../../crates/ma-harness-model/src/multimodal.rs) |
| **Retry + Circuit Breaker** | exponential backoff + jitter | P12-2 | [crates/ma-harness-model/src/retry.rs](../../crates/ma-harness-model/src/retry.rs) |
| **Wasm sandbox** (Code Mode) | wasmtime + 4-layer defense (fuel / epoch / memory+table / fs whitelist) | P2.6 | [crates/ma-harness-code/](../../crates/ma-harness-code/) |
| **Landlock sandbox** (Linux kernel) | ABI V1 (kernel ≥ 5.13) + Seatbelt macOS + Stub | P10 | [crates/ma-harness-sandbox/](../../crates/ma-harness-sandbox/) |
| **TUI dashboard** | ratatui + multi-panel + approval flow | P3.9 | [crates/ma-harness-tui/](../../crates/ma-harness-tui/) |
| **HTTP server** (salvo) | OpenAPI export, SSE, gRPC | P6 | [crates/ma-harness-server/](../../crates/ma-harness-server/) |
| **4 LLM backends** | OpenAI / Anthropic / Deepseek / Stub (dsh: 1) | P8 | [crates/ma-harness-model/src/](../../crates/ma-harness-model/src/) |
| **crates.io publish** | 24 crates at 0.1.1 on crates.io | P12-5 | [docs/en/release-process.md](release-process.md) (planned) |
| **dsh-adapter** (P13) | load dsh (TypeScript) plugins directly via JSON-RPC over stdio — no port needed | P13 | [design/dsh-adapter.md](design/dsh-adapter.md) |

---

## 12. Planned / Not Yet Implemented

| Item | Why deferred | Plan |
|---|---|---|
| Web UI | dsh has browser app; ma-harness TUI only | P15+ (low priority) |
| `ctx.shell` seam | not abstracted (bash plugin runs shell directly) | P14+ (refactor) |
| `ctx.subprocess` seam | using `tokio::process::Command` directly | P14+ (refactor) |
| `ctx.terminals` (PTY) backend | dsh supports persistent terminals | P15+ |
| `ctx.webhookRuntime` | not implemented | P15+ |
| `ctx.goals` (session objectives) | not implemented | P15+ |
| Terminal Bench 2.1 (87.9%) | needs real LLM API key + dataset | business-driven, P11-2.5+ |
| Toolathlon-Verified (74.1%) | external | business-driven |
| DSBench-FullStack (71.1%) | external | business-driven |
| `dsh → ma-harness` migration tool | replaced by **dsh-adapter** (load dsh plugins directly) | done via P13 |
| ACP v3 (when dsh ships) | wait for dsh v0.2 protocol spec | when dsh ships |
| WASI preview2 support | wasmtime 28+ release | P15+ (low priority) |
| PyO3 v2 (replace subprocess) | v1 (subprocess) works, v2 gives 10-100x speedup | P15+ (low priority) |
| Cross-platform binary releases (Win/Mac/Linux) | need cross-compile + GH release workflow | P15+ |
| Plugin Registry public deployment | P12-5 `export` works, need GH Pages hosting | P15+ (30-min task) |
| mah-py → pypi.org production | currently on test.pypi.org only | business verifies test first |

---

## See Also

- [README.md#status-vs-deepseek-harness](../../README.md#status-vs-deepseek-harness) — high-level status table
- [docs/dsh-benchmark-report.md](../dsh-benchmark-report.md) — 9/9 dsh acp-snapshot parity
- [docs/en/design/dsh-adapter.md](design/dsh-adapter.md) — dsh TypeScript plugin adapter (P13)
- [docs/architecture.md (dsh)](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) — dsh's own architecture doc
- [docs/en/ma-harness-arch-map.md](../ma-harness-arch-map.md) — ma-harness dependency map
- [crates/ma-harness-conformance/design.md](../../crates/ma-harness-conformance/README.md) — conformance framework

---

**Last updated**: 2026-09-02 · maintained as part of the v0.1.x release line
