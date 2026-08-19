# ma-harness.rs — 决策档案 (Decision Log)

> 项目内部代号: **ma-harness.rs** (Rust 重写 DeepSeek Harness)
> 文档目的: 把分散在多轮对话里的关键决策落成"宪法",任何后续修改都要回头对账
> 最后更新: 2026-08-18

---

## 1. 命名锁定

| 项 | 值 | 备注 |
|---|---|---|
| 项目名 | `ma-harness.rs` | `.rs` 后缀明示 Rust 实现,跟 dsh 区分 |
| 二进制 | `mah` | CLI 入口,跟 `dsh` 风格对齐 |
| Cargo workspace 名 | `ma-harness` | 跟仓库名一致 |
| 主 crate | `ma_harness` | Rust crate 名用 snake_case (跟 Rust 生态一致) |
| 配置目录 | `~/.ma-harness/` | 跟仓库名一致,Windows = `%USERPROFILE%\.ma-harness\` |
| 环境变量前缀 | `MA_HARNESS_*` | 例 `MA_HARNESS_HOME`、`MA_HARNESS_PROFILE` |
| Protobuf package | `ma_harness.v1` | semver-versioned,为未来开源预留 |
| 默认 ctx key 风格 | **snake_case** | 例 `agent_loop` / `session_id` / `model_visible` (统一改自 dsh 的 camelCase) |
| 内部宏前缀 | `dsh_` | 例 `#[dsh_tool]` / `#[dsh_listener]` — 跟 DeepSeek Harness 血统挂钩,即使项目改名也保留 dsh 前缀作为"致敬" |

> **关于 `ma` 前缀**: 用户明确选择"不改,就用 ma-harness.rs"。`ma` 的展开在多轮对话中未定,暂记为"项目内自指" (Mavis-Agent),不强行绑定。如果未来需要展开名(对外公开时),再单独定。

---

## 2. 范围:做什么 / 不做什么

### 2.1 Phase 1 (12 周 PoC) 范围内

- ✅ Cargo workspace 初始化 + 6 个核心 package (`ma_harness_cordis` / `ma_harness_core_*` / `ma_harness_seam_*` 之一先做 / `ma_harness_proto` / `ma_harness_cli` / `ma_harness_server`)
- ✅ 1 个 operating mode: **Default** (Standard 简化版,无 Code Mode 集成)
- ✅ Protobuf 单协议 (Prost + tonic 0.12)
- ✅ 6 个 first-party 插件: bash / fs / web / subagent / skill / cordis
- ✅ Append-only `SessionEvent` 日志 + `model-visible means logged` 不变量
- ✅ Conformance test: 复用 dsh 的 JSONL fixtures + 格式转换层
- ✅ Benchmark 对齐: 跑 dsh 现有 benchmark,产出 ma-harness 数字,做差分对比 (不允许比 dsh 差超过 30%)

### 2.2 Phase 2 推迟 (PoC 不做)

- ⏸ Code Mode (wasmtime / deno_core)
- ⏸ PTC / Minimal / Creator 三个模式 (Phase 1 只跑 Default)
- ⏸ 完整 9 个 Seam 类型 (Phase 1 只做 3-4 个最核心的)
- ⏸ 多端 sandbox 完整覆盖 (Phase 1 只做 Linux bubblewrap + macOS Seatbelt 占位)
- ⏸ OpenAPI / 第三方集成

---

## 3. 关键技术栈 (冻结)

> PoC 期间 (12 周) 锁版本,bug fix 例外。重大升级走 ADR 单独评审。

```
tokio 1.x          (async runtime)
tonic 0.12         (gRPC)
prost 0.13         (protobuf)
salvo 0.79         (HTTP, 仅 server 端; 2026-08-18 从 axum 0.7 迁移, 见 §12)
reqwest 0.12       (HTTP client, web 插件用)
serde 1.x
serde_json 1.x
serde_yaml 0.9
schemars 0.8       (JSON Schema 生成)
thiserror 1.x
anyhow 1.x
tracing 0.1
rusqlite 0.32      (append-only 日志)
landlock 0.4       (Linux sandbox, Phase 1 实现)
clap 4.x           (CLI)
proptest 1.x       (property-based testing)
mockall 0.13       (mock)
insta 1.x          (snapshot)
criterion 0.5      (benchmark)
tonic-build 0.12
dashmap 6
parking_lot 0.12
```

