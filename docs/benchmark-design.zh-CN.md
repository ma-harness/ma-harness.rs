# Benchmark Design (Week 10-11)

[English](benchmark-design.md) | [简体中文](benchmark-design.zh-CN.md)

> **目的**: 量化 ma-harness 的核心路径性能, 跟 DeepSeek Harness (dsh) 差分对比, 防止 Rust 重写后性能退化。
> **状态**: Week 10-11 设计稿, 实现中。
> **关联文档**: `conformance-design.md`、`ma-harness-arch-map.md`、`docs/tech-stack.md`

---

## TL;DR

| 维度 | 数值 |
|---|---|
| 框架 | criterion 0.5 (workspace deps 已锁) |
| Benchmark crate | 3 处 (cordis / core / seam) |
| Benchmark 数量 | ~12 个 (核心路径全覆盖) |
| 对比目标 | dsh TypeScript 跑同一组 micro-bench |
| 性能判据 | **不允许比 dsh 慢超过 30%** (e.g. dsh 1ms, ma-harness ≤ 1.3ms) |
| 输出 | `target/criterion/*/report/` (HTML), 文本 summary 进 weekly |
| 跑法 | `cargo bench --workspace` 或 `cargo bench -p ma_harness_cordis` |

---

## 1. 性能假设

| 路径 | dsh (TypeScript) | ma-harness (Rust) | 预期 |
|---|---|---|---|
| ctx.set / get (typed key) | ~0.5µs (Map lookup) | ~50ns (HashMap) | **10x 快** |
| ctx.service (TypeId lookup) | ~1µs (class instance Map) | ~80ns (HashMap) | **10x 快** |
| ctx.emit (10 listener) | ~5µs (forEach + call) | ~500ns (fn ptr call x 10) | **10x 快** |
| ctx.plugin install | ~10µs (new + lifecycle) | ~2µs (Arc + state set) | **5x 快** |
| ctx.fork (10 service) | ~15µs (deep clone) | ~200ns (Arc clone x 10) | **75x 快** |
| AgentLoop 1 step | ~3ms (V8 + TS) | ~50µs (Rust) | **60x 快** |
| EventLog append (1000 events) | ~5ms (array push) | ~50µs (Vec + lock-free) | **100x 快** |
| ToolRegistry call (1 tool) | ~2ms (IPC overhead in dsh) | ~100µs (in-process) | **20x 快** |

**最差情况**: ctx.emit reentrancy guard 开销 (thread_local 读) 可能跟 dsh 的 forEach 持平, 但仍不会更慢。

**核心假设**: Rust 的零成本抽象 + Arc 共享, 让 ma-harness 在元框架层比 dsh 快 5-100x, 这是 Rust 重写的主要价值之一。

## 2. Benchmark 范围

**测什么** (核心热路径):

1. ctx typed key set/get (高频, plugin 配置读)
2. ctx.service / inject (高频, plugin 间协作)
3. ctx.emit (高频, event-driven 核心)
4. ctx.plugin install / uninstall (中频, 启动期)
5. ctx.fork / dispose (中频, sub-agent + scope)
6. EventLog append (高频, 每次 emit 都触发)
7. EventLog read_all (低频, 但量大)
8. AgentLoop 1 步 (高频, agent 主循环)
9. StubModelAdapter call (高频, LLM 占位)
10. ToolRegistry call (高频, tool dispatch)
11. PluginRegistry load (低频, 启动期)
12. ctx.listener 10 个 vs 100 个 (验证 listener 数线性)

**不测什么** (成本 > 价值):

- 真实 model adapter (需要 LLM API key, 跑不动)
- 真实 sqlite (I/O bound, 不是元框架关心)
- 网络 (reqwest 自己的 bench 够用)
- gRPC server (tonic 自己的 bench 够用)
- proc-macro 编译期 (不可运行时 bench)

## 3. Benchmark 矩阵

### 3.1 `ma_harness_cordis/benches/core.rs`

| Bench | 操作 | 数据 |
|---|---|---|
| `ctx_set_typed_key` | `ctx.set::<MyKey>(value)` 1000 次 | String 8B |
| `ctx_get_typed_key` | `ctx.get::<MyKey>()` 1000 次 | String 8B |
| `ctx_inject_service` | `ctx.inject::<Svc>(arc)` 1000 次 | 1 个 service |
| `ctx_service_lookup` | `ctx.service::<Svc>()` 1000 次 | 1 个 service |
| `ctx_emit_no_listeners` | `ctx.emit(MyEvent)` 1000 次 | 0 listener |
| `ctx_emit_10_listeners` | `ctx.emit(MyEvent)` 1000 次 | 10 listener |
| `ctx_emit_100_listeners` | `ctx.emit(MyEvent)` 1000 次 | 100 listener |
| `ctx_plugin_install_uninstall` | install + uninstall 1000 次 | 1 个 plugin |
| `ctx_fork_with_10_services` | `ctx.fork()` 1000 次 | 10 service |
| `ctx_dispose` | `ctx.dispose()` 1000 次 | scope 5 listener |

### 3.2 `ma_harness_core/benches/agent.rs`

| Bench | 操作 | 数据 |
|---|---|---|
| `event_log_append_1k` | append 1000 个 SessionEvent | 简单 event |
| `event_log_read_all_10k` | read_all 10000 个 event | append-only |
| `agent_loop_1_step` | AgentLoop 1 步 (无 tool) | StubModel |
| `agent_loop_1_step_with_tool` | AgentLoop 1 步 + 1 tool call | StubModel + 1 tool |
| `stub_model_adapter_call` | StubModel.call 1000 次 | 固定 response |

