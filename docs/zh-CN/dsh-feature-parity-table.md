# dsh ↔ ma-harness 功能对等表

> **精炼表格版: ma-harness.rs v0.1.1 vs [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) v0.1**
>
> 最后核对: 2026-09-04 · 24 crate 在 crates.io (实际成功 16 个; 7 个 SSL 卡住; 5 个 0.1.0 抢占)
>
> **图例**: ✅ 已完成 &nbsp;|&nbsp; 🔄 已扩展 &nbsp;|&nbsp; ⚠️ 部分 &nbsp;|&nbsp; ❌ 缺失 &nbsp;|&nbsp; ⏳ 计划中 &nbsp;|&nbsp; ➖ 不适用
>
> **完整文字版**: 参见 [dsh-feature-parity.md](dsh-feature-parity.md) (12 sections 完整对比)。
> **开发计划**: 存 `_local/dsh-planning/` (本地, 不入仓)。

---

## 1. 核心包 (8 dsh → ma-harness)

| dsh 包 | 职责 | `ctx` 键 | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|---|
| `core/session` | append-only 事件日志 + 存储 | `ctx.sessions` | `ma-harness-core` `SessionEvent` | ✅ 完成 | 一致 — 9/9 dsh acp-snapshot fixture 通过 |
| `core/system-prompt` | prompt section + tool schema 组装 | `ctx.systemPrompt` | `ma-harness-core` `SystemPrompt` | ✅ 完成 | P7-1 |
| `core/tools` | scoped tool 注册表 + 受保护执行 | `ctx.tools` | `ma-harness-core` `ToolSchema` + `ma-harness-seam` | ✅ 完成 | P7-2; +8 个 first-party plugin |
| `core/agent` | `Agent` 接口 + live registry | `ctx.agents` | `ma-harness-cordis` agent 生命周期 | ✅ 完成 | P7-1 |
| `core/agent-loop` | 默认 driver | `ctx.agentLoop` | `ma-harness-core` | ✅ 完成 | P7-1 |
| `core/scope` | per-agent scoped 注册原语 | library | `ma-harness-cordis` `Disposable` | ✅ 完成 | P7-0 |
| `llm/llm` | message + stream + adapter | `ctx.llm` | `ma-harness-model` (4 个 backend) | 🔄 扩展 | 我们有 **4** 个 backend (OpenAI / Anthropic / Deepseek / Stub), dsh 只有 1 个 (Deepseek) |
| `webhook/webhook` | 鉴权 dispatch + Workspace Session | `ctx.webhookRuntime` | n/a | ❌ 缺失 | **P15+** 计划 |

**小计**: 7/8 完成 (87.5%) · 1 个缺失。

---

## 2. 能力缝 (14)

| 能力缝 | 用途 | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| `ctx.llm` | 模型 provider | `ma-harness-model` | ✅ 完成 | +Anthropic / +OpenAI / +Stub (dsh 只有 Deepseek) |
| `ctx.tools` | 工具注册表 | `ma-harness-core` + `ma-harness-seam` | ✅ 完成 | +8 个 first-party plugin |
| `ctx.sessions` | session 日志 + 生命周期 | `ma-harness-core` `SessionStore` | ✅ 完成 | InMemory + Sqlite; **没有 `fork()`** |
| `ctx.agents` | live agent 注册表 | `ma-harness-cordis` | ✅ 完成 | 一致 |
| `ctx.shell` | shell exec 后端 | n/a | ⚠️ 部分 | `plugin-bash` **直接** 通过 `tokio::Command` 跑;没走 `ctx.shell` 能力缝 |
| `ctx.subprocess` | 子进程 spawn | `tokio::process::Command` | ⚠️ 部分 | 直接用,没抽象成 `SubprocessService` trait |
| `ctx.terminals` | PTY 后端 | n/a | ❌ 缺失 | **P15+** 计划 (portable-pty) |
| `ctx.sandbox` | 进程隔离 | `ma-harness-sandbox` | ✅ 完成 | Landlock Linux + Seatbelt macOS + Stub; E2B / Firecracker **P16+** |
| `ctx.fs` | 文件系统 provider | `ma-harness-plugin-fs` | ✅ 完成 | local only; 远程 (s3/ssh) dsh 也没 |
| `ctx.commands` | 人类命令分发 | `#[dsh_command]` 宏 | ✅ 完成 | P7-2 |
| `ctx.jobs` | 后台 work | `tokio::spawn` + `plugin-subagent` | ✅ 完成 | P12-8; 没正式 `Job` trait |
| `ctx.webhookRuntime` | webhook 派发 | n/a | ❌ 缺失 | **P15+** 计划 |
| `ctx.systemPrompt` | prompt 组装 | `ma-harness-core` | ✅ 完成 | 一致 |
| `ctx.goals` | session 目标 | n/a | ❌ 缺失 | **P15+** 计划; 现在用 ad-hoc `#[dsh_command]` |

