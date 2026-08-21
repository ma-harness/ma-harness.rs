# ma-harness dsh-adapter plugin (P13)

> **状态**: P13.1 ✅ 骨架完成 (2026-08-21)
> **目标**: 让 ma-harness 能直接加载并运行 [dsh (DeepSeek Harness)](https://github.com/deepseek-ai/deepseek-harness) 写的 TS plugin, 走 dsh 自家 JSON-RPC over stdio 协议。

## 这是什么?

`ma-harness-plugin-dsh-adapter` 是 ma-harness 生态的第 **8 个 first-party plugin**。它把 ma-harness 跟 dsh 的 plugin 生态桥接起来 — 不用重写 dsh plugin 成 Rust, 直接加载:

```bash
# 加载 dsh TS plugin (P13.5 计划)
mah load-plugin dsh::./examples/k8s_pod_status.ts
```

## P13.1 状态 (本次 commit)

P13.1 实现了**骨架**:
- ✅ `DshAdapter` struct (spawn + JSON-RPC client + Node.js 子进程)
- ✅ 手写 JSON-RPC 2.0 client (~150 行, 不引 `jsonrpc` crate)
- ✅ Node.js 子进程 spawn, 拿 stdin/stdout/stderr pipes
- ✅ stderr 桥接到 `tracing::warn!`
- ✅ Mock node server (inline JS, ~30 行) 给 e2e 测试用
- ✅ 2 个 e2e 测试过: `smoke_spawn_initialize_list_call_shutdown` + `smoke_initialize_then_list_tools_then_shutdown`

P13.1 **还没做**:
- 加载真 dsh `@deepseek-ai/dsh-sdk-jsonrpc-server` (用 mock 代替)
- 桥接到 ma-harness `ToolRegistry` (P13.2)
- Lifecycle (respawn / cancel / SIGTERM) (P13.3)
- CLI 命令 `mah load-plugin dsh::/path` 集成 (P13.5)

## API (P13.1)

```rust
use ma_harness_plugin_dsh_adapter::{DshAdapter, DshConfig};
use serde_json::json;

let mut adapter = DshAdapter::spawn(
    std::path::Path::new("path/to/plugin.ts"),
    DshConfig::default(),  // node path, timeout 30s, max_respawn 3
).await?;

// 1. 握手
let server_info = adapter.initialize().await?;
println!("Server: {} v{}", server_info.name, server_info.version);

// 2. 拿工具
let tools = adapter.list_tools().await?;
for tool in tools {
    println!("Tool: {} - {}", tool.name, tool.description);
}

// 3. 调工具
let result = adapter.call_tool("echo", json!({"msg": "hello"})).await?;
assert!(!result.is_error);

// 4. Shutdown (发 JSON-RPC shutdown, 等 5s, 然后 SIGKILL 兜底)
adapter.shutdown().await?;
```

## 架构

```
┌──────────────────────┐         ┌─────────────────────────┐
│  ma-harness host     │         │  Node.js child process   │
│  (Rust)              │         │                         │
│                      │         │  ┌────────────────────┐ │
│  ┌────────────────┐  │  JSON-  │  │ mock dsh server    │ │
│  │ DshAdapter     │  │  RPC    │  │ (P13.1: inline)    │ │
│  │  - client      │──│ 2.0     │──│ (P13.2: 真 dsh)    │ │
│  │  - child       │  │  stdio  │  └────────────────────┘ │
│  └────────────────┘  │         │                         │
└──────────────────────┘         └─────────────────────────┘
```

## 跑测试

```bash
# 需要 Node.js 22+ 在 PATH
node --version
# v22.19.0+ 或 v24+ (dsh 要求)

cargo test -p ma-harness-plugin-dsh-adapter
```

输出:
```
running 2 tests
test smoke_spawn_initialize_list_call_shutdown ... ok
test smoke_initialize_then_list_tools_then_shutdown ... ok
test result: ok. 2 passed; 0 failed
```

## 配置

`DshConfig` 字段 (P13.1):
- `node_path: Option<PathBuf>` — 显式路径, 默认走 PATH 搜
- `timeout: Duration` — 工具调用超时, 默认 30 秒
- `max_respawn: usize` — respawn 次数, 默认 3 (P13.3)
- `dsh_env: Vec<(String, String)>` — 透传给 dsh 子进程的环境变量 (e.g. `DEEPSEEK_API_KEY`)

P13.1 还**没**支持从 `~/.ma-harness/plugins.dsh-adapter.yaml` 加载 (P13.3)。

## 依赖

- `tokio` (process, io-util, sync, rt-multi-thread, macros, time, fs)
- `serde`, `serde_json`, `thiserror`, `tracing`
- 内部: `ma-harness-cordis`, `ma-harness-seam`, `ma-harness-core` (P13.2 桥接用)
- **不**引 `jsonrpc` crate (手写 ~200 行 client)

## 路线图 (P13 phases)

| Phase | 状态 | 内容 |
|---|---|---|
| **P13.1** | ✅ 完成 | 骨架 + JSON-RPC client + Node.js spawn + mock 测试 |
| P13.2 | 待 | 工具桥接: dsh `defineTool` → ma-harness `ToolSchema` + invoke |
| P13.3 | 待 | Lifecycle: shutdown / respawn / cancel / stderr / 配置 |
| P13.4 | 待 | Conformance: `mah conformance --dsh-adapter` 跑 9/9 dsh-snap |
| P13.5 | 待 | E2E: 真 dsh 插件 + `mah dsh info/doctor` + CI + 文档 |

详细设计: [`docs/zh-CN/design/dsh-adapter.md`](../../docs/zh-CN/design/dsh-adapter.md)

## 许可证

MIT OR Apache-2.0 (跟 ma-harness.rs 一样)
