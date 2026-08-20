# ma-harness.rs — 架构映射 (Arch Map)

[English](../ma-harness-arch-map.md) | [简体中文](ma-harness-arch-map.md)

> 目的: 把 dsh (DeepSeek Harness) 的核心机制,映射到 ma-harness.rs 的 Rust 实现。
> 一份"翻译表",给 Week 1-2 实现 Cordis-rs 时看,避免边写边发明。
>
> 关键原则:
> - **不 1:1 翻译**。Rust 生态有更地道的表达,优先用 Rust idiom。
> - **不引入 dsh 的"包袱"**。dsh 是 Node + Cordis 生态里生长出来的,有些"是 Node 才需要"的复杂度我们直接砍。
> - **dsh 是参照系,不是圣经**。如果 Rust 这边有更直接的表达,大胆换。

---

## 1. 全景对比

| 维度 | dsh (TypeScript) | ma-harness.rs (Rust) | 备注 |
|---|---|---|---|
| 语言 | TypeScript 5.x | Rust 1.85+ (edition 2024, MSRV 1.85) | |
| 元框架 | Cordis (Yifan Shi, npm) | `ma_harness_cordis` (自主重写) | 不依赖 cordis npm 包 |
| 协议 | JSON-RPC + WebSocket | Protobuf over gRPC (tonic) | 单协议,控制面 + 数据面合一 |
| 异步 | Promise / async-await | tokio 1.x | |
| 上下文存储 | `Map<string, unknown>` (ctx) | `dashmap` / 类型化 ctx | 类型安全优先 |
| 日志 | 内存 + flush | rusqlite append-only | 不变量: model-visible means logged |
| 插件语言 | TypeScript | Rust + (Phase 2: wasmtime / deno_core) | Phase 1 只 Rust |
| 插件清单 | `plugin.toml` (YAML/JSON) | `plugin.toml` (YAML, 同名同结构) | JSON Schema 校验 |
| Sandbox | bubblewrap / Seatbelt / restricted-token | landlock 0.4 (Linux) / sandbox-exec (macOS 占位) | Windows Phase 2 |
| 模式 | 4 个 (Standard / PTC / Minimal / Creator) | Phase 1 只 Default | PTC → Phase 2 |
| 模型调用 | 适配器 (OpenAI / Anthropic / 内部) | 适配器 (同形态) | Protobuf schema 一致 |
| AGENTS.md 注入 | yes | yes | 字段对齐 |

---

## 2. Cordis 元框架 (核心重写)

dsh 的 Cordis 提供的能力:

| Cordis 能力 | dsh 实现 | ma-harness.rs 实现 |
|---|---|---|
| Context (DI 容器) | `class Context { tables, state, registry }` | `Context` struct + `dashmap` + 泛型 typed key |
| Service 装饰器 | `@Injectable()` 类装饰器 | `#[dsh_service]` proc-macro + `Service` trait |
| Plugin 注册 | `ctx.plugin(SomePlugin)` | `ctx.plugin(Plugin::install(ctx)?)` + 显式 `apply` |
| Listener (事件) | `ctx.on('event', handler)` | `ctx.on(Event::Foo, handler)` + 类型化 event enum |
| Command (指令) | `ctx.command('name', handler)` | `#[dsh_command("name", ...)]` proc-macro |
| Disposable (资源) | `ctx.effect(() => cleanup)` / `dispose()` | `Disposable` trait + `ctx.scope()` RAII |
| 派生 ctx | `ctx.parent.derive()` | `Context::fork()` 父子链 |
| 生命周期 | `start / stop / dispose` | 同名 + 强类型 LifecycleState enum |

### 2.1 我们**不**照搬的