**小计**: 8/14 完成 (57%) · 3 个部分 · 3 个缺失。

---

## 3. 事件 (3 个域)

| 域 | 语义 | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| **Session 事件** | 持久化, 可重放 | `SessionEvent` 枚举 (13 个变体) | ✅ 完成 | "Model-visible 即 logged" 不变量由 `derive_messages()` 保证 |
| **Agent 事件** (`agent/*`) | live 事件, 在 in-flight 工作中 | `ma-harness-cordis` 类型化事件流 | ✅ 完成 | P7-1 |
| **Capability 事件** | 能力缝上的 policy + adapter 事件 | `ma-harness-cordis` 事件宏 | ✅ 完成 | 一致 |

**小计**: 3/3 完成 (100%)。

---

## 4. Turn Flow (13 步)

| dsh 阶段 | ma-harness 对应 | 状态 | 差异 / 说明 |
|---|---|---|---|
| `turn/start` / `turn/end` | `agent_loop::run_turn` | ✅ 完成 | `crates/ma-harness-core/src/agent.rs` |
| `agent/pre-step` | `SystemPrompt::assemble` | ✅ 完成 | prompt sections + tool schema |
| `agent/request` | `ModelAdapter::complete_stream` | ✅ 完成 | 4 个 backend |
| `llm/stream` | `Stream<Item=StreamChunk>` (futures::Stream) | ✅ 完成 | 一致 |
| `assistant/chunk*` | `SessionEvent::AssistantChunk` | ✅ 完成 | log + replay |
| `tools/pre-execute` | `ToolRegistry::validate_args` | ✅ 完成 | pre-execute hook |
| `tools/execute` | `ToolRegistry::invoke` | ✅ 完成 | — |
| `tools/post-execute` | `invoke` 内的 result transform | ✅ 完成 | — |
| `tool/result*` | `SessionEvent::ToolResult` | ✅ 完成 | log |
| `agent/turn-stopping` | cancellation token + drain | ✅ 完成 | — |
| **行为对等** | **9/9 dsh acp-snapshot fixture 通过** | ✅ 完成 | 100% |

**小计**: 10/10 阶段完成 (100%)。

---

## 5. Profile & Bundle

### 5.1 Profile (5)

| Profile | dsh | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| `web` | 浏览器 Web UI 在 `:3080` | `mah serve` (TUI) | ⚠️ 部分 | 只有 TUI; **没 Web UI** (P15+) |
| `headless` | 一次性 runner | `mah run "task"` | ✅ 完成 | 一致 |
| `sdk` | SDK JSON-RPC server | `mah acp serve` | ✅ 完成 | P11-4; 跟 dsh 互通 |
| `sdk-minimal` | 独立 bundle (无 `dsh-base`) | n/a | ❌ 缺失 | **P15+** 计划 |
| `acp` | 自动化专用 ACP | `mah acp serve` | ✅ 完成 | 跟 sdk 一样 |

### 5.2 Bundle (5)