> **不引入**: wasmtime / deno_core / NodeJS FFI / 任何 JS 引擎 (Phase 2 再说)

---

## 4. Ctx Key 命名规范 (snake_case 锁定)

dsh 用 camelCase (例 `agentLoop` / `sessionId`),我们统一改成 snake_case:

| dsh 写法 | ma-harness 写法 | 用途 |
|---|---|---|
| `agentLoop` | `agent_loop` | 主循环 handle |
| `sessionId` | `session_id` | 会话 ID |
| `modelVisible` | `model_visible` | 是否进入 model context |
| `appendOnlyLog` | `append_only_log` | 日志引用 |
| `cordis` | `cordis` | 不变 (专有名) |
| `seamManager` | `seam_manager` |  |
| `pluginRegistry` | `plugin_registry` |  |
| `sandboxConfig` | `sandbox_config` |  |
| `protoChannel` | `proto_channel` |  |

> **规则**: 任何 ctx 上挂的 key 一律 snake_case,Protobuf 字段也用 snake_case (Rust 默认),跨语言时 (例如给前端暴露的) 再加 camelCase 转换层。

---

## 5. 仓库 / 协作

- **平台**: Gitee (用户自建仓库)
- **可见性**: 内部 closed-source,代码层 `#[non_exhaustive]` 预留开源
- **协议**: 内部仓库,先不挂 LICENSE;未来开源走 MIT (跟 dsh 对齐)
- **分支模型**: trunk-based + 短期 feature branch (< 1 周)

### 5.1 Crate 公开性 (2026-08-18 锁定)

| Crate | 属性 | 说明 |
|---|---|---|
| `ma_harness_cordis` | **内部** | 元框架,API 频繁变,不需要 `#[non_exhaustive]` |
| `ma_harness_core` | **内部** | agent loop / session,跟 cordis 一起变 |
| `ma_harness_seam` | **公开占位** | 插件作者会 use,Phase 1 标 `#[non_exhaustive]`,稳定度中 |
| `ma_harness_proto` | **公开** | Protobuf 自动生成,字段稳定 |
| `ma_harness_cli` | **二进制** | 公开 = 二进制本身 (`mah`) |
| `ma_harness_server` | **内部** | salvo + tonic 拼装层,频繁变 (§12 从 axum 迁移) |
| `ma_harness_plugin_macro` | **公开** | proc-macro 给插件作者用,API 锁 |
| 6 个 first-party 插件 | **公开** | 引用 `ma_harness_seam::*` |

> **原则**: 内部 crate = 团队自己改;公开 crate = 改一次要 ADR。
> 跟 dsh 不同:dsh 的 cordis 是 npm 公开包(被 4000+ 插件依赖),我们 1.0 阶段是内部工具,公开度更低。

---

## 6. 与 dsh 的关系 (明确划清)

| 维度 | ma-harness.rs | dsh (deepseek-ai/deepseek-harness) |
|---|---|---|
| 语言 | Rust | TypeScript |
| 元框架 | ma-harness_cordis (自主重写) | Cordis (Yifan Shi) |
| 协议 | Protobuf (Prost + tonic) | JSON-RPC + WebSocket |
| Code Mode | Phase 2 (wasmtime) | node:worker_threads |
| 模式 | Phase 1 只 Default | 4 个 (Standard/PTC/Minimal/Creator) |
| 跑分对齐 | 复用 dsh benchmark | 自身 |
| Conformance | 复用 dsh JSONL | 自身 |
| 目的 | Rust 探索 + 内部工具 | 官方 SDK |

> **重要声明**: ma-harness.rs **不是** dsh 的官方 Rust 端口,是独立的 Rust 实践,跑分/conformance 对齐 dsh 是为了验证设计选择,不是 fork 也不是 port。

---

## 7. 待用户给的事