| dsh 特性 | 不照搬理由 |
|---|---|
| `ctx.any()` (无类型化访问) | 编译期类型安全 > 运行时灵活,Rust 直接砍 |
| 反射式 service 查找 (`ctx.get(SVC_NAME)`) | Rust 用 trait + 泛型,不要字符串 |
| 装饰器元数据 (装饰器在 TS 是运行时) | Rust 用 proc-macro,编译期生成 |
| 动态插件加载 (运行时 require) | 编译期 link,Phase 2 才考虑 dynamic loading |
| 异步 ctx 钩子 (`ctx.before('start')`) | 用 tokio task + listener 替代 |

### 2.2 ctx key 强类型

dsh: `ctx.set('foo', 1)`,读时 `ctx.get('foo')` 是 `unknown`。
ma-harness: 用 phantom-typed key:

```rust
struct CtxKey<T: 'static>(PhantomData<T>);

static SESSION_ID: CtxKey<String> = CtxKey(PhantomData);

ctx.set(SESSION_ID, "abc".to_string());
let id: String = ctx.get(SESSION_ID).unwrap();
```

编译期保证 T 匹配,不需要运行时 type guard。

### 2.3 snake_case 强制

ctx key 名字 (例如 `SessionId` 类型名 / `"session_id"` 字符串字面量) 走 proc-macro 编译期检查:

```rust
// 这样写编译错误: camelCase not allowed
let k = ctx_key!("agentLoop");

// 这样写 OK
let k = ctx_key!("agent_loop");
```

proc-macro 在 `lit` 阶段就 reject camelCase 字符串,绕不开。

---

## 3. Seam 类型 (Phase 1 选 3-4 个,不全做)

dsh 9 个 Seam 我们**不全做**。Phase 1 范围:

| dsh Seam | ma-harness Phase 1 | 备注 |
|---|---|---|
| `Tool` | ✅ 做 | proc-macro `#[dsh_tool]` |
| `Listener` | ✅ 做 | proc-macro `#[dsh_listener]` |
| `Handler` | ✅ 做 | proc-macro `#[dsh_handler]` |
| `Service` | ✅ 做 | proc-macro `#[dsh_service]` |
| `Command` | ✅ 做 | proc-macro `#[dsh_command]` |
| `Middleware` | ⏸ Phase 2 | salvo middleware 即可,不需要自创 |
| `Guard` | ⏸ Phase 2 | 输入校验,Phase 2 加 |
| `Adapter` (model) | ⏸ Phase 2 | model 适配器先手写 1 个 |
| `Disposable` (seam, 不是 trait) | ❌ 不做 | Rust 有 Drop trait,不需要 Seam |

> **原则**: Seam 存在是为了"插件作者可以用统一模式声明能力"。我们 Phase 1 只暴露
> 5 个最有用的 (Tool / Listener / Handler / Service / Command),其他用 Rust idiom 替代。

---

## 4. SessionEvent 日志

| dsh 行为 | ma-harness 实现 |
|---|---|
| Append-only `SessionEvent` 数组 | SQLite 表 `events(seq INTEGER PRIMARY KEY, session_id, type, payload, ts)` |
| `model-visible means logged` 不变量 | `ctx.emit(Event)` 强制要求 `model_visible: true` 必须有对应 `Event::ModelVisible` |
| 内存 buffer + flush | 直接 sync write (rusqlite 够用,Phase 2 再上 batch) |
| Event replay (重放到 model context) | `events.iter().filter(model_visible).map(format_for_model)` |
| Multi-session | `session_id` 列分区 |

### 4.1 不变量强制点

- **写时**: `ctx.emit(event)` 内部检查。如果 `event.model_visible == true`,
  必须在同事务内落库,事务失败 → panic(append-only 不能丢)。
- **读时**: model context 组装函数 (`for_model()`) 只走日志,不走 ctx 内存。
  这样保证"看到的 = 落库的"。
- **编译期**: `Event` enum 派生 `ModelVisible` trait,`model_visible()` 方法返回 bool,
  漏写 → 编译警告。

---

## 5. Operating Mode (Phase 1 只 Default)