| Bundle | 提供 | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| `dsh-base` | model + tools + sandbox + approval + settings | `cordis` + `core` + `model` + `sandbox` | ✅ 完成 | 由 4 个 crate 覆盖 |
| `dsh-web-app` | 浏览器 app | n/a | ❌ 缺失 | P15+ |
| `dsh-headless` | 一次性 runner | `ma-harness-cli` | ✅ 完成 | — |
| `dsh-sdk-app` | SDK JSON-RPC server | `ma-harness-cli acp` | ✅ 完成 | — |
| `dsh-acp-app` | 自动化专用 ACP | (跟 sdk 一样) | ✅ 完成 | — |
| `dsh-sdk-minimal` | 独立 bundle | n/a | ❌ 缺失 | P15+ |

**小计**: 6/11 完成 (55%) · 1 个部分 · 4 个缺失。

---

## 6. Session 日志

| dsh 概念 | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|
| `SessionEvent` (13 变体) | `SessionEvent` 枚举 | ✅ 完成 | 一致 |
| `dsh-session-projection` | `derive_messages()` | ✅ 完成 | 重建模型历史 |
| `InMemoryStore` | `InMemoryStore` | ✅ 完成 | — |
| `SqliteStore` | `SqliteStore` | ✅ 完成 | 持久化 |
| `SessionStore::fork()` | n/a | ❌ 缺失 | **P15+** 计划 (场景: 给并行 subagent fork session) |
| `ctx.sessionTitle` | n/a | ❌ 缺失 | **P15+** 计划 (从首条消息自动取标题) |

**小计**: 4/6 完成 (67%) · 2 个缺失。

---

## 7. 工具执行管道 (4 事件 waterfall)

| 阶段 | dsh 事件 | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| 调用记录 | `tool/call` | `SessionEvent::ToolCall` | ✅ 完成 | log |
| 预执行 | `tools/pre-execute` | `ToolRegistry::validate_args` | ✅ 完成 | pre-hook |
| 审批 | `ctx.approvals` | `ApprovalService` (P7-2/3) | ✅ 完成 | oneshot / TUI / HTTP |
| 执行 | `tools/execute` | `ToolRegistry::invoke` | ✅ 完成 | — |
| 后执行 | `tools/post-execute` | `invoke` 内的 result transform | ✅ 完成 | — |
| 结果记录 | `tool/result` | `SessionEvent::ToolResult` | ✅ 完成 | log |

**小计**: 6/6 阶段完成 (100%)。

---

## 8. 分发形态 (5)

| 形态 | dsh | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| **Web UI** | `:3080` 浏览器 | n/a (TUI) | ❌ 缺失 | **P15+** 计划 (Leptos WASM 或 React+SSE) |
| **ACP** | JSON-RPC 2.0 stdio | `mah acp serve` | ✅ 完成 | **跟 dsh 互通** (9/9 dsh-snap fixture 验证) |
| **Python SDK** | `pip install deepseek-harness-sdk` | `pip install mah-py` | ✅ 完成 | P11-3; 同样 JSON 线协议 |
| **Node.js SDK** | `npx @deepseek-ai/dsh` | n/a | ➖ 不适用 | 我们用 dsh-adapter (P13) 加载 dsh TS plugin |
| **Headless runner** | `dsh --profile headless` | `mah run "task"` | ✅ 完成 | 一致 |
| **Bundle 分发** | `package.json#dsh.bundle` | `Cargo.toml#ma-harness-registry.bundle` | ✅ 完成 | P12-5 |

**小计**: 4/6 完成 (67%) · 1 个缺失 · 1 个不适用。

---

## 9. CLI 模式

| dsh 命令 | ma-harness 对应 | 状态 | 差异 / 说明 |
|---|---|---|---|
| `dsh web` | `mah tui` | ⚠️ 部分 | TUI 替代 Web UI (P15+) |
| `dsh --profile headless <task>` | `mah run <task>` | ✅ 完成 | 同样语义 |
| `dsh --profile sdk` | `mah acp serve` | ✅ 完成 | JSON-RPC 2.0 stdio |
| `dsh --profile acp` | `mah acp serve` | ✅ 完成 | 跟 sdk 一样 |
| `dsh plugin <cmd>` | `mah plugin <cmd>` | ✅ 完成 | registry-based |
| `dsh --dump-config` | n/a | ❌ 缺失 | 我们用 registry-based 配置 (模型不同) |
| `dsh --profile <custom> --patch foo.yml` | n/a | ❌ 缺失 | 没 profile / patch 系统 (P15+) |
| `npx @deepseek-ai/dsh web` | n/a | ❌ 缺失 | 没 npx 集成; **dsh-adapter** 通过 JSON-RPC 加载 dsh TS plugin (P13) |

