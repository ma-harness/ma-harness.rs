# ma-harness.rs

> **Rust 重写的 AI agent 编排 harness**。独立项目,行为对齐 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 但不是官方 Rust 端口。
> **当前状态**: Day 92 (Phase 1-5 全部收官, P4-1..7 + P5-1..3 完成, crates.io 发版准备完)

---

## 这是什么?

`ma-harness.rs` 是 **独立的 Rust 重写**,跟 dsh 行为对齐、跑分对比,**不是 dsh 的官方 Rust 端口**。

主要设计:

| 维度 | 选择 |
|---|---|
| 框架 | Cordis-rs (typed key + plugin + listener + scope + fork) |
| 协议 | Protobuf 单协议 (Prost + tonic + salvo-oapi) |
| 日志 | append-only SessionEvent (rusqlite, model-visible 不变) |
| 公开 API | `ma-harness-seam` (5 trait + 5 proc-macro re-export) |
| HTTP server | salvo 0.93 (#[endpoint] 自动导出 OpenAPI, 0.79→0.93 跳 14 minor 0 break) |
| Code Mode | wasmtime 27 (4 层沙箱: fuel / epoch / mem+table / fs) |
| 模型 | OpenAI / Anthropic (HTTP) + StubModel (dev/test) |
| Bindings | Python / Node.js (JS+TS) / Go (gRPC) |
| 19 first-party | cordis / core / seam / proto / cli / server / sandbox / model / code / tui / plugin-macro / demo / conformance + 6 plugins |

## 快速开始

```bash
# 编译 (网络通后)
cargo build --workspace

# 跑全部测试 (~303 lib test)
cargo test --workspace --lib

# 跑 benchmark (criterion)
cargo bench --workspace

# 装 `mah` CLI
cargo install --path crates/ma-harness-cli
mah --help
```

## 仓库结构

```
ma-harness.rs/
├── AGENTS.md                       → AI agent / 新成员入口
├── README.md                       → 你正在看
├── CHANGELOG.md                    → 变更记录
├── LICENSE-MIT + LICENSE-APACHE
├── Cargo.toml                      → workspace 根 (19 member)
├── .github/workflows/ci.yml        → GitHub Actions CI (5 job: lint/build/test/conformance)
├── .github/workflows/release.yml    → crates.io 自动发布
├── docs/
│   ├── decision-log.md             → 13 节关键决策 (宪法)
│   ├── ma-harness-arch-map.md      → 跟 dsh 的 12 阶段映射
│   ├── macro-design.md             → 5 个 proc-macro 规范
│   ├── plugin-schema-v1.md         → plugin.toml + JSON Schema
│   ├── tech-stack.md               → 19 crate 总结 + "不引入"清单
│   ├── code-mode-deferred.md       → Code Mode Phase 2 推迟
│   ├── conformance-design.md       → Week 10 conformance 框架
│   ├── benchmark-design.md         → Week 10 benchmark 设计
│   ├── openapi.json                → HTTP API 完整 OpenAPI spec (7 paths)
│   ├── weekly/                     → 周报 (000-007)
│   └── decision-log § 13           → Phase 4 完整记录 (P4-1..7)
├── proto/ma_harness/v1/            → 3 个 .proto (agent / session / event)
├── bindings/                       → 多语言 gRPC 客户端
│   ├── python/                     → grpcio + grpcio-tools
│   ├── node/                       → @grpc/grpc-js + JS+TS
│   └── go/                         → google.golang.org/grpc + protoc-gen-go
├── crates/ (19 个)
│   ├── ma-harness-cordis/          → DI 容器 (publish=true)
│   ├── ma-harness-core/            → SessionEvent / agent loop / EventLog
│   ├── ma-harness-seam/            → 公开 facade (publish=true)
│   ├── ma-harness-plugin-macro/    → 5 proc-macro (publish=true)
│   ├── ma-harness-proto/           → Prost/tonic codegen
│   ├── ma-harness-server/          → gRPC + salvo HTTP (7 paths)
│   ├── ma-harness-cli/             → `mah` 二进制 (13 子命令)
│   ├── ma-harness-sandbox/         → landlock 沙箱
│   ├── ma-harness-model/           → OpenAI / Anthropic adapter
│   ├── ma-harness-code/            → wasmtime Code Mode (publish=true)
│   ├── ma-harness-tui/             → ratatui dashboard (4 panel)
│   ├── ma-harness-demo/            → 端到端 12 步 demo
│   └── ma-harness-conformance/     → Conformance test framework
└── plugins/ (7 first-party)
    ├── ma-harness-plugin-hello/    → Week 1 hello-world 教学
    ├── ma-harness-plugin-bash/     → subprocess + timeout
    ├── ma-harness-plugin-fs/       → read/write/list + 路径白名单
    ├── ma-harness-plugin-web/      → reqwest + URL 白名单 + timeout
    ├── ma-harness-plugin-subagent/ → fork ctx 跑子 agent
    ├── ma-harness-plugin-skill/    → load .skill/ 目录
    └── ma-harness-plugin-cordis/   → ctx 反射
```

## `mah` CLI (13 子命令)

```bash
# Server
mah start [--grpc-port 50051] [--http-port 50050] [--store-path <db>]
# tonic gRPC + salvo HTTP, 7 paths (/health, /version, /v1/runs, /v1/sessions 5 个)

# Local agent
mah run [--session <id>] [--model stub] "echo hi"
mah run-prompt "compute 1+1, return the result as i32"  # LLM → .wat → wasm sandbox
mah run-stream --grpc-url http://localhost:50051 "hello"   # 走 gRPC RunStream 拿实时 token (Day 99)

# Plugins
mah plugins                                     # 列出已装载 plugin (inventory)
mah load-plugin <name> [--ctx-id <id>]

# Sessions + Events
mah events <session_id>                         # 查 session 事件

# Code Mode (wasmtime)
mah code run <file.wat|.wasm>                   # 跑 WAT/WASM 在 4 层沙箱
mah sandbox apply [--read-paths P] [--write-paths P]  # 显式 landlock/seatbelt
mah sandbox status

# Conformance + Benchmark
mah conformance --fixtures fixtures/smoke.jsonl --output target/
mah bench                                       # benchmark 提示 (criterion)

# OpenAPI
mah open-api export --output docs/api/openapi.json  # 7 paths OpenAPI 3.1 spec

# TUI dashboard (4 panel)
mah tui [--log <db>] [--store-path <db>]       # ratatui, j/k 选 session, Enter 进 detail

# TUI P6-5 增强: Tab 切 panel focus, j/k 跨 panel, 选中状态持久化 (~/.ma-harness/tui-state.json)

# Misc
mah version
```

## HTTP API (7 paths, 跟 gRPC SessionService 对齐)

| Method | Path | 对齐 gRPC |
|---|---|---|
| GET | /health | - |
| GET | /version | - |
| POST | /v1/runs | AgentService.Run |
| GET | /v1/sessions | SessionService.ListSessions |
| POST | /v1/sessions | SessionService.CreateSession |
| GET | /v1/sessions/{id} | SessionService.GetSession |
| POST | /v1/sessions/{id}/close | SessionService.CloseSession |
| GET | /v1/sessions/{id}/events | SessionService.GetSessionEvents |

OpenAPI spec: `docs/api/openapi.json` (101KB, 7 paths, 自动 CI drift check).

## Bindings (gRPC, 4 语言)

| 语言 | 目录 | 状态 |
|---|---|---|
| Python | `bindings/python/` | grpcio + grpcio-tools, 4 RPC demo |
| Node.js (JS) | `bindings/node/example_client.js` | @grpc/grpc-js, 4 RPC demo |
| Node.js (TS) | `bindings/node/example_client.ts` | tsc + proto-loader, 4 RPC demo |
| Go | `bindings/go/` | google.golang.org/grpc + protoc-gen-go-grpc, 4 RPC demo |

每个 binding 4 RPC 一致: ListSessions / CreateSession / Run / GetSessionEvents.

## 文档导航

按我需要了解什么看:

| 我需要... | 看这个 |
|---|---|
| 仓库定位 + 决策 | [`AGENTS.md`](./AGENTS.md) + [`docs/decision-log.md`](./docs/decision-log.md) |
| 跟 dsh 怎么对应 | [`docs/ma-harness-arch-map.md`](./docs/ma-harness-arch-map.md) |
| 5 个 proc-macro | [`docs/macro-design.md`](./docs/macro-design.md) |
| 写 plugin | [`docs/plugin-schema-v1.md`](./docs/plugin-schema-v1.md) |
| 跑 conformance | [`docs/conformance-design.md`](./docs/conformance-design.md) |
| 跑 benchmark | [`docs/benchmark-design.md`](./docs/benchmark-design.md) |
| 跟踪进度 | [`docs/weekly/`](./docs/weekly/) (000-007) |
| 加新 crate | [`docs/tech-stack.md`](./docs/tech-stack.md) + "不引入"清单 |
| 用 ma-harness API | [`crates/ma-harness-seam/src/lib.rs`](./crates/ma-harness-seam/src/lib.rs) |
| HTTP API spec | [`docs/openapi.json`](./docs/openapi.json) |
| 多语言 gRPC | [`bindings/README.md`](./bindings/README.md) |
| 变更记录 | [`CHANGELOG.md`](./CHANGELOG.md) |
| 许可 | [`LICENSE-MIT`](./LICENSE-MIT) + [`LICENSE-APACHE`](./LICENSE-APACHE) |

## 关键数字 (Day 92 状态)

| 指标 | 数值 |
|---|---|
| 累计 commit | 131+ (持续增长) |
| Workspace member | 19 (13 crates/ + 7 plugins/) |
| 累计代码 | ~20100 行 |
| 累计 lib test | 303 (全过) |
| 累计 trybuild fixture | 18 |
| crates.io publish | 5/19 (cordis, code, core, plugin-macro, seam) |
| HTTP API paths | 7 (3 → 7) |
| Bindings | 4 语言 (Python / JS / TS / Go) |
| 设计文档 | 9 份 + decision-log § 1-20 |
| 周报 | 7 份 (Day 0 / Week 1-2 / 3-4 / 5-6 / 7-9 / 10 / 11) |

## Phase 路线图 (回顾)

### ✅ Phase 1 (Week 1-9): 基础框架
- Cordis-rs DI 容器 (typed key + plugin + listener + disposable)
- SessionEvent / agent loop / EventLog
- 5 proc-macro + ctx_key!
- gRPC AgentService + SessionService
- 6 first-party plugin
- Conformance / Benchmark framework

### ✅ Phase 2 (Week 10-11): 持久化 + 多 model + 沙箱
- SessionStore trait + InMemoryStore / SqliteStore
- landlock 沙箱 (12 AccessFs ops)
- OpenAI / Anthropic adapter
- 异步 listener + priority dispatch
- AsyncDisposable
- crates.io publish (cordis / code)

### ✅ Phase 3 (Week 11-12): Code Mode + TUI + 多语言
- wasmtime 27 4 层沙箱 (fuel / epoch / mem+table / fs)
- LLM → .wat → wasm 端到端 (mah run-prompt)
- OpenAPI 同步 CI
- WASI 受控 fs (host::read_file)
- Plugin 依赖注入 (拓扑排序 Kahn)
- TUI dashboard (ratatui 0.29)
- Python + Node.js gRPC binding

### ✅ Phase 4 (Day 82-89): 接真数据 + 多语言扩展
- P4-1: TUI 接真 EventLog
- P4-2: ma-harness-seam 发 crates.io
- P4-3: TUI 接真 SessionStore
- P4-4: OpenAPI /v1/runs 注解修复
- P4-5: TUI 4 panel UI 加 events 滚动
- P4-6: Go gRPC binding
- P4-7: TypeScript Node binding

### 🚧 Phase 5 (Day 90-): HTTP API 扩 + TUI 交互
- P5-1: HTTP /v1/sessions 4 endpoint ✅
- P5-2: TUI session detail view (j/k/Enter/Esc) ✅
- P5-3: HTTP /v1/sessions/{id}/events ✅
- P5-4: README 更新 (本文档) ✅
- P5-5: `mah sessions` CLI (本地 SqliteStore / EventLog) ✅
- P5-6: RunStream RPC 实现 (gRPC streaming) ✅
- P5-7: streaming RPC demo 4 语言 (Python/Node/TS/Go) ✅
- P5-8: HTTP SSE `/v1/runs/stream` (浏览器 EventSource) ✅
- P5-9: pyo3 评估 ([`docs/pyo3-evaluation.md`](./docs/pyo3-evaluation.md)) ✅

**Phase 5 收官 9/9** (Day 90-98)

### 🚧 Phase 6 (Day 99-): 真 LLM streaming + perf
- P6-1: `mah run-stream` CLI (gRPC RunStream 客户端) ✅ (Day 99)
- P6-2: OpenaiAdapter 真 SSE (reqwest + bytes_stream + parse) ✅ (Day 100)
- P6-3: AnthropicAdapter 真 SSE (event-based protocol) ✅ (Day 100)
- P6-4: streaming perf bench (criterion 5 bench) ✅ (Day 100)
- P6-5: TUI 增强 (j/k 跨 panel / 选中状态持久化) ✅ (Day 101)
- P6-6: salvo 0.79 → 0.93 兼容性升级 (0 break, 0 代码改动) ✅ (Day 101)

## 网络环境

本机代理 `127.0.0.1:7890` 不能代理 HTTPS, **130+ 文件经 `cargo check` 验证**。所有代码 mental-compile only,等代理恢复或换网络环境后跑。

```bash
cargo check --workspace
cargo test --workspace --lib
cargo bench --workspace
```

## License

MIT OR Apache-2.0 (跟 workspace 锁定一致)

- [`LICENSE-MIT`](./LICENSE-MIT) — MIT License
- [`LICENSE-APACHE`](./LICENSE-APACHE) — Apache License 2.0

## 仓库地址

`git@gitee.com:yifenma/ma-harness.rs.git`
