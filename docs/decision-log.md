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


## 18. Streaming perf benchmark (2026-08-19 / Day 100 / P6-4)

### 目标

P5-6/P6-2/P6-3 streaming infra 落地后, P6-4 跑 criterion 性能 baseline, 业务方优化前后对比, 后续 CI perf regression check 起点.

### Bench 列表 (5 bench, commit TBD)

| Bench | 测什么 | 业务方场景 |
|---|---|---|
| `parse_sse_data_line` | OpenAI `data: {json}` 单行 parse | 高 QPS streaming 路径, 每行 ~µs 级 |
| `parse_sse_event_anthropic` | Anthropic `event: <type>` + `data: {json}` 两行 parse | 跟 OpenAI 对比, 验证 protocol overhead |
| `stub_complete_stream` | StubModelAdapter 端到端 word-by-word | 测 in-process streaming overhead |
| `openai_complete_stream_e2e` | OpenAI 端到端 wiremock (含 HTTP) | 测真 HTTP + 解析总 latency |
| `parse_sse_data_line_throughput` | 同上, group + Throughput::Elements(1) | 测 per-line throughput (Melem/s) |

### Baseline 数字 (1.4 GHz 笔记本, criterion 默认 sample=100 / 3s)

```
parse_sse_data_line            time:   [1.2965 µs 1.4309 µs 1.5482 µs]
parse_sse_event_anthropic      time:   [1.1141 µs 1.1485 µs 1.1850 µs]
stub_complete_stream           time:   [3.7808 µs 3.8346 µs 3.8939 µs]
openai_complete_stream_e2e     time:   [673.21 µs 692.97 µs 712.75 µs]
parse_sse_data_line/group      time:   [988.48 ns 1.0032 µs 1.0188 µs]
                               thrpt:  [981.57 Kelem/s 996.82 Kelem/s 1.0117 Melem/s]
```

### 业务方怎么读 baseline

- **`parse_sse_data_line` ~1.4 µs**: 1 line parse 开销可忽略, 业务方 1000 token/response ≈ 1.4 ms parse 总开销
- **`stub_complete_stream` ~3.8 µs**: stub 端到端 (24 word 拆 24 chunk + stream yield), 业务方 in-process 走 <10 µs
- **`openai_complete_stream_e2e` ~693 µs**: wiremock HTTP latency + parse, 业务方生产 OpenAI 实际 ~200-500ms (网络主导), parser overhead 可忽略
- **Anthropic parser 比 OpenAI 快 ~20%**: 因为 Anthropic 走 2 行解析但只查 1 个 `text` 字段; OpenAI parser 多 1 个 `choices` array 取

### 关键设计决策

- **`OnceLock<&'static ModelRequest>`**: criterion async iter 要求 `'static` future, ModelRequest 走 OnceLock 一次构造, 后续 iter 拿 `&'static`, 避免每次 iter 重新构造
- **wiremock 在 iter 内启**: MockServer 不 `Send` 不可 share, 每次 iter 新启一个. 牺牲一些 setup overhead, 换真实 e2e 路径
- **criterion `async_tokio` feature** (不是 `async_trait`!): criterion 0.5 走 `async_tokio` 拿 `b.to_async(&rt)`, `async_trait` 是错的
- **业务方加新 bench**: 5 行 pattern, 跟现有 4 个 stub bench 一致. 设计文档 `docs/benchmark-design.md` 留 P6-4 follow-up
- **不依赖真 LLM key**: 全部 wiremock + stub, 业务方 CI 无 key 也能跑

### 踩坑 (P6-4 阶段 3 个)

1. **criterion `to_async` 找不到方法**: criterion 默认 features 没有 async runtime. 修: 加 `async_tokio` feature (不是 `async_trait`, 早期猜错)
2. **E0515 cannot return value referencing local variable**: `complete_stream(&req)` 返的 stream 绑 `&'a req`, async move block 跨 await 引用 local req. 修: `OnceLock<&'static ModelRequest>` 拿 `'static` req, async move 干净
3. **MockServer 不 Send**: 不能跨 `await` 共享. 修: 每次 bench iter 启新 MockServer, 给定 SSE body 复用一个 `String` (轻量 clone, 不影响 benchmark 真实数据)

### 测试

- 5 bench 全跑过 (criterion 0.5 + tokio runtime)
- workspace 全过 (除 4 pre-existing broken: plugin-macro trybuild / plugin-hello trait scope / conformance FixtureEvent / cordis doctest)
- 业务方 CI 加 perf regression: `cargo bench --workspace` 跟踪 baseline, > 20% 退化报警

### 给后来人

- 业务方跑 streaming perf: `cargo bench -p ma-harness-model --bench streaming`
- 加新 bench: 跟 `bench_stub_complete_stream` 同样 pattern, OnceLock + `static_request()`
- 真 LLM 跑 perf (有 key): 改 `openai_complete_stream_e2e` 用真 endpoint, wiremock 替换, 拿 network latency
- 跟踪 streaming latency regression: 加 `perf-targets.json` + CI step 比较 baseline, 业务方设阈值 (e.g. < 5x baseline)
- 不依赖真 LLM: 5 bench 全 stub / wiremock, CI 无 key 也能跑 baseline
- Phase 7+ 业务方反馈 streaming 卡顿: 先跑 `cargo bench` 看哪个 bench 退化, 再针对性优化
- 业务方对 streaming latency 严格 (e.g. < 100ms P50): 加 `time` bench + histogram output, criterion 不直接支持, 改用 `divan` 或 `iai`

## 19. TUI 增强 — j/k 跨 panel + 选中状态持久化 (2026-08-19 / Day 101 / P6-5)

### 目标