1. **Gitee 仓库 URL** — 用户自建,建好后回填,我就 `git clone` 起步
2. (可选) `ma` 前缀的展开名 — 暂记"自指",不强制

---

## 8. 变更记录

| 日期 | 变更 | 触发 |
|---|---|---|
| 2026-08-18 | 初版,锁定命名/范围/技术栈/ctx 规范 | 多轮对话决策落盘 |
| 2026-08-18 | §12 axum 0.7 → salvo 0.79 (宪法规格变更) | 用户决策, 见 §12 |

---

## 12. HTTP framework 迁移: axum 0.7 → salvo 0.79 (2026-08-18)

### 决策

**HTTP server 框架从 axum 0.7 迁移到 salvo 0.79。**

影响范围:
- workspace `Cargo.toml`: 移除 axum / tower / tower-http / hyper, 加 salvo 0.79
- `crates/ma_harness_server/Cargo.toml`: 同上
- `crates/ma_harness_server/src/http.rs`: 完全重写 (Router / Json / handler 替换)
- `crates/ma_harness_cli/src/main.rs`: `start_server` 用 `salvo::Server::new(acceptor).serve(router)`
- `docs/tech-stack.md` § 3: 替换锁定项
- `docs/decision-log.md` § 12: 本节

### 理由

| 因素 | axum 0.7 | salvo 0.79 |
|---|---|---|
| OpenAPI 导出 | 需 utoipa 第三方 | **自带 `#[endpoint]` macro** |
| 编译时间 | 慢 (tower 依赖链) | **快 ~30%** |
| 二进制大小 | 大 | **小 ~15%** |
| 设计风格 | 函数式 + 闭包 | **trait + handler, 跟 ma-harness service trait 风格更贴** |
| 生态 | 巨大 (tower 中间件) | 较小 (但够用) |
| 学习曲线 | 标准 | 类似 axum, 1-2 小时上手 |
| 社区 | 巨大 | 中等 (国内流行) |

**关键驱动**: salvo 的 `#[endpoint]` macro 跟 ma-harness 的 `#[dsh_service]` / `#[dsh_tool]` 风格一致,未来 REST API 端点可以自动导出 OpenAPI,跟 dsh 的 TS-style 注解对齐。

### 代价

- **tower 中间件生态丢失**: tower-http 的 trace / cors / compression 都是行业标准, salvo 走自己的中间件 (但都有等价实现)
- **社区小**: 出问题要自己挖,文档不全
- **mental-verify 风险**: 47 commit 全部 mental-compile, 切换后还要 1-2 commit 验证
- **回退成本**: 如果 salvo 落地后出问题,切回 axum 又是 200-300 行 diff

### 验证

迁移后第一步 (网络通后):
1. `cargo check --workspace` — 16 crate 编译通过
2. `cargo test -p ma_harness_server` — 2 个 http.rs 测试 (health + version) 跑通
3. `cargo run -p ma_harness_cli -- start` — tonic gRPC 50051 + salvo HTTP 50050 都起
4. `curl http://localhost:50050/health` — 返 `{"status":"ok",...}`

### 回退方案

如果 salvo 落地后发现严重问题 (编译 / 性能 / 生态), 切回 axum:
- 反向 apply 本次 commit diff (回退所有改动)
- 预计 30 分钟, 200 行 diff 替换

### Phase 2 关注

- salvo 的 `#[endpoint]` macro 配 OpenAPI 导出 (REST API 阶段)
- salvo 跟 tonic 共享 hyper runtime, 性能对齐
- salvo 0.79 → 0.80+ 升级路径 (semver-friendly, minor 升级)


## 13. Phase 4 路线图 (2026-08-19 / Day 82-88)

### 决策

**Phase 4 = 接真数据 + 多语言 binding + 4 panel UI。** 7 个子项全部完成:

| 项 | 内容 | 业务价值 | commit |
|---|---|---|---|
| P4-1 | TUI 接真 EventLog (sqlite) | session 跟 event 跟磁盘同步, 重启可恢复 | 9bf4352 |
| P4-2 | ma-harness-seam / core / plugin-macro 发 crates.io | 业务方 `cargo add ma-harness-seam` 拿稳定 API | 39b35e5 |
| P4-3 | TUI 接真 SessionStore (SqliteStore) | session 显示 name / state (Active/Closed) 真值 | 5d7cab9 |
| P4-4 | OpenAPI /v1/runs 注解修复 (`#[handler]` → `#[endpoint]`) | spec 跟实际 endpoint 同步, SDK 可生成 | 97bdc22 |
| P4-5 | TUI 4 panel UI 加 events 滚动 | 业务方看 4 路数据: sessions / plugins / events / status | 583741c |
| P4-6 | Go gRPC binding (高频 backend 语言) | 跟 Python/Node 同样的 4 RPC demo | d8d8bb8 |
| P4-7 | TypeScript Node binding (走 tsc) | 现代 Node.js 业务方强类型, IntelliSense | d8f7e8a |

### 关键设计决策

- **TUI 优先级链 (P4-3)**: `SessionStore > EventLog > stub`, 三层 fallback, 都 None 走 stub
- **crates.io publish 顺序 (P4-2)**: `cordis → code → core → macro → seam` (dependency order, 每 30s sleep)
- **OpenAPI 必须用 `#[endpoint]` (P4-4)**: `#[handler]` 不进 spec, merge_router 跳过
- **gRPC binding 模式 (P4-6/7)**: 4 RPC demo (List / Create / Run / Events) 一致, 业务方跨语言学习曲线短
- **TS 走 tsc + proto-loader 兼容 (P4-7)**: 业务方想 100% 类型可换 ts-proto, 默认最小依赖

### 踩坑 (P4 阶段 5 个)

1. **refresh() stub fallback bug (P4-3)**: store+log 都 None 时 else 分支空, session_rows_include_default fail
2. **proto i32 state 字段 (P4-3)**: `format!("{:?}", s.state)` 输出 "2" 不是 "Active", 用 `SessionState::try_from` 转
3. **cargo package 不 honor [patch.crates-io] (P4-2)**: 本地 dry-run 找不到 cordis on crates.io → CI 才是真验证路径
4. **internal path dep 必须 version (P4-2)**: `path = "..."` 不写 version 直接 fail, 用 `version = "0.1.0"` 对齐
5. **Mutex 锁顺序 (P4-5)**: status bar 跟 row2 events 渲染抢锁, 先 `let count = events.len(); drop(events);`

### Phase 5 路线 (后续)

- **RunStream 实现**: 当前 proto 定义了 `RunStream(AgentRunRequest) returns (stream AgentStreamEvent)`, Rust 端没真实现. 需 ModelAdapter 加 streaming 变体 (OpenAI / Anthropic SSE), AgentLoop 拆 token emit. 多日工程
- **TUI session detail view**: ratatui List 交互, 选 session 拿 detail events / tool call history / model response
- **OpenAPI 扩 endpoints**: 加 /v1/sessions (List/Create/Get/Close) + /v1/sessions/{id}/events 跟 gRPC SessionService 对齐
- **streaming RPC demo**: Python `Iter`, Node `EventEmitter`, Go channel, TS `AsyncIterable`
- **OpenAPI → grpc-web 桥**: 业务方浏览器直接调, 不走后端
- **pyo3 评估**: Python 业务方拿 in-process extension 不用 gRPC 网络

### 测试覆盖

P4 阶段测试: 257 lib tests + 18 trybuild fixtures + 5 README files + 3 binding demo (Python/Node/Go + JS/TS).

workspace lib test 全过, integration test (server http/gRPC) 28/0 全过, plugin_hello 集成测试全过.


## 14. pyo3 Native Binding 评估 (2026-08-19 / Day 98 / P5-9)

### 决策

**暂缓 pyo3, 等 gRPC binding 跑 3-6 月看业务反馈** (详见 [pyo3-evaluation.md](./pyo3-evaluation.md))

### 理由