| dsh Mode | ma-harness Phase 1 | 备注 |
|---|---|---|
| Standard | ✅ 拆成"Default" + "Minimal" | Default 跑全功能,Mini 不跑 plugin |
| PTC (Code Mode) | ⏸ Phase 2 | 见 `docs/code-mode-deferred.md` |
| Minimal | ⏸ Phase 2 (跟 Default 一起实现,PoC 跑 Default 即可) | 跑通 Default 顺手做 Minimal 是 1 周工作 |
| Creator | ⏸ Phase 3 (生成代码用) | 跟 plugin / codegen 关系深,放最后 |

> **备注 (P11+ 进展)**: PTC (Code Mode) 实际在 P2.6 (Day 50+) 通过 `ma-harness-code` (wasmtime) 出货。Creator 在 P10 (`ma-harness-creator` + 动态 plugin 加载) 出货。Default + Minimal 在 P7 都出货了。

### 5.1 Default 模式的最小行为

```
1. 加载 plugin.toml → 注册 6 个 first-party 插件
2. ctx.plugin(Plugin).apply()
3. event loop: 接收 AgentRun { prompt } → 走 model adapter → emit SessionEvent
4. 工具调用循环: 收到 tool_call → ctx.invoke(tool, args) → 记录到日志 → 继续
5. 终止: 收到 finish_reason → 落库 end session → 退出
```

---

## 6. Plugin (first-party 6 个)

| 插件 | dsh 里有吗 | ma-harness Phase 1 |
|---|---|---|
| bash | ✅ | ✅ `ma_harness_plugin_bash` (子进程 + landlock) |
| fs | ✅ | ✅ `ma_harness_plugin_fs` (path 限制 + read/write) |
| web | ✅ | ✅ `ma_harness_plugin_web` (reqwest + url 过滤) |
| subagent | ✅ | ✅ `ma_harness_plugin_subagent` (fork ctx) |
| skill | ✅ | ✅ `ma_harness_plugin_skill` (加载 .skill/ 目录) |
| cordis | ✅ | ✅ `ma_harness_plugin_cordis` (meta 插件,展示 ctx 自身能力) |
| memory | dsh 有 | ⏸ Phase 2 (跟持久化 / 检索绑) |
| git | dsh 有 | ⏸ Phase 2 |
| notify | dsh 有 | ⏸ Phase 2 |
| plan | dsh 有 | ⏸ Phase 2 |

> Phase 1 选 6 个 = **基础工具齐 (bash/fs/web) + 编排基础齐 (subagent) + 自指 (cordis) + 复用 (skill)**。
> 不选 memory/git 是因为 PoC 阶段不验证"长期记忆 / VCS 集成"。

> **备注 (P10+ 进展)**: P10 加了 `ma-harness-plugin-creator` 支持动态 plugin 编译跟加载. `ma-harness-plugin-hello` 是 workspace 里的 test/demo plugin.

---

## 7. Model Adapter

dsh 有 OpenAI / Anthropic / 内部 / 自定义 4 类适配器,通过 `ctx.inject(['modelAdapter', 'openai'])` 拿。

ma-harness Phase 1:

| Adapter | ma-harness Phase 1 |
|---|---|
| OpenAI Chat Completions | ✅ (`ma_harness_model_openai`, reqwest + 简单轮询) |
| Anthropic Messages | ✅ P11-5 (vision + messages) |
| 内部 (deepseek 自家协议) | ⏸ — (dsh 有, 我们跟 dsh 划清, 不复用) |
| 自定义 (用户写) | ✅ 留 `ModelAdapter` trait, 用户 impl |

> **设计**: 不复刻 dsh 的 internal adapter (虽然 dsh 有),跟 dsh 划清 ——
> ma-harness 不做"特定厂商的特殊优化",只做 OpenAI-compatible 通用协议。

> **备注 (P11+ 进展)**: P11-5 加了 Anthropic Messages adapter. P11-5 / P11-9 加了 vision (OpenAI + Anthropic).

---

## 8. AGENTS.md 注入

dsh 行为:
1. 启动时读 `<cwd>/AGENTS.md`
2. 解析成 `Memory<Role>` 数据结构
3. 注入到 system prompt