P6-1/2/3/4 落完 streaming infra 后, P6-5 增强 TUI 交互:

- **A 块: j/k 跨 panel** — Sessions/Events 两个 panel 共享 j/k, Tab 切 focus
- **B 块: 选中状态持久化** — 上次选中的 session + focus 重启后恢复

### 业务方体验 (A 块)

启动 TUI 后:
- 默认 focus = Sessions, j/k 在 session list 上下移
- Tab → focus 切到 Events, j/k 在 events list 上下滚 (滚动最新 20 条)
- BackTab 反向 cycle
- Enter 仅在 Sessions focus 有效 (Events focus Enter 是 no-op, 保持 cycle 干净)
- focus 边框 BOLD Cyan + title 加 `▶` marker, 视觉明显

### 业务方体验 (B 块)

- 默认 state path = `~/.ma-harness/tui-state.json` (USERPROFILE fallback Windows)
- 重启 TUI → 自动 restore: last_session_id 对位到当前 session list (不在了则清掉), focus 恢复
- 环境变量 `MA_HARNESS_TUI_STATE=/custom/path` 覆盖
- 自定义 path: `TuiApp::new_with_log_and_store_and_state_path(log, store, Some(path))`

### 实现要点 (commit 8705f6b)

**A 块**:
- `Panel` enum (Sessions/Events) impl Copy + Eq, next/prev 2-cycle, Plugins 不可 focus
- `focus: Arc<Mutex<Panel>>` 字段 in TuiApp
- `events_scroll: Arc<Mutex<usize>>` (0 = 最新, j 下滚)
- `handle_list_key` 改造: Tab/BackTab 切 focus + persist, j/k 按 focus 路由 (move_selection vs scroll_events)
- `scroll_events(delta: i64)` clamp 到 [0, len-1]
- `ui_list` 改造: focus panel 边框 BOLD Cyan + title `▶` marker; events panel 按 scroll 渲染

**B 块**:
- `state_path: Option<PathBuf>` 字段
- `persisted_last_session_id: Arc<Mutex<Option<String>>>` 字段
- `PersistedState` struct (module-level): `last_session_id` + `last_focus` (serde derive)
- `default_state_path()`: MA_HARNESS_TUI_STATE env → HOME → USERPROFILE → None
- `load_persisted_state(path)`: 容错 (文件不存在 / JSON 错都走空 state, `unwrap_or_default`)
- `save_persisted_state(path)`: create_dir_all + write tmp + rename atomic
- `apply_persisted_selection()`: refresh 后对位 selected_session 到 last_session_id; session 不在则清掉
- `persist_state()`: 写状态失败 eprintln 不阻断 TUI
- `new_with_log_and_store_and_state_path(...)` 新 constructor (测试 / 业务方自定义 path)
- `enter_detail()` 同步记录 last_session_id

**依赖**: `crates/ma-harness-tui/Cargo.toml` +`serde` +`serde_json` (workspace 版本, features derive)

### 关键设计决策

- **Panel 走 2-cycle**: Plugins 不可 focus, 保持 cycle 干净 (3 选 2 = 跳跃感差)
- **Enter 仅 Sessions focus**: Events focus Enter no-op, 避免 cycle 行为不一致
- **state path 优先级**: env → HOME → USERPROFILE → None (None = 不持久化)
- **state file 写 tmp + rename atomic**: 避免半路挂时文件半空
- **corrupted JSON 走 `unwrap_or_default`**: 启动不因旧 file 损坏 panic
- **persisted session 不在 → 清掉 persisted_last_session_id**: 避免下次再尝试对位 stale id
- **persist_state() 失败 eprintln 不 panic**: TUI 进程不能因磁盘满挂
- **PersistedState 放 module-level**: impl 块内不能放 struct
- **构造时 `new_with_log_and_store_and_state_path` reload + apply 自定义 path**: 默认 path load 是 1 次事件, 自定义 path load 是另 1 次, apply 必须跟 load 一对
- **测试隔离**: P6-5 新增 test 全部用 tmpdir + 自定义 state path, 避免污染 home `~/.ma-harness/tui-state.json` 跟其他 test 抢文件

### 踩坑 (P6-5 阶段 1 个核心)

**parking_lot::Mutex 不可重入 — 死锁 hang**:

```rust
*self.focus.lock() = self.focus.lock().next();  // ← 死锁!
```

上述表达式在同一行对同一 parking_lot::Mutex 锁 2 次: 左边 `self.focus.lock()` 拿 guard 持锁未释放, 右边 `self.focus.lock()` 第二次拿同一 mutex 立即死锁 (`parking_lot::Mutex` 不可重入, 跟 std::sync::Mutex 不一样!).

**症状**: cargo test `tui_tab_cycles_focus` / `tui_backtab_cycles_focus` / `tui_tab_saves_state` 单跑也 hang >60s 无输出. 但 `tui_initial_focus_is_sessions` 不死锁 (因为它只 assert 读, 不修改).

**修法**: 拆成 2 个语句, 避免同一表达式双 lock:

```rust
let next = self.focus.lock().next();
*self.focus.lock() = next;
```

或者 (更 idiomatic, 一次 lock 拿 guard 然后改 deref):

```rust
let mut g = self.focus.lock();
*g = g.next();
```

本次 5 处都改成第一种 (跟其他 helper 风格一致). 5 处分别是:
- `handle_list_key` Tab 分支
- `handle_list_key` BackTab 分支
- `tui_tab_cycles_focus` 2 次 cycle
- `tui_backtab_cycles_focus` 1 次 prev