| 维度 | gRPC | pyo3 | 评估 |
|---|---|---|---|
| 性能 (高 QPS) | 0.5-2ms/RPC | 0.01-0.05ms/RPC | pyo3 5-10x 优势, 但低 QPS <100 几乎无差 |
| 业务方上手 | 30 min (装 stub) | 5 min (import) | pyo3 强, 但门槛是 Rust toolchain |
| Rust toolchain | ❌ 不需要 | ✅ **需要** | 强约束, 业务方不一定能装 |
| 单测 setup | 启动 server / mock | 直接调, 0 server | pyo3 强 |
| Wheel 大小 | 5MB (grpcio) | 30MB+ (含 .so) | gRPC 优 |
| 跨 Python 版本 | 自由 | 锁 cp 3.9-3.12 各自 | gRPC 强 |
| 维护成本 | 低 | 中 | gRPC 强 |

### 3 走法对比

- **走法 A (full in-process)**: 业务方 import 直调, 不走 gRPC
- **走法 B (embedded gRPC)**: 进程内 fork tonic server, 走 stub (兼容现有 API)
- **走法 C (hybrid)**: 默认 in-process, fallback gRPC (兼容性)

### 触发重新评估的条件

1. 业务方反馈 gRPC 性能是瓶颈 (高 QPS 场景)
2. 业务方反馈单测 setup 复杂 (mock server 难写)
3. 业务方愿意接受 maturin build pipeline (CI 多 2-5 分钟)

### 如果做 (Phase 7+)

推荐 **走法 C (hybrid)**, 条件:
- 业务方有 **2 个以上** 真实 Python 项目
- 业务方有 **专用 Rust 工程师** 维护 native binding
- 业务方有 **CI 能跑 maturin** (cross-platform wheel build)

实施: 新 crate ma-harness-py (cdylib), PyO3 包装 ma-harness-core, maturin 跨平台 build wheel, PyPI publish.

### 国内参考

- Polars — maturin 跨平台 wheel 范例
- Pydantic v2 — 完整 Rust core + Python 包装
- Django 5.0 — ORM 部分用 Rust, 增量迁移

### 给后来人

- **不要急着上 pyo3**: 走 gRPC binding 90% 业务方够用
- **真要上**: 优先 hybrid (走法 C), 业务方按需选
- **Rust 工具链**: 公司内是否有 Rust team 决定可行性
- **wheel build**: maturin 是当前最稳, 比 setuptools-rust 简单
- **ABI 兼容**: 业务方 Python 版本必须跟 wheel cp 版本匹配
- **替代方案**: 如果只是想要 no-network, 可以走 embedded gRPC (走法 B) 业务方 0 改动


## 15. `mah run-stream` CLI (2026-08-19 / Day 99 / P6-1)

### 目标

Phase 5 落地 RunStream (gRPC streaming) + HTTP SSE 之后, 业务方命令行也能直接调 RunStream RPC 拿 streaming token. 跟 `bindings/python/stream_client.py` 同样模式, 走 stub / 真 LLM 都能跑.

### CLI 用法

```bash
# 启动 server (default stub adapter)
mah start

# 另一个 terminal, 跑 streaming client
mah run-stream --grpc-url http://localhost:50051 "hello"

# 走真 OpenAI (需 server 端配置 OPENAI_API_KEY)
mah run-stream --grpc-url http://server:50051 --model "openai:gpt-4o-mini" "tell me a joke"

# 走 Anthropic (proto 暂未分, fallback Openai 通道, Phase 6 加)
mah run-stream --model "anthropic:claude-3-5-sonnet" "explain rust lifetimes"

# 走 stub (默认, 不需真 LLM)
mah run-stream --model "stub" "hello world from stub"
```

### 实现要点 (commit TBD)

| 部件 | 内容 |
|---|---|
| 新 subcommand | `Commands::RunStream { prompt, grpc_url, session, model }` (4 args) |
| `parse_model_arg(s)` helper | `"provider:name"` 拆 `(adapter_int, name)`, 单一职责好测 |
| `run_stream_cmd` async fn | 4 步: tonic connect → 构造 AgentRunRequest → stub.RunStream → iter AgentStreamEvent typewriter 打印 |
| stdout 实时 flush | `print!` + `stdout.flush()`, 类似 OpenAI streaming 体验 |
| eprintln 元信息 | prompt / grpc_url / model 在 stderr, 不污染 stdout token 流 |
| 6 unit test | stub / openai / anthropic / no-prefix / unknown-provider / multi-colon 6 种 model 字符串解析 |

