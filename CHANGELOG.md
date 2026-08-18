# Changelog

> 12 周 PoC 收官版本。Phase 1 锁定,Phase 2 路线图见 `README.md` § Phase 2 路线图。

## [Unreleased]

### Phase 2 路线图 (不在 12 周 PoC scope)

- [ ] macro 增强 (`#[dsh_service(cordis, seam)]` 自动派生两套)
- [ ] Sandbox 强化 (landlock / Seatbelt syscall)
- [ ] 持久化 (SessionServiceImpl 内存换 rusqlite)
- [ ] Code Mode (wasmtime / deno_core)
- [ ] 多 model adapter (OpenAI / Anthropic)
- [ ] 真 plugin 动态装载 (conformance runner 现在用 placeholder ctx)
- [ ] 异步 listener
- [ ] listener priority
- [ ] deferred emit queue
- [ ] AsyncDisposable
- [ ] trybuild 编译失败测试
- [ ] HTTP/HTTPS inbound (除 tonic gRPC)
- [ ] 持久化 session + 重启恢复

## [0.1.0] - 2026-08-18 — 12 周 PoC 收官

> **状态**: 代码 100% 完成,验证 8% 待网络 (7890 代理不通 HTTPS)
> **累计**: 44 commit, 16 crate workspace, ~14000 行, ~167 测试, 18 bench

### 主要功能

#### 元框架 (ma_harness_cordis)
- Context: typed key storage + service registry + plugin registry + listener registry
- Service / Plugin / Listener / Disposable traits
- typed key 编译期 snake_case 强制 (`ctx_key!` macro)
- fork / scope / dispose (LIFO + 幂等)
- emit reentrancy guard (thread_local + RAII)

#### 核心 (ma_harness_core)
- SessionEvent: 14 种 EventType (SessionStart/End / RunStart/End / ModelRequest/Response/Error / ToolCall/Result/Error / UserInput/Cancel / SandboxViolation/Config) + Unspecified
- EventLog: append-only (rusqlite), model-visible means logged 不变量
- AgentLoop: Default 模式 agent loop
- StubModelAdapter: Phase 1 占位

#### 协议 (ma_harness_proto)
- Prost + tonic 0.12 codegen
- 3 个 service: AgentService / SessionService / EventService
- 包名 `ma_harness.v1` (semver-versioned)

#### 公开抽象 (ma_harness_seam)
- 5 trait: Service / Plugin / Listener / Disposable / Tool
- 5 proc-macro: `#[dsh_service]` / `#[dsh_listener]` / `#[dsh_tool]` / `#[dsh_command]` / `#[dsh_handler]`
- `ctx_key!` re-export
- PluginRegistry 公开

#### gRPC server (ma_harness_server)
- AgentService / SessionService 真实实现
- axum /health + /version

#### CLI (ma_harness_cli,二进制 `mah`)
- 7 子命令: start / run / plugins / events / conformance / bench / version

#### 6 first-party 插件
- bash: subprocess + timeout
- fs: read/write/list + 路径白名单 (fail-closed)
- web: reqwest + URL 白名单 + timeout
- subagent: fork ctx 跑子 agent
- skill: load .skill/ 目录
- cordis: ctx 反射

#### Conformance test framework (ma_harness_conformance)
- Fixture schema (JSONL, 5 category)
- FixtureLoader (from_jsonl + from_dir)
- ConformanceRunner (replay events via EventLog)
- CompareEngine (浅比对 payload_match)
- ReportWriter (Markdown + JSON)
- dsh_format: dsh 风格 fixture 转换层
- 8 合成 fixture + 7 dsh synthetic fixture

#### 18 benchmark (criterion 0.5)
- cordis: 10 bench (ctx typed key / service / emit / plugin / fork / dispose)
- core: 4 bench (EventLog append / AgentLoop / StubModel)
- seam: 4 bench (PluginRegistry / ctx plugin by_name / plugins list)

### 文档

- 10 份设计文档 (decision-log / arch-map / macro / plugin / tech-stack / code-mode / conformance-design / benchmark-design / conformance-report / benchmark-report)
- 8 份周报 (Day 0 / Week 1-2 / 3-4 / 5-6 / 7-9 / 10 / 11 / 12)
- README.md (人类入口)
- AGENTS.md (AI agent / 新成员入口)

### 工具

- 2 个 CI 平台: GitHub Actions + Gitee Go
- `.gitattributes` 跨平台 LF 规范化
- `.gitignore` target/ + IDE + OS

### 已知问题 (mental-compile only, 网络通后验证)

> 以下是 12 周内 mental-compile 写代码可能漏掉的问题,**待 `cargo check` 验证**。

- `Service::name(&self)` 是实例方法, 跟 spec 里 `name() -> &'static str` 不一样 (impl 时发现 instance method 更实用)
- `EmitGuard` 在 panic unwind 时 `Cell::set` 路径可能不安全 (需测)
- `ctx_key!` 宏展开的 type inference 可能不通过
- `ListenerEvent` 改名跟 cordis::Event 区分
- tonic-build 生成的 server trait 编译期 macro 可能跟锁定的 tonic 0.12 不匹配
- 6 first-party plugin 的 `ctx.inject(Arc::new(svc))` 跟 ctx.service::<S>() 的 TypeId 匹配待验证
- cordis 中 `ctx.set<CtxKey<T>>(value: T)` 在 `&self` 上调用, 跟 dsh 的 `ctx.set(key, value)` 行为一致, 但 immutable receiver 可能有 surprise

### 阻塞

- 本机代理 `127.0.0.1:7890` 不能代理 HTTPS, 130+ 文件 mental-compile only
- 16 crate 编译 + 167 测试 + 18 bench 跑通预计 2-3 分钟, 等网络恢复

## 早期版本 (无,这是第一个 release)

> 这是 0.1.0 第一版,直接进入 12 周 PoC 收官。
> Phase 1 走完,Phase 2 路线图见 README.md。