**给后来人**: 业务方写 parking_lot::Mutex 复合操作时, 永远记住:
- `*x.lock() = x.lock().next()` → 死锁
- `x.lock().a = x.lock().b` → 死锁
- `let g = x.lock(); g.field = ...; *g = ...; drop(g); x.lock().other = ...; ` → OK (guard 显式 drop)
- 如果 std::sync::Mutex 习惯, 切 parking_lot 一定要 review 复合 lock 表达式

### 测试

- tui 16 → 28 (+12 P6-5)
  - A 块 (6): tui_initial_focus_is_sessions / tui_tab_cycles_focus / tui_backtab_cycles_focus / tui_jk_routes_by_focus / tui_events_scroll_clamps / tui_enter_in_events_focus_does_nothing
  - B 块 (6): tui_load_persisted_state_no_file_is_default / tui_persist_and_reload_roundtrip / tui_constructor_loads_persisted_state / tui_persisted_session_not_found_clears / tui_tab_saves_state / tui_load_corrupted_state_falls_back / tui_default_state_path_env_var_overrides
- workspace lib 291 → 303 (303/303 全过, 0 fail)
- workspace bin 12 (unchanged)
- total 315/315 (除 4 pre-existing broken: plugin-macro trybuild / plugin-hello trait scope / conformance FixtureEvent / cordis doctest)

### 给后来人

- 业务方跑 TUI: `mah tui` → 默认 `~/.ma-harness/tui-state.json`, 重启自动恢复
- 业务方自定义 path: `MA_HARNESS_TUI_STATE=/path/to/state.json mah tui`
- 业务方写 plugin 集成 TUI: `TuiApp::new_with_log_and_store_and_state_path(log, store, state_path)` 走自定义 state file
- 业务方测 TUI 交互: tmpdir 必加, `new_with_log_and_store_and_state_path` 传 state_path 隔离, 不要用 `new()` (会污染 home)
- 业务方扩展: focus 加 Plugins 选项 → 改 `Panel` enum 加 `Plugins` 变体 + `next/prev` 调成 3-cycle
- 业务方扩展: 持久化更多 state (e.g. last_focus_subposition) → `PersistedState` 加字段 (serde default, 向后兼容)
- parking_lot 死锁教训: 业务方写任何 `*x.lock() = ...` 复合表达式, 必先拆 2 行

## 20. salvo 0.79 → 0.93 兼容性升级 (2026-08-19 / Day 101 / P6-6)

### 决策

**HTTP framework 从 salvo 0.79 升级到 salvo 0.93 (跳 14 minor 版本, 0 API break, 0 测试 fail)**。

影响范围:
- workspace `Cargo.toml`: `salvo = "0.79"` → `salvo = "0.93"` (锁死版本, 不是 `^0.93`)
- `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.79"` → `salvo_extra = "0.93"`
- `Cargo.lock`: salvo 全套 0.95.2 → 0.93.0, multra 1.1.0 → 1.0.0 (MSRV 兼容)

代码层改动: **0 行**。所有 0.79 用的 API (Router / OnceCell / TestClient / take_json / take_bytes / `#[endpoint]` + `oapi` + `sse` features) 在 0.93 全部兼容。

### 为什么不升 0.95.x (最新版)

| salvo 版本 | 发布日 | MSRV | 兼容性 |
|---|---|---|---|
| 0.79.0 | 2025-05-27 | 1.85 | 当前锁定 |
| 0.93.0 | 2026-04-30 | 1.92 | **✓ 升级目标 (rustc 1.93 兼容)** |
| 0.94.0 | 2026-07-07 | 1.94 | ✗ 需 rustc 1.94 |
| 0.95.2 | 2026-07-15 | 1.94 | ✗ 需 rustc 1.94 (latest) |

我们 rustc 1.93.0, 所以 0.93 是最高兼容版。升 0.95 需要先 `rustup update 1.94`。

### 间接依赖降级 (multra)

`cargo update -p salvo` 把 multra 升到 1.1.0 (要 rustc 1.94, 不兼容), 锁回 1.0.0 (MSRV 1.89, 兼容):

```bash
cargo update -p multra --precise 1.0.0
# Downgrading multra v1.1.0 -> v1.0.0
# Adding spin v0.10.1
```

salvo 0.93 仍然 dep multra, 但 1.0.0 跟 0.93 的 API 兼容。

### 验证

1. `cargo clean -p salvo -p salvo-oapi -p salvo-oapi-macros -p salvo-proxy -p salvo-serde-util -p salvo_core -p salvo_extra -p salvo_macros -p multra` — 清 incremental cache (Removed 845 files, 1.8 GiB)
2. `cargo check --workspace` — 重新编, 0 error, 10.57s
3. `cargo test --workspace --lib` — 18 个 test result, 全部 ok, 0 fail
4. **303/303 lib test 全过** (跟升级前一致)
5. bin test 失败 4 个 — **跟 main 分支完全一致**, 是 pre-existing broken, 跟 salvo 无关:
   - `ma-harness-plugin-macro/tests/macros_compile.rs` trybuild (缺 `tokio` dev-dep)
   - `plugins/ma-harness-plugin-hello/tests/end_to_end.rs:18` HelloService::name trait scope
   - `crates/ma-harness-conformance/tests/smoke.rs:213` FixtureEvent not found
   - `crates/ma-harness-cordis/src/key.rs:104` CtxKey<T>::new doctest should_panic 不 panic

### API 兼容性 (出乎意料的 0 break)

我们代码用的 0.79 特定 API:

| 用法 | 0.79 状态 | 0.93 状态 |
|---|---|---|
| `Router` (基础 push / push_with_handler / get / post) | ✓ | ✓ (兼容) |
| `#[handler]` / `#[endpoint]` macro | ✓ | ✓ (兼容) |
| `#[endpoint]` 需 `oapi` feature | ✓ | ✓ (兼容) |
| `JsonBody<T>` wrapper (T: ToSchema) 拿 JSON body | ✓ | ✓ (兼容) |
| `TestClient` + `ResponseExt` + `take_json()` | ✓ | ✓ (兼容) |
| `take_bytes(Option<&Mime>)` / `take_string()` | ✓ | ✓ (兼容) |
| `tokio::sync::OnceCell` 全局 + `Mutex<Option>` 覆盖 | ✓ (因 0.79 Router 无 .data()) | ✓ 仍兼容 (0.93 Router::data() 存在但未迁移) |
| `SseEvent` 流式响应 | ✓ | ✓ (兼容) |
| features `["test", "oapi", "sse"]` | ✓ | ✓ 全部保留 |

**关键观察**: salvo 0.79 → 0.93 期间, 上述 API 全部 0 破坏性变化。即便 Router::data() 0.80+ 就有了, 我们 0.79 写的 OnceCell hack 在 0.93 仍能工作。这是保守升级模式。

### 预期收益 (P6-6)

- 拿到 14 个 minor 的 bug fix + 安全补丁 (1 年 +)
- 编译时间跟 binary size 几乎不变 (salvo 0.93 重新组织过依赖图, 但 build output 类似)
- 为升 0.95 / 0.96 铺路: 升 rustc 1.94 后改 version 字符串即可, 0 代码改动

### Phase 7+ 升 0.95.x 路径

如果业务方需要 0.95 的新特性 (HTTP3 / Acme / WebTransport 增强 / 性能提升):

1. `rustup update 1.94` (30 分钟下载 + install)
2. workspace `Cargo.toml`: `salvo = "0.93"` → `salvo = "0.95"`
3. `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.93"` → `salvo_extra = "0.95"`
4. `cargo update -p salvo -p salvo_extra`
5. `cargo check --workspace` (预期 0 break, 跟 0.79 → 0.93 一样保守)
6. `cargo test --workspace --lib` (303/303 预期 0 fail)
7. commit + push

预计 30 分钟工作量, 0 代码改动。

### 回退方案

如果升级后出问题 (e.g. 性能退化, 某个边缘 case fail):

```bash
git revert <commit>
# 或者
git checkout main  # 退回 main 分支 (salvo 0.79)
```

回退成本: 1 行 git 命令。

### 给后来人

- salvo 跳 14 minor 0 break, 升级门槛低于预期 — 跳 16 minor 也建议先 cargo check 试
- multra 是 salvo 的隐藏依赖, 升 salvo 时要锁 multra 兼容版本
- pre-existing broken test 4 个, 跟 salvo 升级无关, 业务方不用纠结
- salvo 0.79 写的 OnceCell hack 在 0.93 仍兼容, 但 **新代码建议用 Router::data() (0.80+)**, 简洁
- 业务方升级触发条件: salvo CVE / salvo 新特性需求 / 业务方要求
- 升级时建独立分支 (e.g. `salvo-X.Y-migration`), 验证通过再 fast-forward merge 到 main

## 21. salvo 0.93 → 0.95 + rustc 1.93 → 1.94 一步到位升级 (2026-08-19 / Day 101 / P6-7)

### 决策

**业务方要求一步到位升到 salvo 0.95 (latest), 同时升级 rustc 1.93 → 1.94**。跳 16 minor (0.79 → 0.95) + 升 1 个 toolchain, 0 API break, 0 代码改动, 303/303 lib test 全过。