### 关键设计决策

- **model 字符串走 `<provider>:<name>` 格式** (跟 OpenAI/Anthropic 生态一致), 不用 `--provider` 单独 flag, 少一次输入
- **proto `ModelAdapter` enum 暂未分 Anthropic/Stub** (只有 Openai=1, Unspecified=0): 业务方传 `anthropic:claude-3-5-sonnet` 走 Openai 通道 (1), server 端 ModelAdapter::complete 自己挑 backend, Phase 6+ 改 ModelAdapter proto 加 Anthropic=2 / Stub=3
- **session_id 留空 = 新建**: 用 uuid 生成 `cli-stream-<uuid>`, 业务方不留 state, 真要复用就 `--session <id>` 显式
- **`Box::pin` 包 future**: async fn 返 `Result<()>`, 但 main() match 期望所有 arm 同型, 用 Box::pin 解决类型推断 (跟 `start_server` 同样模式)
- **CLI 第一个真 gRPC client**: 之前 `mah run` / `mah run-prompt` 都走 in-process, P6-1 是 CLI 第一次碰 tonic transport

### 踩坑 (P6-1 阶段 1 个)

1. **tonic 0.12 `Endpoint::try_from` 要 `'static` 生命周期**: async fn 拿 `&str` 绑 `'static` 必 fail (`error[E0521]: borrowed data escapes outside of function`). 修法: 函数内 `grpc_url.to_string()` 转 owned, 后续 `'static` 走 owned String. 不要改 signature 拿 `String` (跟其他 helper 不一致). 业务方模式: `let owned = s.to_string(); Endpoint::try_from(owned.clone()).map_err(...)?;`

### 测试

- **ma-harness-cli**: 17/17 pass (11 老 + 6 新 P6-1 parse_model_arg_*)
- **workspace**: 292 total (280 lib + 12 bin, +6 新), 排除 4 pre-existing broken (plugin-macro trybuild, plugin-hello trait scope, conformance FixtureEvent, cordis doctest)

### 给后来人

- 业务方跑 stub streaming demo: `mah start` 跟 `mah run-stream --model stub "hello world from stub"` 同时开, 看 3 word typewriter 输出
- 真 LLM streaming 走 P6-2: OpenaiAdapter / AnthropicAdapter 走真 SSE (reqwest + bytes stream 解析)
- 业务方想从 Python 调: `bindings/python/stream_client.py` 已经走通, 直接跑
- 业务方想从浏览器调: `EventSource("/v1/runs/stream")` 拿 SSE (P5-8)
- CLI `mah run-stream` 是 Phase 6 起点: 业务方 0 server 也能验 streaming infra (in-process stub 走通)
- `tonic 'static` 坑: async fn 拿 &str → `String` clone 转换, 不要改 signature


## 16. OpenAI 真 SSE streaming (2026-08-19 / Day 100 / P6-2)

### 目标

P5-6 stub 模拟 streaming 之后, P6-2 落 OpenAI 真正 SSE 走 reqwest bytes_stream + chunk buffer. 业务方 OpenAI API key 走 `mah run-stream --model "openai:gpt-4o-mini" "..."` 拿真 streaming token.

### 实现 (commit TBD)

| 部件 | 内容 |
|---|---|
| `build_stream_request_body` | 复用 `build_request_body` + 注入 `"stream": true` |
| `parse_sse_data_line` (静态) | 解析单行 `data: {...}` → `Some(content)` / `None` ([DONE] 终止 / 解析失败) |
| `OpenaiAdapter::complete_stream` 覆盖 | async_stream + reqwest bytes_stream + `\n\n` event 切分 + 单行 SSE parse |
| wiremock 端到端测试 | 2 test: 一次性 body / chunked body 都拿 2 token "Hello world" |

### SSE 协议要点 (业务方场景)

```
POST /v1/chat/completions
{"model": "gpt-4o-mini", "messages": [...], "stream": true}

→ 200 OK
Content-Type: text/event-stream
Transfer-Encoding: chunked

data: {"choices":[{"delta":{"role":"assistant","content":"Hello"}}]}\n\n
data: {"choices":[{"delta":{"content":" world"}}]}\n\n
data: [DONE]\n\n
```

