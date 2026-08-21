# ma-harness dsh-adapter plugin (P13)

> **Status**: P13.1 ✅ 骨架完成 (2026-08-21)
> **Goal**: Let ma-harness load and run [dsh (DeepSeek Harness)](https://github.com/deepseek-ai/deepseek-harness) TS plugins directly, via dsh's native JSON-RPC over stdio.

## What is this?

`ma-harness-plugin-dsh-adapter` is an **8th first-party plugin** in the ma-harness ecosystem. It bridges ma-harness to dsh's plugin ecosystem — instead of rewriting a dsh plugin in Rust, you can load it directly:

```bash
# Load a dsh TS plugin (planned for P13.5)
mah load-plugin dsh::./examples/k8s_pod_status.ts
```

## P13.1 Status (this commit)

P13.1 implements the **skeleton**:
- ✅ `DshAdapter` struct (spawn + JSON-RPC client + Node.js subprocess)
- ✅ Hand-written JSON-RPC 2.0 client (~150 lines, no `jsonrpc` crate)
- ✅ Node.js subprocess spawn with stdin/stdout/stderr pipes
- ✅ stderr bridge to `tracing::warn!`
- ✅ Mock node server (inline JS, ~30 lines) for e2e testing
- ✅ 2 e2e tests pass: `smoke_spawn_initialize_list_call_shutdown` + `smoke_initialize_then_list_tools_then_shutdown`

P13.1 **does NOT yet**:
- Load real dsh `@deepseek-ai/dsh-sdk-jsonrpc-server` (uses mock instead)
- Bridge to ma-harness `ToolRegistry` (P13.2)
- Implement lifecycle (respawn / cancel / SIGTERM) (P13.3)
- Wire `mah load-plugin dsh::/path` CLI command (P13.5)

## API (P13.1)

```rust
use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};
use serde_json::json;

let mut adapter = DshAdapter::spawn(
    std::path::Path::new("path/to/plugin.ts"),
    DshConfig::default(),  // node path, timeout 30s, max_respawn 3
).await?;

// 1. Handshake
let server_info = adapter.initialize().await?;
println!("Server: {} v{}", server_info.name, server_info.version);

// 2. Get tools
let tools = adapter.list_tools().await?;
for tool in tools {
    println!("Tool: {} - {}", tool.name, tool.description);
}

// 3. Call tool
let result = adapter.call_tool("echo", json!({"msg": "hello"})).await?;
assert!(!result.is_error);

// 4. Shutdown (sends JSON-RPC shutdown, waits 5s, then SIGKILL fallback)
adapter.shutdown().await?;
```

## Architecture

```
┌──────────────────────┐         ┌─────────────────────────┐
│  ma-harness host     │         │  Node.js child process   │
│  (Rust)              │         │                         │
│                      │         │  ┌────────────────────┐ │
│  ┌────────────────┐  │  JSON-  │  │ mock dsh server    │ │
│  │ DshAdapter     │  │  RPC    │  │ (P13.1: inline)    │ │
│  │  - client      │──│ 2.0     │──│ (P13.2: real dsh)  │ │
│  │  - child       │  │  stdio  │  └────────────────────┘ │
│  └────────────────┘  │         │                         │
└──────────────────────┘         └─────────────────────────┘
```

## Running the tests

```bash
# Requires Node.js 22+ in PATH
node --version
# v22.19.0+ or v24+ (dsh requirement)

cargo test -p ma-harness-plugin-dsh-adapter
```

Output:
```
running 2 tests
test smoke_spawn_initialize_list_call_shutdown ... ok
test smoke_initialize_then_list_tools_then_shutdown ... ok
test result: ok. 2 passed; 0 failed
```

## Configuration

`DshConfig` fields (P13.1):
- `node_path: Option<PathBuf>` — explicit path, defaults to PATH search
- `timeout: Duration` — tool call timeout, default 30s
- `max_respawn: usize` — respawn budget, default 3 (P13.3)
- `dsh_env: Vec<(String, String)>` — env vars passed to dsh subprocess (e.g. `DEEPSEEK_API_KEY`)

P13.1 doesn't yet load config from `~/.ma-harness/plugins.dsh-adapter.yaml` (P13.3).

## Dependencies

- `tokio` (process, io-util, sync, rt-multi-thread, macros, time, fs)
- `serde`, `serde_json`, `thiserror`, `tracing`
- Internal: `ma-harness-cordis`, `ma-harness-seam`, `ma-harness-core` (P13.2 wiring)
- **No** `jsonrpc` crate (we hand-write the ~200 line client)

## Roadmap (P13 phases)

| Phase | Status | What |
|---|---|---|
| **P13.1** | ✅ done | Skeleton + JSON-RPC client + Node.js spawn + mock tests |
| P13.2 | pending | Tool bridge: dsh `defineTool` → ma-harness `ToolSchema` + invoke |
| P13.3 | pending | Lifecycle: shutdown / respawn / cancel / stderr / config |
| P13.4 | pending | Conformance: `mah conformance --dsh-adapter` runs 9/9 dsh-snap |
| P13.5 | pending | E2E: real dsh plugin + `mah dsh info/doctor` + CI + docs |

See [`docs/en/design/dsh-adapter.md`](../../docs/en/design/dsh-adapter.md) for full design.

## License

MIT OR Apache-2.0 (same as ma-harness.rs)
