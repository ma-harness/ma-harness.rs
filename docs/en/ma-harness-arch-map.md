# ma-harness.rs — Architecture Map

[English](ma-harness-arch-map.md) | [简体中文](../zh-CN/ma-harness-arch-map.md)

> **Purpose**: Map the core mechanisms of dsh (DeepSeek Harness) to the
> ma-harness.rs Rust implementation. A "translation table" used when
> implementing `cordis-rs` in Week 1-2 to avoid reinventing on the fly.
>
> **Key principles**:
> - **No 1:1 translation**. The Rust ecosystem has more idiomatic expressions;
>   prefer Rust idioms.
> - **No dsh "baggage"**. dsh grew up in the Node + Cordis ecosystem; some
>   complexity is "needed only because of Node" and we cut it.
> - **dsh is a reference, not scripture**. If Rust has a more direct expression,
>   switch boldly.

---

## 1. Full-picture comparison

| Dimension             | dsh (TypeScript)                                | ma-harness.rs (Rust)                                  | Notes |
|-----------------------|--------------------------------------------------|-------------------------------------------------------|-------|
| Language              | TypeScript 5.x                                  | Rust 1.85+ (edition 2024, MSRV 1.85)                  | |
| Meta-framework        | Cordis (Yifan Shi, npm)                          | `ma_harness_cordis` (rewritten from scratch)          | Does not depend on the cordis npm package |
| Protocol              | JSON-RPC + WebSocket                             | Protobuf over gRPC (tonic)                            | Single protocol, control plane + data plane unified |
| Async                 | Promise / async-await                            | tokio 1.x                                             | |
| Context storage       | `Map<string, unknown>` (ctx)                     | `dashmap` / typed ctx                                 | Type safety first |
| Logging               | in-memory + flush                                | rusqlite append-only                                  | Invariant: model-visible means logged |
| Plugin language       | TypeScript                                       | Rust + (Phase 2: wasmtime / deno_core)                | Phase 1 is Rust only |
| Plugin manifest       | `plugin.toml` (YAML/JSON)                        | `plugin.toml` (YAML, same name, same structure)       | JSON Schema validation |
| Sandbox               | bubblewrap / Seatbelt / restricted-token         | landlock 0.4 (Linux) / sandbox-exec (macOS placeholder) | Windows in Phase 2 |
| Modes                 | 4 (Standard / PTC / Minimal / Creator)          | Phase 1 only Default                                  | PTC → Phase 2 |
| Model call            | adapter (OpenAI / Anthropic / internal)          | adapter (same shape)                                  | Protobuf schema identical |
| AGENTS.md injection   | yes                                              | yes                                                   | Fields aligned |

---

## 2. Cordis meta-framework (core rewrite)

Capabilities provided by dsh's Cordis:

| Cordis capability          | dsh implementation                                  | ma-harness.rs implementation                                |
|----------------------------|------------------------------------------------------|--------------------------------------------------------------|
| Context (DI container)     | `class Context { tables, state, registry }`          | `Context` struct + `dashmap` + generic typed key             |
| Service decorator          | `@Injectable()` class decorator                      | `#[dsh_service]` proc-macro + `Service` trait                |
| Plugin registration        | `ctx.plugin(SomePlugin)`                             | `ctx.plugin(Plugin::install(ctx)?)` + explicit `apply`        |
| Listener (event)           | `ctx.on('event', handler)`                           | `ctx.on(Event::Foo, handler)` + typed event enum              |
| Command                    | `ctx.command('name', handler)`                       | `#[dsh_command("name", ...)]` proc-macro                     |
| Disposable (resource)      | `ctx.effect(() => cleanup)` / `dispose()`            | `Disposable` trait + `ctx.scope()` RAII                      |
| Derived ctx                | `ctx.parent.derive()`                                 | `Context::fork()` parent-child chain                         |
| Lifecycle                  | `start / stop / dispose`                             | Same names + strongly typed `LifecycleState` enum             |

### 2.1 What we **don't** copy