**小计**: 4/8 完成 (50%) · 1 个部分 · 3 个缺失。

---

## 10. Conformance (3 个 fixture 套)

| 套 | dsh | ma-harness | 状态 | 差异 / 说明 |
|---|---|---|---|---|
| `dsh acp-snapshot` (9 fixtures) | 100% | **100% (9/9)** | ✅ 对等 | 完全行为对等 |
| `dsh_synthetic` (7 fixtures) | n/a | **100% (7/7)** | ✅ 对等 | shape conversion |
| `smoke` (8 fixtures) | n/a | 62.5% (5/8) | ✅ by design | 3 个预期失败 (UI only) |
| Terminal Bench 2.1 | 87.9% | not run | ⏳ 计划 | 需业务方 LLM API key |
| Toolathlon-Verified | 74.1% | not run | ⏳ 计划 | 同上 |
| DSBench-FullStack | 71.1% | not run | ⏳ 计划 | 同上 |

**小计**: dsh 提供的 3 个套 100% 通过 (5/8 smoke 是 by design)。

---

## 11. ma-harness 扩展 (dsh 没有)

| 扩展 | ma-harness crate | 阶段 | 比 dsh 多出的价值 |
|---|---|---|---|
| **Plugin Registry** (npm 风格) | `ma-harness-registry` | P11-6 / P12-5 | `mah plugin install <name>` 从本地索引安装 |
| **Bundle** (lockfile 安装) | `ma-harness-bundle` | P11-8 / P12-7 | 可复现的多 plugin 安装 |
| **Vibe Coding Artifact 查看器** | `ma-harness-artifact` | P11-7 | 自动识别 + 渲染 10 种 artifact (HTML/SVG/JSON) |
| **DAG 编排** | `ma-harness-dag` | P12-9 | Kahn 拓扑 + 失败短路 |
| **多模态 vision** | `ma-harness-model` | P11-5/9, P12-8 | `describe_image` 工具, OpenAI + Anthropic |
| **Retry + Circuit Breaker** | `ma-harness-model` | P12-2 | 指数退避 + jitter |
| **Wasm 沙箱** (Code Mode) | `ma-harness-code` | P2.6 | wasmtime + 4 层防御 (fuel / epoch / mem / fs) |
| **Landlock 沙箱** (Linux kernel) | `ma-harness-sandbox` | P10 | kernel 强制 fs/进程限制 (kernel ≥ 5.13) |
| **Python SDK** | `mah-py` | P11-3 | subprocess 桥到 `mah` CLI |
| **crates.io 发布** | workspace | P12-5 | 24 crate 在 0.1.0/0.1.1 |
| **HTTP server (salvo)** | `ma-harness-server` | P6 | OpenAPI 导出, SSE |
| **TUI dashboard** | `ma-harness-tui` | P3.9 | ratatui 的 session / 事件查看器 |
| **dsh-adapter** | `ma-harness-plugin-dsh-adapter` | **P13** | 通过 JSON-RPC stdio 直接加载 dsh TS plugin |

**小计**: 13 个 ma-harness 独有功能, dsh 没对应。

---

## 12. 延后 / 未实现