ma-harness 行为:**完全一致**。但加 2 项扩展:

| 扩展 | 说明 |
|---|---|
| 多级搜索 | 当前目录 → 父目录链 → 仓库根 → `~/.ma-harness/AGENTS.md` 全局 |
| 分层 | `AGENTS.md` (项目) + `AGENTS.local.md` (gitignore,个人) + `~/.ma-harness/AGENTS.md` (全局) |

字段格式跟 dsh 一致,Protobuf 定义里加 `memory` message 复用。

---

## 9. 跑分 / Conformance 对齐 (Week 10-12)

| 对齐项 | dsh 来源 | ma-harness 做法 |
|---|---|---|
| Benchmark suite | dsh `bench/` | 复制 dsh 的 benchmark 脚本,改 binary 名 |
| JSONL fixtures | dsh `tests/fixtures/*.jsonl` | **直接复用**,加 `tests/fixtures_loader.rs` 转 Protobuf |
| Conformance ratio | 100% (dsh 内部) | Phase 1 目标 ≥ 95%,漏的标 `phase2_skip` |
| Latency baseline | dsh 测得 | 跑同样 workload,差分对比,差 > 30% 标 `investigate` |

### 9.1 不复刻的

- dsh 的 network setup (mock 模型 API 的方式) — 我们用 `wiremock` 重写,跟 dsh 的 mock server 划清
- dsh 的 prompt template — 我们不抄 prompt 文本,只对齐"接口"
- dsh 的 eval dataset — 内部数据不复制,只复刻 workload 形态

> **备注 (P11-2 进展)**: dsh acp-snapshot 9/9 fixture = 100% (见 `docs/dsh-benchmark-report.md`). dsh_synthetic 7/7 = 100%. dsh Terminal Bench / Toolathlon / DSBench full-stack benchmark 待业务方提供 LLM API key (P11-2.5+).

---

## 10. 跟 dsh 划清的"硬线"清单

> 这些是 dsh 有但我们**故意不做**或**反过来做**的。违反就是 dsh port 嫌疑。

| 项 | dsh | ma-harness | 原因 |
|---|---|---|---|
| cordis npm 依赖 | 是 | 否 (自主重写) | 划清生态 |
| `node:worker_threads` | 是 (Code Mode) | 否 (Phase 2 wasmtime) | 见 `code-mode-deferred.md` |
| JSON-RPC | 是 | 否 (Protobuf) | 单一协议 |
| camelCase ctx key | 是 | 否 (snake_case) | Rust idiom |
| 4 个 mode | 是 | Phase 1 只 Default | 减 scope |
| runtime plugin 加载 | 是 | 否 (compile-time) | 静态优先 |
| 9 个 Seam | 是 | Phase 1 5 个 | 减 scope |
| 内部 deepseek 模型适配 | 是 | 否 (OpenAI-only) | 划清 |

---

## 11. Week 1-2 实现优先级

按 arch map 排,Week 1-2 任务分配:

| Day | 任务 | 涉及 arch map 章节 |
|---|---|---|
| Day 1-2 | `ma_harness_cordis` 骨架: Context / Service trait / Plugin trait | §2 |
| Day 3 | ctx.extend + typed key + snake_case proc-macro | §2.2, §2.3 |
| Day 4 | hello-world 端到端 (register → inject → call) | §2 |
| Day 5 | listener / command / disposable | §2 (Cordis 完整) |
| Day 6-7 | rusqlite + SessionEvent 基础 + model-visible 不变量 | §4 |
| Day 8 | 5 个 proc-macro 签名实现 | §3 |
| Day 9 | Week 1-2 周报 | - |

---

## 12. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | 初版,Week 1-2 起步前定稿 |
| 2026-08-20 | P11+ 更新: PTC 在 P2.6 出货, Creator 在 P10, Anthropic adapter 在 P11-5, vision 在 P11-5/9, dsh conformance 9/9 在 P11-2 |