### 3.3 `ma_harness_seam/benches/plugin.rs`

| Bench | 操作 | 数据 |
|---|---|---|
| `plugin_registry_register` | PluginRegistry.register 1000 次 | 10 plugin |
| `plugin_registry_get_by_name` | PluginRegistry.by_name 1000 次 | 100 plugin |
| `seam_ctx_build` | build_seam_ctx 1000 次 | 5 service + 3 plugin |

## 4. Criterion 用法

每个 bench 写一个独立 fn, criterion 自动加 warmup + 多次采样:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_ctx_set_typed_key(c: &mut Criterion) {
    let ctx = ma_harness_cordis::Context::new();
    let key = ctx_key!(my_key);
    let value = "hello".to_string();
    c.bench_function("ctx_set_typed_key", |b| {
        b.iter(|| {
            ctx.set(key, black_box(value.clone()));
        });
    });
}

criterion_group!(benches, bench_ctx_set_typed_key);
criterion_main!(benches);
```

**配置** (`Cargo.toml [bench]`):

```toml
[[bench]]
name = "core"
harness = false  # 用 criterion, 不是 libtest
```

## 5. 跑法

```bash
# 全部 bench
cargo bench --workspace

# 单 crate
cargo bench -p ma_harness_cordis

# 单个 bench
cargo bench -p ma_harness_cordis --bench core -- ctx_set_typed_key

# 短跑 (开发期, 默认 100 sample, criterion 默认 100)
cargo bench -p ma_harness_cordis -- --quick
```

输出:

- `target/criterion/<bench_name>/report/` — HTML 报告 (含 chart + summary)
- `target/criterion/<bench_name>/base/estimates.json` — 原始数据
- `target/criterion/<bench_name>/base/sample.json` — 单次 sample

## 6. 跟 dsh 对比

**dsh TypeScript bench** (用 `tinybench` 写, 同样操作):

```typescript
import { Bench } from 'tinybench';
import { Context } from '@deepkit/harness';

const bench = new Bench();
const ctx = new Context();

bench.add('ctx_set_typed_key', () => {
  ctx.set('my_key', 'hello');
});

await bench.run();
console.table(bench.table());
```

**对比表** (Week 11 出, 模板):

| Bench | dsh (V8) | ma-harness (Rust) | 倍数 |
|---|---|---|---|
| ctx_set_typed_key | 0.5µs | 50ns | 10x |
| ctx_service_lookup | 1.2µs | 80ns | 15x |
| ctx_emit_10_listeners | 5.3µs | 520ns | 10x |
| AgentLoop 1 step | 3.1ms | 48µs | 64x |
| EventLog append x 1000 | 4.8ms | 52µs | 92x |
| **几何平均** | — | — | **~30x** |

**判据**:

- 任何 bench **慢过 dsh** → 立刻优化 (考虑移除 Arc clone, 用 Rc 一次性路径等)
- 任何 bench **慢过 dsh 30%** → 进 P1 优化 backlog
- 几何平均 < 10x → Phase 2 投入优化

## 7. Benchmark 治理

**禁止**:

- bench 里 sleep / yield / tokio::time::sleep — 不可重现
- bench 里 print — 输出污染
- bench 用全局可变状态 (除 criterion 自己的 state) — 不可重现
- bench 跑 < 100 sample — 噪声大, 数据不可信

**要求**:

- 每个 bench 注释**数据规模** (e.g. `// 1000 次 set, 8 字节 value`)
- 每个 bench 注释**预热** (criterion 默认 3s warmup, 不需要改)
- 新 bench 必须在 PR 描述里写 "为什么这个 bench 重要"
- 删 bench 必须在 commit message 里写 "为什么不需要了"

## 8. 不在 scope

- **不**做持续集成 (CI 跑 bench 太慢, 通常 nightly)
- **不**做内存 benchmark (criterion 测时间, 内存用 `dhat` 单独跑)
- **不**做 async bench (criterion 不直接支持, 用 `tokio-test` block_on, 但这跟真实 runtime 有 gap, 见 Phase 2 `criterion-async` 升级)
- **不**做并发 bench (元框架核心是单线程快路径, 并发在 plugin 层)

## 9. Phase 2 升级

| 现状 | Phase 2 |
|---|---|
| criterion 0.5 (libtest) | criterion-async 0.5 (tokio runtime) |
| 同步 bench | async bench (AgentLoop async 路径) |
| 单 bench 文件 | bench 模板 + 自动生成 |
| 手动跑 | cargo bench 自动出 PR comment (GitHub Action) |

## 10. 跟其它 doc 的关系

- `conformance-design.md` — 行为等价, 性能正交
- `docs/weekly/004-w07-w09.md` — Week 7-9 完成, Week 10-11 跑 bench
- `docs/tech-stack.md` §"测试栈" — criterion 选型理由

---

## 给后来人

新加 bench:

1. 先确认**有现实热路径**对应 (不要 bench 假想场景)
2. 用 `black_box()` 防止编译器优化掉
3. 跑 3 次取中位数 (criterion 自己会做, 不要手动)
4. 写 commit message 写清楚"为什么这个 bench"

发现 bench 慢:

1. 先看 `target/criterion/<name>/report/` 的 violin plot
2. 看 sample 是否双峰 (GC / 调度抖动) — 加 warmup
3. 看 flamegraph (`cargo flamegraph --bench core`) — 找热点
4. 别急着优化, 确认是热路径 (不是 cold path)