| dsh feature                              | Reason for not copying                                       |
|------------------------------------------|--------------------------------------------------------------|
| `ctx.any()` (untyped access)             | Compile-time type safety > runtime flexibility; cut in Rust  |
| Reflective service lookup (`ctx.get(SVC_NAME)`) | Rust uses trait + generics, no strings needed        |
| Decorator metadata (runtime in TS)       | Rust uses proc-macros, generated at compile time             |
| Dynamic plugin loading (runtime require) | Compile-time link; dynamic loading considered in Phase 2     |
| Async ctx hooks (`ctx.before('start')`)  | Use tokio task + listener instead                            |

### 2.2 Strongly-typed ctx key

dsh: `ctx.set('foo', 1)`, read returns `unknown`.
ma-harness: use phantom-typed key:

```rust
struct CtxKey<T: 'static>(PhantomData<T>);

static SESSION_ID: CtxKey<String> = CtxKey(PhantomData);

ctx.set(SESSION_ID, "abc".to_string());
let id: String = ctx.get(SESSION_ID).unwrap();
```

Compile-time guarantees that `T` matches; no runtime type guard needed.

### 2.3 snake_case enforcement

ctx key names (e.g. `SessionId` type name / `"session_id"` string literal)
are checked at compile time via proc-macro:

```rust
// This fails to compile: camelCase not allowed
let k = ctx_key!("agentLoop");

// This is OK
let k = ctx_key!("agent_loop");
```

The proc-macro rejects camelCase strings at the `lit` stage — no way around it.

---

## 3. Seam types (Phase 1 picks 3-4, not all)

We **do not implement all 9** of dsh's Seams. Phase 1 scope:

| dsh Seam                              | ma-harness Phase 1 | Notes                                  |
|---------------------------------------|--------------------|----------------------------------------|
| `Tool`                                | ✅ done            | proc-macro `#[dsh_tool]`               |
| `Listener`                            | ✅ done            | proc-macro `#[dsh_listener]`           |
| `Handler`                             | ✅ done            | proc-macro `#[dsh_handler]`            |
| `Service`                             | ✅ done            | proc-macro `#[dsh_service]`            |
| `Command`                             | ✅ done            | proc-macro `#[dsh_command]`            |
| `Middleware`                          | ⏸ Phase 2         | salvo middleware suffices              |
| `Guard`                               | ⏸ Phase 2         | input validation, added in Phase 2     |
| `Adapter` (model)                     | ⏸ Phase 2         | write 1 model adapter by hand first    |
| `Disposable` (seam, not trait)        | ❌ not done        | Rust has the Drop trait                |

> **Principle**: Seams exist so "plugin authors can use a uniform pattern to
> declare capabilities". Phase 1 exposes only the 5 most useful
> (Tool / Listener / Handler / Service / Command); the rest use Rust idioms.

---

## 4. SessionEvent log

| dsh behavior                                  | ma-harness implementation                                              |
|-----------------------------------------------|------------------------------------------------------------------------|
| Append-only `SessionEvent` array              | SQLite table `events(seq INTEGER PRIMARY KEY, session_id, type, payload, ts)` |
| Invariant: `model-visible means logged`       | `ctx.emit(Event)` enforces that `model_visible: true` requires a corresponding `Event::ModelVisible` |
| in-memory buffer + flush                      | Direct sync write (rusqlite is enough; batching in Phase 2)            |
| Event replay (back to model context)          | `events.iter().filter(model_visible).map(format_for_model)`            |
| Multi-session                                 | `session_id` column partition                                          |

### 4.1 Where the invariant is enforced

- **On write**: `ctx.emit(event)` checks internally. If `event.model_visible == true`,
  it must be persisted in the same transaction; transaction failure → panic
  (append-only cannot drop).
- **On read**: the model context assembly function (`for_model()`) goes through
  the log only, not the in-memory ctx. This guarantees "what you see = what was
  persisted".
- **At compile time**: `Event` enum derives the `ModelVisible` trait, the
  `model_visible()` method returns bool; missing it → compile warning.

---

## 5. Operating Mode (Phase 1 is Default only)