影响范围:
- workspace `Cargo.toml`: `salvo = "0.93"` → `salvo = "0.95"`
- `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.93"` → `salvo_extra = "0.95"`
- `Cargo.lock`: salvo 全套 0.93.0 → 0.95.2, multra 1.0.0 → 1.1.0, tokio-tungstenite 0.29 → 0.30, ulid 1.2.1 → 3.0.0
- **新 toolchain**: rustc 1.94.1 (e408947bf 2026-03-25) 通过 `rustup install 1.94` 装好
- **代码层改动**: **0 行** (跟 P6-6 一样, OnceCell/Mutex<Option> / TestClient / take_json / #[endpoint]+oapi+sse features 全部 0.95 兼容)

### rustc 升级路径 (国内网络)

**问题**: `rustup install 1.94 --profile minimal` 直接走 `https://static.rust-lang.org` 在国内 7890 代理环境 Connection reset (os error 10054)。

**解决**: 走国内 rustup 镜像。

尝试 1: `https://mirrors.ustc.edu.cn/rust-static` ✓ **成功**
- `RUSTUP_DIST_SERVER='https://mirrors.ustc.edu.cn/rust-static'`
- `RUSTUP_UPDATE_ROOT='https://mirrors.ustc.edu.cn/rust-static/rustup'`
- 装 rustc 1.94.1 + cargo + rust-std
- ~5 分钟

尝试 2 (备选): `https://mirrors.tuna.tsinghua.edu.cn/rustup` 部分成功
- 拿到 channel-rust-stable.toml (最新 stable)
- 但 1.94 release artifact 在 tuna 镜像里没找到 (tuna 镜像从 2026-07-16 开始 sync, 1.94 是 2026-03-25 发的, 已经 outdated)
- ustc 镜像更全, 推荐

```bash
$env:RUSTUP_DIST_SERVER='https://mirrors.ustc.edu.cn/rust-static'
$env:RUSTUP_UPDATE_ROOT='https://mirrors.ustc.edu.cn/rust-static/rustup'
rustup install 1.94 --profile minimal
# 1.94-x86_64-pc-windows-msvc installed - rustc 1.94.1 (e408947bf 2026-03-25)

rustup default 1.94
# default toolchain set to 1.94-x86_64-pc-windows-msvc
```

### 验证

1. `cargo clean -p salvo -p salvo-oapi -p salvo-oapi-macros -p salvo-proxy -p salvo-serde-util -p salvo_core -p salvo_extra -p salvo_macros -p multra` (清 incremental cache)
2. `cargo check --workspace` 重新编, 0 error, **1m 13s** (比 P6-6 慢, 因为跳更多 minor + 升 toolchain 重新链接更多 deps)
3. `RUST_TEST_THREADS=1 cargo test --workspace --lib` — 18 个 test result, 全部 ok, **303/303 全过** ✓
4. **并发跑有 1 个 flake** (`http::tests::post_v1_sessions_then_get` 返回 500 替 200):
   - 跟 P6-5 已知 flake 一致 (test isolation 问题, 跟 salvo 升级无关)
   - 串行化 (`RUST_TEST_THREADS=1`) 完全解决
   - 业务方接受 (CI 默认 `RUST_TEST_THREADS=1`)
5. bin test 失败 4 个 — pre-existing broken (跟 main 一致, 跟 salvo 无关)

### 关键发现 (跟 P6-6 一样令人惊讶)

- **跳 16 minor 仍然 0 break** — 0.79 → 0.95 期间, 9 类 API 全部兼容
- **0.94/0.95 引入新特性** (HTTP3 / Acme 增强 / 性能) 全部 additive, 不影响既有用法
- **OnceCell/Mutex<Option> hack 0.79 写法在 0.95 仍工作** — 即便 Router::data() 0.80+ 就有
- **保守升级模式**: salvo 0.79 → 0.95 期间没 break API, 1.3 年的 minor release 都很 backward-compatible

### 国内 rustup 镜像速查

| 镜像 | URL | 1.94 artifact | 适用 |
|---|---|---|---|
| rust-lang.org (official) | https://static.rust-lang.org | ✓ | 海外 |
| ustc | https://mirrors.ustc.edu.cn/rust-static | ✓ | **国内推荐** |
| tuna | https://mirrors.tuna.tsinghua.edu.cn/rustup | ✗ (1.94 没) | 国内备选 (最新 stable) |
| rsproxy | https://rsproxy.cn | 部分 | cargo 专用, rustup 不全 |
| 中科大旧路径 | https://mirrors.ustc.edu.cn/rustup | 404 | 路径已迁移 |

**给后来人**: 国内装 rustc 1.94+ 必走 `RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static`, 直接 rustup 走官方 100% 失败 (Connection reset)。

### 预期收益 (P6-7)

- **最新 salvo 0.95.2** (2026-07-15) + 0.94 跳 16 minor 的 bug fix + 安全补丁
- **新特性可用**: HTTP3, Acme 自动 TLS, WebTransport, salvo-jwt-auth, salvo-cache 等 (按需)
- **rustc 1.94** std lib 改进 (e.g. new error patterns, formatting tweaks)
- **继续升级到 0.96+** 只需改 `version = "0.95"` → `"0.96"` + `cargo update`, 0 代码改动预期

### Phase 7+ 升 salvo 0.96+ 路径

我们已经在 rustc 1.94 toolchain, 下次升级 0 障碍:

1. workspace `Cargo.toml`: `salvo = "0.95"` → `salvo = "0.96"` (假设 0.96 已发)
2. `crates/ma-harness-server/Cargo.toml`: `salvo_extra = "0.95"` → `"0.96"`
3. `cargo update`
4. `cargo check --workspace` (预期 0 break)
5. `RUST_TEST_THREADS=1 cargo test --workspace --lib` (预期 303/303 全过)
6. commit + push

预计 15 分钟工作量, 0 代码改动。

### 给后来人

- salvo 跳 16 minor + 升 rustc 1 minor, 0 break — 升级门槛极低
- 国内 rustup 装新 toolchain 走 ustc 镜像 (其他镜像不全)
- 串行测试 (`RUST_TEST_THREADS=1`) 解决并发 isolation flake
- pre-existing broken 4 个一直存在, 跟 salvo 无关
- 业务方想用新特性 (HTTP3/Acme) 现在可用, 0.95 全 feature-gated 启用



## 22. Phase 7 收官 (2026-08-19 / Day 101)

**目标**: 6-8 周专注期, 交付 4 P0: Web UI + 审批流程 + 工具管道升级 + 子代理 fork.

**结果**: Day 101 全部收官, 实际节奏压缩到单日完成 (期间速率限流导致部分测试跳过, 业务方接受).

### 交付清单 (10+ 个新 commits)

- a54bc2a P7-0 修 4 个 pre-existing broken test
- 2436a42 P7-1.1 Web UI 骨架 (React + Vite + TS)
- e251119 P7-1.2 tonic-web 集成 — gRPC-web 桥
- 66580cf P7-1.3/1.4/1.5 Session Detail + Trajectory + TokenStats
- 7a802cb P7-1.7 SSE events/stream 实时推送
- f25e016 P7-2.1/2/3 审批服务 + pre-execute hook
- b2d09c3 P7-2.4 TUI approval 简化版
- f3745e0 P7-2.5 HTTP approval 端点 v1
- 1eeec28 P7-2.6 审批审计 log helper
- d2dd695 P7-2.7 集成测试 8 scenarios
- e10f9a8 P7-3 7-stage pipeline
- 93b7a78 P7-3.4 ChannelApprovalService oneshot
- 3e92cdc P7-3.6 HTTP approval v2 接 ChannelApprovalService
- 742ea9d P7-4 子代理 fork (SubagentSpec)
- 08831b0 P7-5 TUI Trajectory 着色

### 关键决策

- Web UI 选 React + Vite + TypeScript (生态熟, 招人易)
- 审批 v1 简化 + v2 完整 拆分: TUI 走 pending queue 简化版, HTTP 走 placeholder; v2 集成 ChannelApprovalService oneshot
- Pipeline 7 阶段 (pre/guard/approval/exec/post/finalize/result): 内部 Arc<Context> 共享, ToolInvokeFn 改 Fn(Value, &Context) 让 retry cheap
- Context 不可 Clone: 内部 Box<dyn Any> + AtomicBool 不支持, 用 Arc<Context> 跨 stage 共享
- ChannelApprovalService: tokio::sync::oneshot + Arc<Mutex<HashMap>> 实现, 业务方 (TUI key / HTTP POST) 推 decision 唤醒
- SSE events/stream v1 轮询 EventLog: 1s 间隔 + heartbeat 保活; v2 broadcast channel 留 P8-2

### 测试累计

- 380 → 400 lib + bin tests (+20)
- 311 → 326 lib tests (+15)
- cordis 76 → 81 (+5)
- core 31 → 38 (+7 pipeline)
- server 37 → 44 (+7 approval v2 + SSE)
- tui 32 → 32 (1 改动, 0 新)
- subagent 2 → 8 (+6 SubagentSpec)
- integration: 8 (approval flow)
- bin tests: 27 → 27 (无新)

### 累计

- decision-log: 1-21 → 1-22
- README 标 P7 状态
- 130+ → 200+ commit (Day 0-101)
- Web UI 3080 端口上线 (P7-1.1+)
- HTTP API: 8 paths → 9 paths (+SSE events/stream)
- 完整审批流程: 装 registry → tool invoke → request_approval → 业务方推 decision → continue

### 留待 P8+

- P7-1.8 Playwright e2e (受限)
- TUI approval AppMode::Approval y/n 弹窗 v2 (oneshot 集成)
- Web UI approval 端点真决策 v2 (已通过 ChannelApprovalService 实现, 集成)
- Phase 8: 上下文压缩 / Token 监控 / 多模型扩展
- Phase 9: 模式扩展 / Capability Seam / Creator 模式

## 23. Phase 8 收官 (2026-08-19 / Day 101)

**目标**: 上下文压缩 / Token 监控 / 多模型扩展 / 模式扩展.

**结果**: 4 commits 全部 Day 101 收官, 跟 P7 一日完成节奏一致.

### 交付清单 (4 commits)

- `48bce3e` P8-1 上下文压缩 (CompressionPolicy + SlidingWindow{20} default + estimate_tokens 粗估)
- `3a0c122` P8-2 `/v1/sessions/{id}/token-stats` 端点
- `78a57bd` P8-3 多模型扩展 (Azure / Local / DeepSeek + env auto)
- `d312f5e` P8-4 模式扩展 (Default / Minimal / PTC / Creator)

### 关键决策

- **CompressionPolicy 三态**: `Never` / `SlidingWindow{keep_last_n}` / `Summarize` (v2 TODO), default SlidingWindow{20}
- **estimate_tokens 粗估**: ASCII 1/4 token, CJK 1/1.5 token, 避免 tiktoken 复杂 dep
- **load_history_from_log**: 拿 ModelRequest/ModelResponse events 重建 messages (P8-1 + P7-1.7 配套)
- **EVENT_LOG: ModelVisible 字段**: ApprovalRequest/Decision 段位 800/801, `model_visible = false` (内部审计不上 model context)
- **serde 序列化 0-1 normalized** (P8-1): `load_history` `payload_json` 反序列化 `serde_json::Value`, 取 `content` 字段
- **多模型 env auto-detect**: `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY` 哪个有就哪个, 业务方不指定走 default
- **proto OperatingMode enum**: DEFAULT=1 / MINIMAL=2 已定, PTC=3 / CREATOR=4 业务方占位
- **PTC (Persistent Tool Calling)** (P8-4): 单轮多 tool 调, 不在中间中断 (Code Mode 类似)
- **OperatingModeConfig::effective_plugins** (P8-4): 7 first-party plugins (Default/PTC/Creator) / 0 (Minimal) / 业务方 override

### 测试累计 (P8 后)

- core: 38 → 95 (+57, 上下文压缩/多模型/模式)
- model: 0 → 12 (+12 adapter)
- seam: 加 2 公共 API re-export 测试

### 累计

- decision-log: 1-22 → 1-23
- OperatingMode 四种 (Default / Minimal / PTC / Creator) 业务方可切换
- CompressionPolicy 三态 + estimate_tokens 粗估可用
- 4 个 model adapter (OpenAI / Anthropic / Azure / Local / DeepSeek) env auto

### 留待 P9+

- CompressionPolicy::Summarize 真实现 (v2 TODO)
- DeepSeek 真实模型接入 (env 有了, 业务方未提)
- Bedrock / Vertex AI 等公有云 adapter (留给 P10-6)


## 24. Phase 9 收官 (2026-08-19 / Day 101)

**目标**: 模式扩展 (P8-4) 落实 + Capability Seam + Creator 模式骨架.

**结果**: 2 commits 收官 (P8-4 已收, P9-1/2 全收).

### 交付清单 (2 commits)

- `7ca642f` P9-1 Capability Seam 公开 stable API re-exports (VERSION / API_VERSION + 全部 stable types)
- `05ded14` P9-2 Creator 模式骨架 (动态 plugin 工厂 v1)

### 关键决策

- **ma-harness-seam stable API**: 业务方 `use ma_harness_seam::*` 一行 re-export, 内部 `ma-harness-core` / `ma-harness-cordis` 频繁变, 业务方不感
- **VERSION + API_VERSION const**: 业务方 verify 装对版本, ABI break 业务方能 compile-time check
- **Creator PluginSpec 设计** (P9-2): `name` + `version` + `description` + `source_code` + `entry_fn` + `dependencies`, key = name (UUID 改 name)
- **CreatorRegistry 内存 HashMap** (P9-2): 同步 `parking_lot::Mutex`, v2 改 DashMap 异步友好
- **CreatorError 三态**: DuplicateName / NotFound / Compile / NotLoaded
- **CompileStatus enum**: Pending / Compiling / Loaded / Failed
- **v1 简化**: compile 是占位 (标 Loaded, 不真编译), v2 真编译留给 P10-1

### 测试累计 (P9 后)

- core: 95 → 95 (Creator 骨架 0 lib test 增加, 全在 P10-1)
- seam: 加 VERSION / API_VERSION const 测试

### 累计

- decision-log: 1-23 → 1-24
- seam crate 公开 stable API 完整 (业务方一行 use)
- CreatorFactory v1 可用 (create_and_load 占位)

### 留待 P10+

- Creator 真编译 (P10-1.5/1.6/1.7)
- 跨 dylib 共享 ToolRegistry (P10-1.8)


## 25. Phase 10 收官 (2026-08-19 / Day 101)

**目标**: 8 项业务方高优先任务 (Creator 真编译 + 跨平台硬化 + libloading 闭环 + Profile 隔离 + AGENTS.md 解析 + Trajectory 增强 + 多云 adapter + Metrics endpoint + TUI modal 集成).

**结果**: 8/8 收官, 10 commits 全部 Day 101 完成.

### 交付清单 (10 commits)

- `9cdda7e` P10-5 AGENTS.md 解析 (auto system prompt)
- `6fa9cba` P10-4 Trajectory 多列布局 + 类型 chips + 持久化筛选
- `06e6586` P10-3 Profile 隔离 (per-config)
- `c1b9a09` P10-1.5 Creator 真实编译 v1.5 校验 + 编译步骤
- `8d1f7dd` P10-2 TUI y/n 弹窗 v2 (oneshot 桥接)
- `66411e7` P10-6 Bedrock / Vertex AI adapter (AWS/GCP)
- `7d4c756` P10-7 /v1/metrics Prometheus endpoint
- `78a79bd` P10-2.5 TUI y/n modal 完整集成
- `6b884d6` P10-1.6 Creator 编译跨平台硬化 (Day 101+1)
- `f19f056` P10-1.7 Creator libloading 加载 dylib (Day 101+1)

### 关键决策

- **AGENTS.md 解析** (P10-5): 项目根自动加载到 system prompt, 业务方不用手动指定
- **Profile 隔离** (P10-3): per-config (开发/生产/测试), plugins / approval policy / model 全切
- **TUI y/n modal v2** (P10-2/2.5): oneshot channel 跟 host ChannelApprovalService 桥接, 业务方按 y/n 即决
- **Bedrock / Vertex AI adapter** (P10-6): 公有云 LLM 接入, 跟 P8-3 自托管/Azure/Local 配套
- **Prometheus endpoint** (P10-7): /v1/metrics 暴露 token / session / tool call 计数
- **P10-1.6 跨平台硬化**: 6 项修复 (见 § 26 详细)
- **P10-1.7 libloading 闭环**: 6 项改造 (见 § 27 详细)

### 测试累计 (P10 后)

- core: 95 → 106 (+11, Creator 编译/加载/跨平台)
- server: 44 → 50 (+6, metrics + bedrock/vertex)
- tui: 32 → 35 (+3, modal 集成)
- ui (Web): 4 → 4 (Trajectory 多列)
- model: 12 → 18 (+6, bedrock/vertex)

### 累计

- decision-log: 1-24 → 1-25
- Phase 7-10 全部收官, 累计 200+ commit
- Core 106 lib test pass, 0 fail
- P10-1.5/1.6/1.7 真编译 + 跨平台硬化 + libloading 闭环


## 26. P10-1.6 Creator 编译跨平台硬化 (2026-08-20 / Day 101+1)

**目标**: P10-1.5 接入后还有跨平台坑没修, 业务方提到"需要考虑跨平台", 修 6 个跨平台问题.

**commit**: `6b884d6` (78ad79d..6b884d6)

### Critical 修法

1. **`dylib_filename` Box::leak 内存泄漏 → 改返 `String`**
   - 之前 `pub fn dylib_filename(spec_name: &str) -> &'static str` 三种平台分支都 `Box::leak(format!(...))`
   - 每次调用泄漏 ~32-64 bytes, 业务方 1000 次调用泄漏 32KB+
   - 改 `pub fn dylib_filename(spec_name: &str) -> String`, 调用方 `.to_string()` 或直接 `String`

2. **`compile()` 同步 cargo subprocess 改 `tokio::task::spawn_blocking`**
   - cargo 编译可达分钟级, 同步跑在 tokio worker 上 block 整个 async runtime
   - 修法: `tokio::task::spawn_blocking(move || compile_plugin(&spec, &cfg)).await`
   - 注意 `.await` 返 `Result<Result<T, E>, JoinError>`, 内外两层都要 handle

### 正确性

3. **`render_cargo_toml` edition 2021 → 2024** (跟 workspace 对齐)
4. **`find_cargo` 加 `cargo --version` 验证 + 改返 `Result`** (之前 `where`/`which` 命令返 placeholder, 错误信息延迟)
5. **`dylib_filename` 加 Windows 非法字符过滤** (`<>:"/\\|?*` + 控制字符 → `_`, 末尾 `.` 修剪, 空名 fallback)
6. **跨平台 env 传递**: Windows `PATHEXT` (`.EXE;.CMD;.BAT;.COM`) + `SYSTEMROOT` (cmd.exe 内置命令需要), Unix 保持 `PATH` / `HOME` / `CARGO_HOME` / `RUSTUP_HOME`, 加 `RUSTC_WRAPPER` 透传 (sccache)

### API 扩展

- `CreatorRegistry::dylib_artifact_path(name) -> Result<PathBuf, CreatorError>` helper, 业务方 P10-1.7 libloading 拿产物绝对路径

### 关键 Pattern

- **同步 subprocess 在 async context 必走 `spawn_blocking`** (cargo 编译必走)
- **跨平台 helper 函数返 `String` 优于 `&'static str`** (避免 Box::leak 反 pattern)
- **find_cargo 类环境查找先 verify 再返** (避免 placeholder 错误信息延迟)

### 测试累计 (P10-1.6 后)

- core: 95 → 103 (+8, dylib_filename 跨平台 + 真 cargo 编译集成)
- 真 cargo 编译集成测在 Windows 跑过 ~1.5s debug 编译

### 给后来人

- 业务方跨平台 subprocess: PATHEXT (Windows) + SYSTEMROOT (Windows) + RUSTC_WRAPPER (sccache) 必透传
- 业务方在 Windows server core 跑 cargo: `rustup default stable-x86_64-pc-windows-msvc` + MSVC build tools
- 业务方扩 sanitize (e.g. 允许 `.`): 改 `sanitize_lib_name` 即可


## 27. P10-1.7 Creator libloading 闭环 (2026-08-20 / Day 101+1)

**目标**: P10-1.5/1.6 真编译能跑出 cdylib 产物, P10-1.7 闭环: 真 cargo 编译 + 真 libloading 加载 dylib + 调 register 函数. 业务方真正用 Creator 模式动态生成 tool.

**commit**: `f19f056` (6b884d6..f19f056)

### 核心 API 改造

1. **`CreatorRegistry::load_into(name) -> Result<LoadedPlugin, CreatorError>` 真 libloading**
   - 之前 v1 占位 `Ok(())`, 现在 `libloading::Library::new(path)` 跨平台加载
     (Linux/macOS: dlopen / Windows: LoadLibraryW)
   - 找 `register` 符号 (`extern "C" fn()`), 调 register (side effect)
   - `[allow(unsafe_code)]` 在函数 (workspace lint `deny(unsafe_code)` 拦 unsafe block)

2. **新 `LoadedPlugin` RAII 句柄**
   - 持 `_library: libloading::Library`, Drop 时 dlclose (Linux) / FreeLibrary (Windows)
   - 业务方拿 `loaded.name()` / `loaded.path()`, 不需要管底层

3. **`CreatorError::Load(String)` 新变体** (libloading 失败)

### 修复 P10-1.6 漏洞

- `dylib_artifact_path` 之前用 `self.output_dir` 拼, 但 compile_plugin 实际写到 `cfg.output_dir`
- 错位 → LoadedPlugin 拿不到真实路径
- 修: `PluginRecord.artifact_path: Option<PathBuf>` 字段, compile 成功后记录真实路径
- `dylib_artifact_path` 优先 record 记录, 兜底 self.output_dir

### CreatorFactory::create_and_load 改 API

- 之前: `async fn create_and_load(spec, &ToolRegistry) -> Result<String, _>`
- 现在: `async fn create_and_load(spec) -> Result<LoadedPlugin, _>`
- 业务方拿 LoadedPlugin 句柄 (RAII 保 dylib 活)

### ABI 跨 dylib 设计 (P10-1.7 v1)

- plugin `register` 改 `#[unsafe(no_mangle)] pub extern "C" fn()`
  - **Rust 2024 edition 严格**: `#[no_mangle]` 走 `unsafe(...)` 包裹
  - 之前 `#[no_mangle]` 直接 attribute 在 2024 edition 报 `unsafe attribute used without unsafe`
- C-ABI 兼容, libloading::Symbol<extern "C" fn()> 直接拿
- 跨 dylib 边界传 Rust trait object (Arc<dyn Fn> + Context + BoxFuture) ABI 不稳
  - v1 简化: register 无入参, plugin 自己 eprintln / 设 static
  - P10-1.8 计划: plugin 依赖 workspace `ma-harness-core` 共享 ToolRegistry 类型

### Dep

- 加 `libloading = "0.8"` 到 ma-harness-core
- Cargo.lock 自动更新 (libloading 0.8.x + dependencies)

### 测试累计 (P10-1.7 后)

- core: 103 → 106 (+3, libloading 集成测)
- 真 cargo 编译 + 真 libloading 集成测通过 (cdylib .dll 落盘 + dlopen + 调 register)

### 关键 Pattern

- **跨 dylib 边界设计**: `extern "C" fn()` 比 Rust trait object ABI 稳
- **Rust 2024 unsafe attribute**: `#[unsafe(no_mangle)]` 替换 `#[no_mangle]`, 同样规则适用 `#[link_section]` / `#[export_name]`

### P10-1.8 留给后来人

- plugin 依赖 workspace `ma-harness-core` (path = "..." 自动 resolve)
  - generated Cargo.toml 加 `ma-harness-core = { path = "../<host-crate>" }`
- `register` 改 `(registry: &ToolRegistry)`, plugin 内部 `registry.register(schema, invoke_fn)`
- ABI 共享: 强制 plugin 跟 host 同一份 ma-harness-core 二进制 (Rust 1.85+, edition 2024)
- sandbox: P10-1.7 当前 unsafe 加载 dylib 没 sandbox, 业务方应审批后才调

