# ma-harness.rs — 决策档案 (Decision Log) — Part 3 — Phase 7-10 (Code/ACP/Vision/Creator)

> 项目内部代号: **ma-harness.rs** (Rust 重写 DeepSeek Harness)
> 文档目的: 把分散在多轮对话里的关键决策落成'宪法', 任何后续修改都要回头对账

[**总目录**](decision-log.md) | 本文件: **Part 3 — Phase 7-10 (Code/ACP/Vision/Creator)**

> 章节范围: § 22-27

---
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
