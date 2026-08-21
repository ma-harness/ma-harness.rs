# dsh-adapter Design Doc (P13)

> **Task**: P13 / Phase 13
> **Priority**: P0
> **Created**: 2026-08-21 (Day 101+2)
> **Status**: 📋 Designing, awaiting implementation

## 1. Background

### 1.1 dsh (DeepSeek Harness) Overview

[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) (aka dsh) is DeepSeek's open-source Agent runtime, MIT-licensed, 70k+ stars, 0.1.0-rc.5, released 2026-08-13.

**Core philosophy**: "Everything is a Plugin" — model adapters, tools, sessions, sandboxes, Agent loop, UI are all plugins.

**Underlying engine**: [Cordis](https://github.com/cordiverse/cordis) (Shigma, Koishi ecosystem meta-framework), Spatiotemporal Composability paradigm.

**Plugin form**: TypeScript source, registered via Cordis Service + `defineTool({ name, description, parameters, output, async execute })`.

### 1.2 ma-harness.rs Current State

ma-harness.rs is a **Rust rewrite of dsh-style Agent harness**, NOT a dsh official Rust port:

- **API style**: Decorator style aligned (`#[dsh_tool]` / `#[dsh_service]` / `#[dsh_command]`)
- **Fixture format**: `dsh_format` adapter, dsh_synthetic 7/7 + dsh-snap 9/9 = 100% parity
- **Conformance**: Real dsh repo 9/9 acp-snapshot fixtures pass
- **Plugin binary**: Rust dylib (`.so/.dll/.dylib`) + C-ABI extern "C" + libloading
- **Incompatibility**: Rust dylib vs dsh TS plugin — different runtimes

### 1.3 Motivation

dsh community has 1000+ npm `dsh-plugin` packages. ma-harness users want to **reuse existing dsh plugins without rewriting them**.

**P13 Goal**: Write `dsh-adapter` plugin, let ma-harness load and run dsh TS plugins directly, via dsh's existing `@deepseek-ai/dsh-sdk-jsonrpc-server` protocol (no new protocol).

## 2. Design Goals

### 2.1 In-Scope (P13 must-have)

| Item | Description |
|---|---|
| **JSON-RPC client (Rust)** | Pair with dsh `@deepseek-ai/dsh-sdk-jsonrpc-server` |
| **Node.js subprocess spawn** | tokio::process::Command, start `node` running dsh plugin entry |
| **Tool schema bridge** | dsh `defineTool` schema → ma-harness `ToolSchema` |
| **Tool invoke** | ma-harness `ToolRegistry::invoke` → dsh `tools/call` JSON-RPC |
| **Lifecycle** | install / invoke / cancel / shutdown / subprocess respawn |
| **stderr / logging bridge** | dsh subprocess stderr → ma-harness `tracing` |
| **Config (cordis yaml)** | Support dsh-style Cordis YAML subset |
| **Conformance 9/9 dsh-snap** | Reuse existing `dsh_snap.jsonl`, run via adapter |
| **1 real dsh plugin e2e** | Pull real plugin (e.g. `@deepseek-ai/dsh-tool-bash`) and run |
| **Docs** | README + zh-CN guide, how-to + known limits |

### 2.2 Out-of-Scope (later phases)

| Item | Reason / Follow-up |
|---|---|
| **Bridge dsh's 78 plugins** | Sandbox / approval / persistence are dsh internals, not ma-harness surface |
| **PTC (Code mode) bridge** | dsh `run_code` + generated TS SDK too complex, P14+ |
| **Multiple dsh profiles** | One plugin at a time, profile switch P14+ |
| **dylib ↔ dsh interop** | One host loads both, P14+ |
| **Web UI bridge** | dsh-web ↔ ma-harness-tui, P15+ |
| **Cordis event bridge** | `tools/pre-execute` etc. hooks ignored, P14+ |

## 3. Architecture

### 3.1 Overview

```
┌─────────────────────────────────────────────────────────┐
│ ma-harness host (Rust)                                   │
│                                                          │
│  ┌────────────┐   ┌─────────────┐   ┌───────────────┐   │
│  │ ToolReg.   │   │ PluginLoader│   │ Conformance   │   │
│  │ (cordis)   │   │             │   │               │   │
│  └─────┬──────┘   └──────┬──────┘   └───────────────┘   │
│        │                 │                              │
│        │ schema/invoke   │ install("dsh::/path")        │
│        ▼                 ▼                              │
│  ┌─────────────────────────────────────┐                │
│  │ DshAdapter (new plugin)             │                │
│  │  - JSON-RPC client (Rust)           │                │
│  │  - schema cache                     │                │
│  │  - invoke → JSON-RPC call           │                │
│  └────────────┬────────────────────────┘                │
│               │ JSON-RPC 2.0 over stdio                 │
└───────────────┼──────────────────────────────────────────┘
                │
                ▼ (subprocess)
┌─────────────────────────────────────────────────────────┐
│ node child process                                        │
│                                                          │
│  ┌─────────────────────────────────────┐                │
│  │ @deepseek-ai/dsh-sdk-jsonrpc-server │ (reused)     │
│  │  - bootstrap dsh Cordis ctx          │                │
│  │  - load user plugin (TS)            │                │
│  │  - tools/register via Cordis        │                │
│  └────────────┬────────────────────────┘                │
│               │                                           │
│               ▼                                           │
│  ┌─────────────────────────────────────┐                │
│  │ user dsh plugin (TS, e.g. k8s)      │                │
│  │  - defineTool({...})                │                │
│  │  - async execute(args, exec)         │                │
│  └─────────────────────────────────────┘                │
└─────────────────────────────────────────────────────────┘
```

### 3.2 Wire Protocol (JSON-RPC 2.0)

**Reuse dsh's existing JSON-RPC server**. Client methods to implement:

| Method | Request | Response | Purpose |
|---|---|---|---|
| `initialize` | `{ protocolVersion, clientInfo }` | `{ serverInfo, capabilities }` | Handshake + version |
| `tools/list` | `{}` | `{ tools: ToolSchema[] }` | Get schemas, once on install |
| `tools/call` | `{ name, arguments, callId }` | `{ content: ContentBlock[], isError }` or `{ jobId }` | Invoke tool |
| `tools/cancel` | `{ callId, jobId? }` | `{}` | Cancel (exec.signal) |
| `shutdown` | `{}` | `{}` | Exit subprocess |

**Not implemented** (P13 out-of-scope): `session/*`, `approval/*`, `sandbox/*`, `files/*`, `ui/*`

**Protocol version**: Locked to `0.1.0-rc.5` (dsh current preview), upgrade via minor release.

### 3.3 Tool Schema Mapping

```rust
// ma-harness ToolSchema (existing)
struct ToolSchema {
    name: String,
    description: String,
    parameters: serde_json::Value,  // JSON Schema
    output_schema: Option<serde_json::Value>,  // NEW field (P13)
}

// dsh defineTool provides:
{
    name: "k8s_pod_status",
    description: "Check pod status",
    parameters: {
        namespace: { type: "string", required: true, ... },
        labelSelector: { type: "string", required: false, ... },
    },
    output: { schema: {...}, render: ... }
}
```

**Mapping rules**:
- `name` → `name`
- `description` → `description`
- `parameters` (record of fields) → JSON Schema object `{ properties, required: [...] }`
- `output.schema` → `ToolSchema::output_schema` (new field)
- `output.render` → **not mapped**, ma-harness renders locally

### 3.4 Error Handling

| dsh error | ma-harness error |
|---|---|
| `tools/call` returns `{ isError: true }` | `ToolError::RemoteError(msg)` |
| JSON-RPC parse fail | `ToolError::ProtocolError` |
| Subprocess crash / pipe close | `ToolError::PluginCrashed`, trigger respawn (max 3) |
| Timeout (default 30s) | `ToolError::Timeout`, auto `tools/cancel` |
| Schema validation fail | `ToolError::InvalidArgs` |

### 3.5 Config (cordis yaml subset)

```yaml
# plugins.dh-adapter.yaml
dsh:
  runtime: "node"  # or "deno" (P14+)
  node_path: "/usr/bin/node"  # auto-detect via `which node`
  timeout_secs: 30
  max_respawn: 3
  # dsh-native config
  dsh_env:
    DEEPSEEK_API_KEY: "${DEEPSEEK_API_KEY}"  # env var passthrough
```

**Config load**: Reuse `ma-harness-registry` YAML loader, path `~/.ma-harness/plugins.dh-adapter.yaml` or `MA_HARNESS_DSH_CONFIG` env var.

### 3.6 Dependencies

```toml
# plugins/ma-harness-plugin-dsh-adapter/Cargo.toml
[dependencies]
# Internal
ma-harness-cordis = { path = "../../crates/ma-harness-cordis" }
ma-harness-seam   = { path = "../../crates/ma-harness-seam" }
ma-harness-core   = { path = "../../crates/ma-harness-core" }
# External
tokio = { version = "1", features = ["process", "io-util", "sync", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
# No jsonrpc crate — write ~200 line client, protocol is simple
```

**Node.js deps** (user / CI):
- Node.js 22.19+ or 24+ (dsh requirement)
- `npm install -g @deepseek-ai/dsh-sdk-jsonrpc-server` or local install

## 4. Task Breakdown (5 Phases)

### P13.1 Skeleton (1 week)

- [ ] Create `plugins/ma-harness-plugin-dsh-adapter/` crate
- [ ] `Cargo.toml` publish=true
- [ ] `src/lib.rs` empty shell: `pub struct DshAdapter;` + `Plugin` impl
- [ ] `src/jsonrpc.rs` ~150-line JSON-RPC 2.0 client (read/write framed over stdin/stdout)
- [ ] `src/process.rs` `tokio::process::Command` spawn `node` + read stderr → `tracing::warn!`
- [ ] Unit test: mock fake node running JSON-RPC echo server
- [ ] Docs: `README.md` + `README.zh-CN.md`, "hello world: spawn node + 1 sentence"

**Acceptance**:
- `cargo test -p ma-harness-plugin-dsh-adapter` 0 errors
- `dsh_adapter_smoke` runs, mock node responds to JSON-RPC initialize + tools/list
- CI build (3 OS) passes

### P13.2 Tool Bridge (1 week)

- [ ] `src/schema.rs` dsh `defineTool` → ma-harness `ToolSchema` conversion
- [ ] `src/registry.rs` install calls `tools/list` once, registers to ma-harness `ToolRegistry`
- [ ] `src/invoke.rs` `ToolRegistry::invoke` → `tools/call` JSON-RPC
- [ ] Schema validation: ma-harness JSON Schema validate args (same as local dylib)
- [ ] Unit test: mock node returns 1 simple tool (e.g. `echo`), install + invoke E2E
- [ ] Error handling: isError / timeout / protocol error → `ToolError`

**Acceptance**:
- mock node returns `echo(msg: string)`, ma-harness invokes + gets result
- Error cases: tool returns isError=true → `ToolError::RemoteError`
- Schema validation fail → `ToolError::InvalidArgs`

### P13.3 Lifecycle (1 week)

- [ ] `shutdown` step on `DshAdapter` drop, close subprocess
- [ ] Subprocess crash → auto respawn (max 3 times, exp backoff 1s/2s/4s)
- [ ] After respawn: re-run `initialize` + `tools/list` to recover schema
- [ ] `tools/cancel` bridge ma-harness `tokio::sync::oneshot` → JSON-RPC `tools/cancel`
- [ ] Subprocess stderr parse: dsh logs to stderr, bridge to `tracing::warn!`, don't pollute stdout JSON-RPC
- [ ] Config load: `~/.ma-harness/plugins.dsh-adapter.yaml`
- [ ] Unit test: kill subprocess (SIGKILL) → verify respawn, cancel call → verify dsh receives `tools/cancel`

**Acceptance**:
- Subprocess crash 3 times → fail-fast (no infinite respawn)
- Cancel path: ma-harness `select!` waits for invoke result OR cancel signal, cancel triggers JSON-RPC `tools/cancel`
- Config reload: change yaml, restart, new timeout takes effect

### P13.4 Conformance (1 week)

- [ ] Add `mah conformance --dsh-adapter` flag in `ma-harness-conformance`
- [ ] Reuse existing `crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl` (9 fixtures)
- [ ] Write mock dsh plugin (`fixtures/dsh-snap-converted/mock-dsh-plugin.ts`): implements tools dsh snap expects
- [ ] `dsh-adapter` installs this mock plugin, runs conformance, 9/9 = 100%
- [ ] Distinguish from existing `--dsh` flag (dsh_synthetic): `--dsh` = ma-harness dsh_format, `--dsh-adapter` = real dsh process
- [ ] Benchmark: vs dsh native (Node.js direct), latency < 2x

**Acceptance**:
- `mah conformance --dsh-adapter --fixtures dsh_snap.jsonl` 9/9 = 100%
- Latency < 2x of dsh native (main overhead is JSON-RPC serialization)
- Conformance report same format as existing dsh, with dsh-adapter path

### P13.5 E2E + Docs (1 week)

- [ ] E2E fixture: real dsh repo plugin (e.g. `@deepseek-ai/dsh-tool-str-replace-editor` or self-written k8s_pod_status demo)
- [ ] Add `plugins/ma-harness-plugin-dsh-adapter/examples/k8s_pod_status.ts` (complete dsh plugin, in README)
- [ ] `mah load-plugin dsh::./examples/k8s_pod_status.ts` works, outputs schema
- [ ] CLI subcommand `mah dsh info / mah dsh doctor`:
  - `info`: dsh runtime version / Node.js version / JSON-RPC protocol version
  - `doctor`: health check (Node.js installed / npm packages / subprocess starts)
- [ ] Docs:
  - README 5-minute quickstart
  - Link to dsh repo (`https://github.com/deepseek-ai/deepseek-harness`)
  - Known limits (PTC / multi-profile / dylib interop P14+)
  - Performance data (vs dsh native)
- [ ] CI: add e2e job (`test-dsh-adapter-e2e`), ubuntu + Node.js 24 runner
- [ ] memory: write 3-5 entries (research / design / impl / known limits)

**Acceptance**:
- New user clones repo → 5 min runs hello world
- CI e2e passes
- Docs 100% (bilingual, matches `docs/style.md`)

## 5. Risks & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| **dsh 0.1.0-rc.5 protocol unstable** (official says "breaking changes coming") | P13 may break on dsh upgrade | Lock dsh version `package.json` to `0.1.0-rc.5`, upgrade via minor release, write compat layer 1 week ahead |
| **Node.js not always installed** | e2e fails | CI installs (`actions/setup-node@v4` with node-version: 24), README emphasizes "Node 22.19+ required" |
| **dsh subprocess single-threaded** | Heavy tools may block | Per dsh, user uses `Promise` async + `exec.signal` cancel, ma-harness 30s timeout fallback |
| **Windows Node.js path differences** | spawn fails | Use `which::which("node")` probe, fallback `where.exe node`; README has manual `node_path` config |
| **conformance 9/9 not 100%** | Exposes ma-harness/dsh semantic diff | Allow 1-2 fixture skip, tag `dsh_format_skip` category |
| **Rust dylib ↔ dsh interop deferred** | Users can't mix | P14 dedicated phase |

## 6. P13 Acceptance

- [ ] All 5 phases complete
- [ ] `plugins/ma-harness-plugin-dsh-adapter/` published to crates.io
- [ ] `mah conformance --dsh-adapter` 9/9 dsh-snap = 100%
- [ ] 1 real dsh plugin e2e passes
- [ ] CI e2e job on all 3 OS
- [ ] Docs 100% bilingual
- [ ] `mah dsh doctor` self-check passes
- [ ] memory 3-5 new entries
- [ ] `mah info` shows dsh-adapter status

## 7. Follow-up Roadmap (P14+)

- **P14.1**: Interop (ma-harness loads dylib + dsh plugins, share ToolRegistry)
- **P14.2**: PTC (Code mode) bridge (`run_code` tool + generated TS SDK)
- **P14.3**: Cordis event hook (`tools/pre-execute` permission gate bridge)
- **P15.1**: Multi-dsh-profile (headless / web mode selection)
- **P15.2**: Web UI bridge (dsh-web ↔ ma-harness-tui)

## 8. Timeline (est)

```
Week 1  P13.1 skeleton
Week 2  P13.2 tool bridge
Week 3  P13.3 lifecycle
Week 4  P13.4 conformance
Week 5  P13.5 e2e + docs
Week 6  buffer / review / release

Total: 5-6 weeks (1 person, full-time)
```

## 9. References

- [DeepSeek Harness repo](https://github.com/deepseek-ai/deepseek-harness) (MIT)
- [Cordis meta-framework](https://github.com/cordiverse/cordis) (MIT, from Koishi)
- [A Programming Paradigm for Spatiotemporal Composability](https://arxiv.org/abs/...) (Cordis paper)
- [ma-harness dsh references](../../conformance-design.md) (`dsh_format` / `dsh_synthetic` / `dsh_snap`)
- Phase 11 P11-1 / P11-2: dsh 9/9 conformance already 100%, foundation for P13

---

**Version**: v1.0 (2026-08-21)
**Next review**: P13.1 complete