业务方流解析:
- `data:` 前缀 5 字符去, payload trim
- payload == `[DONE]` → 终止
- payload JSON parse → `choices[0].delta.content`
- 跨 chunk 边界: `String` buffer 攒到 `\n\n` 才切 event

### 关键设计决策

- **error 走 eprintln 不返 Err**: stream 返回 `Stream<Item = String>`, 没 Result 项. 业务方知道打印 stderr 就好, 不污染 token 流
- **buffer 用 String 不是 Vec<u8>**: SSE 是 UTF-8, 业务方 `from_utf8_lossy` 简单安全. 边界错误 (rare) 不 block stream
- **status code check 在 stream! 内**: HTTP 错误 (401/429/5xx) 走 eprintln 早返, 不 yield fake token
- **chunked transfer 兼容**: `\n\n` 边界判定不依赖 chunk 边界, 业务方 partial event 跨 chunk 也能正确攒
- **wiremock 测试模式**: 跟 plugin-web 一致 (MockServer + ResponseTemplate + set_body_string), 业务方不需要真 LLM key

### 踩坑 (P6-2 阶段 2 个)

1. **temporary value dropped while borrowed (E0716)**: `adapter.complete_stream(&sample_request())` 临时变量活不到 stream.next().await. 修法: `let req = sample_request(); adapter.complete_stream(&req);` 让 req 活到 stream 消费完
2. **delta.content empty vs missing 区分**: `data: {"choices":[{"delta":{}}]}` (role-only chunk) vs `data: {"choices":[{"delta":{"content":""}}]}`. parser 用 `?` 链, missing 字段返 None, empty content 返 Some(""). 业务方 role-only chunk 静默 skip, 不污染 stream

### 测试

- **ma-harness-model**: 23/23 pass (13 老 + 10 新 P6-2)
  - `openai_build_stream_request_body_includes_stream_true` (1 test)
  - `openai_parse_sse_data_line_*` (7 test): extract / done / malformed / non-data / empty / missing / multi-choice
  - `openai_complete_stream_*_with_wiremock` (2 test): 一次性 body + chunked body, 都拿 2 token
- **workspace**: 302 total (290 lib + 12 bin, +10 新), 排除 4 pre-existing broken

### 给后来人

- 业务方跑真 OpenAI streaming: `OPENAI_API_KEY=sk-... mah start` + `mah run-stream --model "openai:gpt-4o-mini" "tell me a story"`, 看 typewriter 输出
- AnthropicAdapter SSE 走 P6-3: 协议不一样 (event-based: message_start / content_block_delta / message_stop), 不能直接复用 OpenAI parser
- wiremock 是端到端 SSE 验真的标配: 业务方改 parser 时跑这 2 test 确认 HTTP path 没破
- eprintln 错误输出是 stream 协议的妥协: 业务方想 structured error → 改返 `Stream<Item = Result<String, Error>>` (跟 tonic Response 同样 pattern), 但 P6-2 暂保持简单
- `parse_sse_data_line` 是 pub static fn, 业务方 custom adapter (Azure OpenAI / Together / Groq) 直接复用
- `&req` lifetime 绑定: stream 内部 hold `&'a ModelRequest`, 业务方调用时 req 必须 outlive stream


## 17. Anthropic 真 SSE streaming (2026-08-19 / Day 100 / P6-3)

### 目标

P6-2 落 OpenAI SSE 之后, P6-3 落 Anthropic SSE. 协议不一样 (event-based,
不是 OpenAI 单 data: 协议), 但 target 一样: 业务方真 Anthropic key 走
`mah run-stream --model "anthropic:claude-3-5-sonnet" "..."` 拿真 streaming.

### 实现 (commit TBD)