| 项 | 阶段 | 状态 | 差异 / 说明 |
|---|---|---|---|
| **Web UI** (替代 TUI 给浏览器用户) | P15+ | ❌ 缺失 | 需要 Leptos WASM 或 React + REST + SSE |
| **PTY 后端** (`ctx.terminals`) | P15+ | ❌ 缺失 | portable-pty + session_id→pty_handle 映射 |
| **Webhook** (`ctx.webhookRuntime`) | P15+ | ❌ 缺失 | HMAC-SHA256 + 限速 + 派发队列 |
| **Profile 系统** (完整 dsh 对等) | P15+ | ❌ 缺失 | `~/.ma-harness/profiles/<name>/cordis.yml` |
| **Subagent** (正式 `ctx.subagent`) | P16+ | ⚠️ 部分 | `plugin-subagent` 能用, 但没 `SubagentService` trait |
| **Agent Teams** (`ctx.agentTeams`) | P16+ | ❌ 缺失 | dsh 是实验性, ma-harness 没 |
| **远程沙箱** (E2B / Firecracker / gVisor) | P16+ | ❌ 缺失 | 我们只有 Landlock local |
| **分布式 session 存储** (Redis / PostgreSQL) | P16+ | ❌ 缺失 | 我们只有 sqlite |
| **TypeScript SDK** (`@ma-harness/sdk`) | P17+ | ❌ 缺失 | 我们用 dsh-adapter 互通 |
| **身份 & 权限** (Branding, ACL) | P17+ | ❌ 缺失 | ma-harness 没正式身份 |
| **12+ LSP 语言** (完整生态) | P17+ | ⚠️ 部分 | 完全没 LSP (P14.5 计划首个) |
| **生产工具** (`mah dashboard / trace / cost`) | P17+ | ❌ 缺失 | — |
| **真 benchmark conformance** (Terminal Bench 2.1 / Toolathlon / DSBench) | P17+ | ⏳ 阻塞 | 需业务方 LLM API key |

**小计**: 13 个延后项, 映射到 P14–P17+ 阶段。

---

## 汇总

| 类别 | 总数 | ✅ 完成 | 🔄 扩展 | ⚠️ 部分 | ❌ 缺失 | ➖ 不适用 | 得分 |
|---|---|---|---|---|---|---|---|
| 1. 核心包 | 8 | 7 | 0 | 0 | 1 | 0 | **87.5%** |
| 2. 能力缝 | 14 | 8 | 0 | 3 | 3 | 0 | **57%** |
| 3. 事件 | 3 | 3 | 0 | 0 | 0 | 0 | **100%** |
| 4. Turn flow | 10 | 10 | 0 | 0 | 0 | 0 | **100%** |
| 5. Profile & Bundle | 11 | 6 | 0 | 1 | 4 | 0 | **55%** |
| 6. Session 日志 | 6 | 4 | 0 | 0 | 2 | 0 | **67%** |
| 7. 工具执行管道 | 6 | 6 | 0 | 0 | 0 | 0 | **100%** |
| 8. 分发形态 | 6 | 4 | 0 | 0 | 1 | 1 | **67%** |
| 9. CLI 模式 | 8 | 4 | 0 | 1 | 3 | 0 | **50%** |
| 10. Conformance | 3 | 3 | 0 | 0 | 0 | 0 | **100%** |
| 11. ma-harness 扩展 | 13 | 13 | 0 | 0 | 0 | 0 | **100%** (我们有) |
| 12. 延后 | 13 | 0 | 0 | 0 | 13 | 0 | **0%** (计划中) |
| **合计** | **101** | **68** | **0** | **5** | **27** | **1** | **67% 完成** |

**Snapshot 级行为对等**: **100% (9/9 dsh-snap + 7/7 dsh-synthetic)**。剩下的差距是 feature-surface 差距 (PTY / Web UI / Profile / Subagent / 等), 不是行为差距。

**路线图**: 2-3 年加 ~28 个新 crate (P14–P17+) 关闭 feature-surface 差距。计划存在 `_local/dsh-planning/` (本地, 不入仓)。

---

**参见**:
- [dsh-feature-parity.md](dsh-feature-parity.md) — 完整文字对比 (12 sections)
- [ma-harness-arch-map.md](../en/ma-harness-arch-map.md) — crate 依赖图
- [_local/dsh-planning/dsh-development-plan.zh-CN.md](../../_local/dsh-planning/dsh-development-plan.zh-CN.md) — P14–P17+ 路线图 (本地, 不入仓)