| dsh Mode   | ma-harness Phase 1                | Notes                                          |
|------------|------------------------------------|------------------------------------------------|
| Standard   | ✅ split into "Default" + "Minimal" | Default runs full features; Minimal skips plugin |
| PTC (Code Mode) | ⏸ Phase 2                  | see `docs/code-mode-deferred.md`               |
| Minimal    | ⏸ Phase 2 (along with Default)     | Running Default first; Minimal is 1 week of work |
| Creator    | ⏸ Phase 3 (for code generation)   | Heavily tied to plugin / codegen; do last      |

> **Note (P11+ update)**: PTC (Code Mode) was actually shipped in P2.6 (Day 50+)
> via `ma-harness-code` (wasmtime-based). Creator mode shipped in P10
> (`ma-harness-creator` + dynamic plugin loading). Default + Minimal both
> shipped in P7.

### 5.1 Minimum behavior of Default mode

```
1. Load plugin.toml → register 6 first-party plugins
2. ctx.plugin(Plugin).apply()
3. event loop: receive AgentRun { prompt } → go through model adapter → emit SessionEvent
4. Tool call loop: receive tool_call → ctx.invoke(tool, args) → record to log → continue
5. Termination: receive finish_reason → persist end session → exit
```

---

## 6. Plugin (6 first-party)

| Plugin     | In dsh? | ma-harness Phase 1                                       |
|------------|---------|----------------------------------------------------------|
| bash       | ✅      | ✅ `ma_harness_plugin_bash` (subprocess + landlock)      |
| fs         | ✅      | ✅ `ma_harness_plugin_fs` (path restriction + read/write)|
| web        | ✅      | ✅ `ma_harness_plugin_web` (reqwest + URL filter)        |
| subagent   | ✅      | ✅ `ma_harness_plugin_subagent` (fork ctx)               |
| skill      | ✅      | ✅ `ma_harness_plugin_skill` (load `.skill/` directory)  |
| cordis     | ✅      | ✅ `ma_harness_plugin_cordis` (meta plugin, demonstrates ctx capabilities) |
| memory     | dsh has | ⏸ Phase 2 (bound to persistence / retrieval)             |
| git        | dsh has | ⏸ Phase 2                                                 |
| notify     | dsh has | ⏸ Phase 2                                                 |
| plan       | dsh has | ⏸ Phase 2                                                 |

> Phase 1 picks 6 = **basic tool set complete (bash/fs/web) + orchestration
> basics complete (subagent) + self-reference (cordis) + reuse (skill)**.
> Not picking memory/git because the PoC does not validate "long-term memory
> / VCS integration".

> **Note (P10+ update)**: `ma-harness-plugin-creator` was added in P10 to
> support dynamic plugin compilation and loading. `ma-harness-plugin-hello`
> is a test/demo plugin included in the workspace.

---

## 7. Model Adapter

dsh has OpenAI / Anthropic / internal / custom 4 types of adapters, obtained
via `ctx.inject(['modelAdapter', 'openai'])`.

ma-harness Phase 1:

| Adapter                            | ma-harness Phase 1                                       |
|------------------------------------|----------------------------------------------------------|
| OpenAI Chat Completions            | ✅ `ma_harness_model_openai` (reqwest + simple polling)  |
| Anthropic Messages                 | ✅ `ma_harness_model_anthropic` (P11-5)                  |
| Internal (deepseek's own protocol) | ⏸ — (dsh has it; we don't reuse, draw the line)         |
| Custom (user-written)              | ✅ leaves `ModelAdapter` trait, user impl                 |

> **Design**: Do not replicate dsh's internal adapter (even though dsh has it);
> draw the line — ma-harness does not do "special optimizations for a specific
> vendor", only the OpenAI-compatible general protocol.

> **Note (P11+ update)**: Anthropic Messages adapter was added in P11-5 (vision
> + messages). Vision support (P11-5 / P11-9) covers OpenAI and Anthropic.

---

## 8. AGENTS.md injection

dsh behavior:
1. Read `<cwd>/AGENTS.md` on startup
2. Parse into a `Memory<Role>` data structure
3. Inject into system prompt

ma-harness behavior: **exactly the same**. Plus 2 extensions:

| Extension       | Description |
|-----------------|-------------|
| Multi-level search | Current directory → parent chain → repo root → `~/.ma-harness/AGENTS.md` (global) |
| Layering       | `AGENTS.md` (project) + `AGENTS.local.md` (gitignore, personal) + `~/.ma-harness/AGENTS.md` (global) |

Field format matches dsh; the Protobuf definition reuses a `memory` message.

---

## 9. Benchmark / Conformance alignment (Week 10-12)

| Alignment item        | dsh source                          | ma-harness approach                                         |
|-----------------------|-------------------------------------|-------------------------------------------------------------|
| Benchmark suite       | dsh `bench/`                        | Copy dsh's benchmark script, change the binary name         |
| JSONL fixtures        | dsh `tests/fixtures/*.jsonl`        | **Reuse directly**; add `tests/fixtures_loader.rs` to convert to Protobuf |
| Conformance ratio     | 100% (dsh internal)                 | Phase 1 target ≥ 95%; gaps marked `phase2_skip`             |
| Latency baseline      | measured by dsh                     | Run the same workload; diff > 30% mark as `investigate`     |

### 9.1 What we don't replicate

- dsh's network setup (how it mocks the model API) — we rewrite with `wiremock`,
  drawing the line from dsh's mock server
- dsh's prompt template — we don't copy the prompt text, only align the "interface"
- dsh's eval dataset — internal data is not copied, only the workload shape

> **Note (P11-2 update)**: dsh acp-snapshot 9/9 fixture = 100% (see
> `docs/dsh-benchmark-report.md`). dsh_synthetic 7/7 = 100%. dsh Terminal
> Bench / Toolathlon / DSBench full-stack benchmarks are pending business-side
> LLM API key (P11-2.5+).

---

## 10. The "hard line" we draw from dsh

> These are things dsh does that we **deliberately don't do** or **do in
> reverse**. Violating them would be a dsh port smell.

| Item                            | dsh     | ma-harness                  | Reason                |
|---------------------------------|---------|-----------------------------|-----------------------|
| cordis npm dependency           | yes     | no (rewritten from scratch) | draw the ecosystem line |
| `node:worker_threads`           | yes (Code Mode) | no (Phase 2 wasmtime) | see `code-mode-deferred.md` |
| JSON-RPC                        | yes     | no (Protobuf)               | single protocol       |
| camelCase ctx key               | yes     | no (snake_case)             | Rust idiom            |
| 4 modes                         | yes     | Phase 1 only Default        | reduce scope          |
| runtime plugin loading          | yes     | no (compile-time)           | static-first          |
| 9 Seams                         | yes     | Phase 1: 5                  | reduce scope          |
| internal deepseek model adapter | yes     | no (OpenAI-only)            | draw the line         |

---

## 11. Week 1-2 implementation priority

Tasks allocated by arch map for Week 1-2:

| Day       | Task                                                 | Arch map section |
|-----------|------------------------------------------------------|------------------|
| Day 1-2   | `ma_harness_cordis` skeleton: Context / Service trait / Plugin trait | §2 |
| Day 3     | ctx.extend + typed key + snake_case proc-macro       | §2.2, §2.3       |
| Day 4     | hello-world end-to-end (register → inject → call)    | §2               |
| Day 5     | listener / command / disposable                     | §2 (Cordis full) |
| Day 6-7   | rusqlite + SessionEvent basics + model-visible invariant | §4           |
| Day 8     | 5 proc-macro signature implementation                | §3               |
| Day 9     | Week 1-2 weekly report                               | -                |

---

## 12. Changelog

| Date       | Change |
|------------|--------|
| 2026-08-18 | Initial version, finalized before Week 1-2 |
| 2026-08-20 | P11+ updates: PTC shipped in P2.6, Creator in P10, Anthropic adapter in P11-5, vision in P11-5/9, dsh conformance 9/9 in P11-2 |
| 2026-08-21 | P13 dsh-adapter design complete: §13 added, design/dsh-adapter.md 16628 bytes, 5 phases × 1 week, reuse dsh JSON-RPC server, lock dsh 0.1.0-rc.5 |