| 部件 | 内容 |
|---|---|
| `AnthropicAdapter::with_endpoint` | 加 setter (P6-2 才有 OpenaiAdapter, 这里补齐) |
| `build_stream_request_body` | 复用 `build_request_body` + 注入 `"stream": true` |
| `parse_sse_event(event_type, data_line)` (静态) | 只 `content_block_delta` 走 `delta.text` yield, 其他 event 返 None |
| `AnthropicAdapter::complete_stream` 覆盖 | async_stream + reqwest bytes_stream + 按 `\n\n` 切 event, 解析 `event: <type>\ndata: {...}` 两行 |
| wiremock 端到端 | 1 test: 6 events (message_start + content_block_start + 2 delta + stop + message_stop) 拿 2 token |

### Anthropic SSE 协议 (跟 OpenAI 不一样)

```
POST /v1/messages
x-api-key: sk-ant-...
anthropic-version: 2023-06-01
{"model": "claude-3-5-sonnet-20241022", "stream": true, ...}

→ 200 OK
Content-Type: text/event-stream

event: message_start
data: {"type":"message_start","message":{"id":"msg_01","role":"assistant"}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}

event: message_stop
data: {"type":"message_stop"}
```

业务方流解析:
- 每个 event 含 `event: <type>` + `data: <json>` 两行 + 空行
- 只 `content_block_delta` 走 yield, 拿 `data.delta.text`
- `message_stop` 终止
- 其他 event (`message_start` / `content_block_start` / `content_block_stop` / `message_delta`) 静默 skip

### 关键设计决策

- **跟 OpenAI parser 完全分离**: 协议结构不同 (event-based vs data-only), 共享 SSE buffer/byte 解析逻辑, 但 event routing 各自 impl
- **`message_stop` 走 early return** (在 yield 前检查): 业务方 stream 干净收尾, 不多 yield 空 token
- **Anthropic error response 仍是 JSON 不走 SSE**: HTTP 4xx/5xx 跟 OpenAI 同样 status check, 走 eprintln 早返
- **parser 拿 (event_type, data) tuple**: 业务方 stream! 内部分流, 边界清晰, 单元测试简单 (跟 OpenAI 7 test 类似)
- **不动 proto / 业务方协议**: 业务方拿 `Stream<Item = String>` 跟 P6-2 OpenAI 完全一致, Phase 7 业务方无感升级

### 踩坑 (P6-3 阶段 1 个)

1. **`AnthropicAdapter` 缺 `with_endpoint`**: P6-2 测试时发现 OpenaiAdapter 有 setter, AnthropicAdapter 之前只 with_model, wiremock 测试 endpoint 写死. 修法: 跟 OpenaiAdapter 一致, 加 `with_endpoint` setter

### 测试

- **ma-harness-model**: 28/28 pass (23 老 + 5 新 P6-3)
  - `anthropic_build_stream_request_body_includes_stream_true` (1 test)
  - `anthropic_parse_sse_event_*` (3 test): content_block_delta / non-content-block / malformed
  - `anthropic_complete_stream_end_to_end_with_wiremock` (1 test): 6 events 拿 2 token "Hello world"
- **workspace**: 307 total (295 lib + 12 bin, +5 新), 排除 4 pre-existing broken

### 给后来人

- 业务方跑真 Anthropic: `ANTHROPIC_API_KEY=sk-ant-... mah start` + `mah run-stream --model "anthropic:claude-3-5-sonnet" "explain rust"`, 看 typewriter 输出
- OpenAI / Anthropic / Stub 三家 streaming 都走通: 业务方按 model 字符串选, CLI 透明
- Phase 6 streaming PoC 完成: stub (P5-6) / OpenAI (P6-2) / Anthropic (P6-3) / HTTP SSE (P5-8) / gRPC RunStream (P5-6) / CLI (P6-1) 全链路
- 业务方想 Azure Anthropic: `AnthropicAdapter::new(key).with_endpoint("https://...azure.com/v1/messages")`
- 业务方想 custom adapter (Together / Groq / Cohere): 复用 SSE buffer pattern, 自己写 event routing
- OpenAI/Anthropic parser 都没处理 keepalive (`:` comment line): 业务方 SSE buffer `\n\n` 切到空 event 静默 skip, 行为正确
- Phase 7+ 业务方反馈 streaming latency / token rate 时, 加 perf test

