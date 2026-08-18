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
axum 0.7           (HTTP, 仅 server 端)
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
| `ma_harness_server` | **内部** | axum + tonic 拼装层,频繁变 |
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
