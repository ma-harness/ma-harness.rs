# dsh-adapter 设计文档 (P13)

> **任务编号**: P13 / Phase 13
> **优先级**: P0
> **创建日期**: 2026-08-21 (Day 101+2)
> **作者**: ma-harness.rs team
> **状态**: 📋 设计中, 待实施

## 1. 背景

### 1.1 dsh (DeepSeek Harness) 简介

[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) (下文简称 dsh) 是 DeepSeek 官方 2026-08-13 开源的 Agent 运行时框架, MIT 协议, 70k+ Star, 0.1.0-rc.5。

**核心理念**: "Everything is a Plugin" — 模型适配器、工具、会话、沙箱、Agent 循环、UI 全是插件。

**底层引擎**: [Cordis](https://github.com/cordiverse/cordis) (Shigma 写, Koishi 生态抽出的元框架), 时空可组合性 (Spatiotemporal Composability) 范式。

**Plugin 形态**: TypeScript source, 通过 Cordis Service + `defineTool({ name, description, parameters, output, async execute })` 注册。

### 1.2 ma-harness.rs 现状

ma-harness.rs 是 **Rust 重写 dsh 风格 Agent harness**, 不是 dsh 官方 Rust 端口:

- **API 风格**: 装饰器风格对齐 (`#[dsh_tool]` / `#[dsh_service]` / `#[dsh_command]`)
- **Fixture 格式**: `dsh_format` 转换层, dsh_synthetic 7/7 + dsh-snap 9/9 = 100% parity
- **Conformance**: 真 dsh repo 9/9 acp-snapshot fixture 全过
- **Plugin binary**: Rust dylib (`.so/.dll/.dylib`) + C-ABI extern "C" + libloading
- **不兼容点**: Rust dylib 跟 dsh TS plugin 不是一个 runtime

### 1.3 业务动机

dsh 社区 1000+ npm `dsh-plugin` 标签包, 业务方 (跟 ma-harness 同源的) 想直接复用 dsh 现有 plugin, **不重写**。

**P13 目标**: 写 `dsh-adapter` plugin, 让 ma-harness 能直接加载并运行 dsh TS plugin, 走 dsh 自家 `@deepseek-ai/dsh-sdk-jsonrpc-server` 协议 (已经存在, 不造协议)。

## 2. 设计目标

### 2.1 必须做 (In-Scope)

| 项 | 说明 |
|---|---|
| **JSON-RPC client (Rust)** | 跟 dsh `@deepseek-ai/dsh-sdk-jsonrpc-server` 配对 |
| **Node.js 子进程 spawn** | tokio::process::Command 启动 `node` 跑 dsh plugin 入口 |
| **工具 schema 桥接** | dsh `defineTool` schema → ma-harness `ToolSchema` |
| **工具调用 invoke** | ma-harness `ToolRegistry::invoke` → dsh `tools/call` JSON-RPC |
| **lifecycle** | install / invoke / cancel / shutdown / 子进程 respawn |
| **stderr / logging 桥接** | dsh 子进程 stderr → ma-harness `tracing` |
| **配置 (cordis yaml)** | 支持 dsh 风格的 Cordis YAML 子集 |
| **conformance 9/9 dsh-snap 全过** | 复用现有 `dsh_snap.jsonl`, adapter 跑通 |
| **1 个真 dsh 插件 e2e** | 从 dsh repo 拉一个真 plugin (e.g. `@deepseek-ai/dsh-tool-bash`) 跑通 |
| **文档** | README + 中文指南, 怎么用 + 已知限制 |

### 2.2 不做 (Out-of-Scope, 后续 phase)

| 项 | 原因 / 后续 |
|---|---|
| **dsh 全 78 行 plugin 桥接** | 沙箱 / approval / 持久会话等 dsh 内部 plugin 不属于 ma-harness 暴露面 |
| **PTC (Code mode) 桥接** | dsh `run_code` tool + generated TS SDK 太复杂, P14+ |
| **多 dsh profile** | 一次只加载 1 个 dsh plugin, profile 切换 P14+ |
| **dylib ↔ dsh 互操作** | 一个 host 同时加载 dylib 跟 dsh 插件, P14+ |
| **Web UI 桥接** | dsh 有 dsh-web, ma-harness 有 TUI, P15+ |
| **Cordis 事件桥接** | `tools/pre-execute` 等 hook 全部忽略, P14+ |

## 3. 架构设计

### 3.1 总体

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
                ▼ (子进程)
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

**直接复用 dsh 已有 JSON-RPC server**. 客户端要实现的 method:

| Method | Request | Response | 用途 |
|---|---|---|---|
| `initialize` | `{ protocolVersion, clientInfo }` | `{ serverInfo, capabilities }` | 握手 + 协议版本 |
| `tools/list` | `{}` | `{ tools: ToolSchema[] }` | 拿工具 schema, install 时一次性 |
| `tools/call` | `{ name, arguments, callId }` | `{ content: ContentBlock[], isError }` 或 `{ jobId }` | 调工具 |
| `tools/cancel` | `{ callId, jobId? }` | `{}` | 取消 (exec.signal) |
| `shutdown` | `{}` | `{}` | 退出子进程 |

**不实现** (P13 out-of-scope): `session/*`, `approval/*`, `sandbox/*`, `files/*`, `ui/*`

**协议版本**: 锁 `0.1.0-rc.5` (dsh 当前 preview), 升级走 minor release。

### 3.3 工具 schema 映射

```rust
// ma-harness ToolSchema (现有)
struct ToolSchema {
    name: String,
    description: String,
    parameters: serde_json::Value,  // JSON Schema
    output_schema: Option<serde_json::Value>,  // 新增字段 (P13)
}

// dsh defineTool 给的
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

**映射规则**:
- `name` → `name`
- `description` → `description`
- `parameters` (record of fields) → JSON Schema object `{ properties, required: [...] }`
- `output.schema` → `ToolSchema::output_schema` (新字段)
- `output.render` → **不映射**, ma-harness 端 render

### 3.4 错误处理

| dsh 错误 | ma-harness 错误 |
|---|---|
| `tools/call` 返回 `{ isError: true }` | `ToolError::RemoteError(msg)` |
| JSON-RPC 解析失败 | `ToolError::ProtocolError` |
| 子进程 crash / pipe close | `ToolError::PluginCrashed`, 触发 respawn (最多 3 次) |
| 超时 (默认 30s) | `ToolError::Timeout`, 自动 `tools/cancel` |
| schema 校验失败 | `ToolError::InvalidArgs` |

### 3.5 配置 (cordis yaml 子集)

```yaml
# plugins.dh-adapter.yaml
dsh:
  runtime: "node"  # or "deno" (P14+)
  node_path: "/usr/bin/node"  # auto-detect via `which node`
  timeout_secs: 30
  max_respawn: 3
  # dsh 自家 config
  dsh_env:
    DEEPSEEK_API_KEY: "${DEEPSEEK_API_KEY}"  # env var 透传
```

**配置加载**: 复用 `ma-harness-registry` 的 YAML loader, 路径 `~/.ma-harness/plugins.dh-adapter.yaml` 或 `MA_HARNESS_DSH_CONFIG` 环境变量。

### 3.6 依赖

```toml
# plugins/ma-harness-plugin-dsh-adapter/Cargo.toml
[dependencies]
# 内部
ma-harness-cordis = { path = "../../crates/ma-harness-cordis" }
ma-harness-seam   = { path = "../../crates/ma-harness-seam" }
ma-harness-core   = { path = "../../crates/ma-harness-core" }
# 外部
tokio = { version = "1", features = ["process", "io-util", "sync", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
# 不用 jsonrpc crate — 自己手写 ~200 行 client, 协议简单, 不引入依赖
```

**Node.js 依赖** (业务方本机 / CI):
- Node.js 22.19+ 或 24+ (dsh 要求)
- `npm install -g @deepseek-ai/dsh-sdk-jsonrpc-server` 或本地 install

## 4. 任务分解 (5 Phase)

### P13.1 骨架 (1 周)

- [ ] 建 `plugins/ma-harness-plugin-dsh-adapter/` crate
- [ ] `Cargo.toml` 配 publish=true (跟其他 7 个 plugin 一致)
- [ ] `src/lib.rs` 空壳: `pub struct DshAdapter;` + `Plugin` impl
- [ ] `src/jsonrpc.rs` ~150 行手写 JSON-RPC 2.0 client (read/write framed over stdin/stdout)
- [ ] `src/process.rs` `tokio::process::Command` spawn `node` + 读 stderr → `tracing::warn!`
- [ ] 单测: `cargo test -p ma-harness-plugin-dsh-adapter`, mock 一个假 node 跑 JSON-RPC echo server
- [ ] 文档: `plugins/ma-harness-plugin-dsh-adapter/README.md` (英文) + `README.zh-CN.md` (中文), 写"hello world: spawn node + 1 句话"

**Acceptance**:
- `cargo test -p ma-harness-plugin-dsh-adapter` 0 错
- `dsh_adapter_smoke` 跑通, 跟 mock node 通一次 JSON-RPC initialize + tools/list
- ci build (3 OS) 过

### P13.2 工具桥接 (1 周)

- [ ] `src/schema.rs` dsh `defineTool` → ma-harness `ToolSchema` 转换
- [ ] `src/registry.rs` install 时调 `tools/list` 一次性拿全部 schema, 注册到 ma-harness `ToolRegistry`
- [ ] `src/invoke.rs` `ToolRegistry::invoke` 转发到 `tools/call` JSON-RPC
- [ ] schema 校验: ma-harness 端 JSON Schema 校验 args (跟本地 dylib plugin 走同一套)
- [ ] 单测: 用 mock node 返回 1 个简单 tool (e.g. `echo`), install + invoke 端到端
- [ ] 错误处理: isError / 超时 / 协议错误统一到 `ToolError`

**Acceptance**:
- mock node 返回 `echo(msg: string)`, ma-harness 端能 invoke + 拿到结果
- 错误用例: tool 返回 isError=true → ma-harness 端拿到 `ToolError::RemoteError`
- schema 校验失败 → `ToolError::InvalidArgs`

### P13.3 lifecycle (1 周)

- [ ] `shutdown` step 在 `DshAdapter` drop 时自动调, 关子进程
- [ ] 子进程 crash → 自动 respawn (最多 3 次, 指数 backoff 1s/2s/4s)
- [ ] respawn 后重新调 `initialize` + `tools/list` 恢复 schema
- [ ] `tools/cancel` 调用的 `exec.signal` 桥接: ma-harness `tokio::sync::oneshot` cancel → JSON-RPC `tools/cancel`
- [ ] 子进程 stderr 解析: dsh 用 stderr 打日志, 桥到 `tracing::warn!`, 不污染 stdout JSON-RPC
- [ ] 配置加载: `~/.ma-harness/plugins.dsh-adapter.yaml` (复用 registry loader)
- [ ] 单测: 杀子进程 (SIGKILL) → 验证 respawn, cancel 调 → 验证 dsh 收到 `tools/cancel`

**Acceptance**:
- 子进程 crash 3 次后 fail-fast (不无限 respawn)
- cancel 路径: ma-harness 端 select! 一边等 invoke result 一边等 cancel signal, cancel 触发 → JSON-RPC tools/cancel
- 配置 reload: 改 yaml 重启进程, 新 timeout 生效

### P13.4 conformance (1 周)

- [ ] 在 `ma-harness-conformance` 加 `mah conformance --dsh-adapter` flag
- [ ] 复用现有 `crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl` (9 个 fixture)
- [ ] 写一个 mock dsh plugin (`fixtures/dsh-snap-converted/mock-dsh-plugin.ts`): 实现 dsh snap 期望的工具 (e.g. 简单 echo tool)
- [ ] `dsh-adapter` install 这个 mock plugin, 跑 conformance, 验 9/9 = 100%
- [ ] 跟现有 `--dsh` flag (dsh_synthetic) 区分: `--dsh` 走原 ma-harness dsh_format, `--dsh-adapter` 走真 dsh 进程
- [ ] benchmark: 跟 dsh 原生 (Node.js 直接跑) 对比, latency 差异 < 2x

**Acceptance**:
- `mah conformance --dsh-adapter --fixtures dsh_snap.jsonl` 跑 9/9 = 100%
- latency 跟 dsh 原生比 < 2x (主要 overhead 是 JSON-RPC serialization)
- conformance-report 跟现有 dsh 一致, 报告新增 dsh-adapter 路径

### P13.5 e2e + 文档 (1 周)

- [ ] 写一个 e2e fixture: 从 dsh 真 repo 拉一个简单 plugin (e.g. `@deepseek-ai/dsh-tool-str-replace-editor` 或自写一个 k8s_pod_status demo), 跑通
- [ ] 加 `plugins/ma-harness-plugin-dsh-adapter/examples/k8s_pod_status.ts` (一个完整 dsh plugin, 写到 README)
- [ ] `mah load-plugin dsh::./examples/k8s_pod_status.ts` 命令跑通, 输出 schema
- [ ] CLI 子命令加 `mah dsh info / mah dsh doctor`:
  - `info`: 显示 dsh runtime 版本 / Node.js 版本 / JSON-RPC 协议版本
  - `doctor`: 健康检查 (Node.js 装没 / npm 包齐不齐 / 子进程能起)
- [ ] 文档:
  - README 写 5 分钟 quickstart
  - 跟 dsh repo 链接 (`https://github.com/deepseek-ai/deepseek-harness`)
  - 已知限制 (PTC / 多 profile / dylib 互操作 P14+)
  - 性能数据 (跟 dsh 原生对比)
- [ ] CI: 加 e2e job (`test-dsh-adapter-e2e`), ubuntu + Node.js 24 runner
- [ ] memory 更新: `### ma-harness dsh-adapter 调研 / 设计 / 实现` 多条 entry

**Acceptance**:
- 新业务方 clone 仓库 → 5 分钟跑通 hello world
- CI 跑 e2e 绿
- 文档 100% (中英双语, 跟 `docs/style.md` 一致)

## 5. 风险 & 缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| **dsh 0.1.0-rc.5 协议不稳定** (官方明言"会有破坏性变更") | P13 写完 dsh 升级后可能不兼容 | 锁 dsh 版本 `package.json` 用 `0.1.0-rc.5`, 升级走 minor release, 提前 1 周写 compat layer |
| **Node.js 业务方本机不一定装** | e2e 跑不起来 | CI 装 (`actions/setup-node@v4` with node-version: 24), 本地 README 强调 "需要 Node 22.19+" |
| **dsh 子进程是单线程** | 重活工具可能阻塞 | 跟 dsh 一致, 业务方在 defineTool 里用 `Promise` 异步 + `exec.signal` 取消, ma-harness 端 timeout 30s 兜底 |
| **Windows 上 Node.js 路径差异** | spawn 失败 | 用 `which::which("node")` 探测, fallback `where.exe node`; README 给手动配置 `node_path` 字段 |
| **conformance 9/9 不能 100%** | 暴露 ma-harness 跟 dsh 在 fixture 层的语义差 | 1-2 个 fixture 允许 skip, 标 `dsh_format_skip` 类别, 不影响其他 |
| **Rust dylib ↔ dsh 互操作的 phase 跳票** | 业务方不能混用 | P14 单独 phase, 不在 P13 范围 |

## 6. 验收标准 (P13 整体)

- [ ] 5 phase 全部完成
- [ ] `plugins/ma-harness-plugin-dsh-adapter/` 发布到 crates.io (跟其他 7 个 plugin 同等)
- [ ] `mah conformance --dsh-adapter` 跑通 9/9 dsh-snap = 100%
- [ ] 1 个真 dsh 插件 e2e 跑通 (k8s_pod_status 或 str-replace-editor)
- [ ] CI 加 e2e job, 全 3 OS 跑过 (windows + macos 需要 Node.js runner image)
- [ ] 文档 100% (中英双语)
- [ ] `mah dsh doctor` 命令上线, 自检通过
- [ ] memory 写 3-5 条新 entry (调研 / 设计 / 实现 / 已知限制)
- [ ] `mah info` 显示 dsh-adapter 状态

## 7. 后续路线 (P14+)

- **P14.1**: 互操作 (ma-harness 同时 load dylib + dsh 插件, 共享 ToolRegistry)
- **P14.2**: PTC (Code mode) 桥接 (`run_code` tool + generated TS SDK)
- **P14.3**: Cordis 事件 hook (`tools/pre-execute` permission gate 桥接 ma-harness approval)
- **P15.1**: dsh profile 多套 (headless / web 模式选择)
- **P15.2**: Web UI 桥接 (dsh-web ↔ ma-harness-tui)

## 8. 时间线 (估)

```
Week 1  P13.1 骨架
Week 2  P13.2 工具桥接
Week 3  P13.3 lifecycle
Week 4  P13.4 conformance
Week 5  P13.5 e2e + 文档
Week 6  buffer / review / 发布

Total: 5-6 周 (1 人全职)
```

## 9. 引用

- [DeepSeek Harness repo](https://github.com/deepseek-ai/deepseek-harness) (MIT)
- [Cordis meta-framework](https://github.com/cordiverse/cordis) (MIT, 来自 Koishi)
- [A Programming Paradigm for Spatiotemporal Composability](https://arxiv.org/abs/...) (Cordis 论文)
- [ma-harness 仓库内 dsh 相关引用](../../conformance-design.md) (`dsh_format` / `dsh_synthetic` / `dsh_snap`)
- Phase 11 P11-1 / P11-2: dsh 9/9 conformance 已经 100%, 是 P13 基础

---

**版本**: v1.0 (2026-08-21)
**变更**: 初版
**下次 review**: P13.1 完成时
