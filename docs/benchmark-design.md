# Benchmark Design (Week 10-11)

[English](benchmark-design.md) | [简体中文](zh-CN/benchmark-design.md)

> **Purpose**: Quantify the performance of ma-harness's core paths and compare
> with DeepSeek Harness (dsh), to prevent regression after the Rust rewrite.
> **Status**: Week 10-11 design draft; implementation in progress.
> **Related docs**: `conformance-design.md`, `ma-harness-arch-map.md`,
> `docs/tech-stack.md`

---

## TL;DR

| Dimension           | Value |
|---------------------|-------|
| Framework           | criterion 0.5 (locked in workspace deps) |
| Benchmark crates    | 3 (cordis / core / seam) |
| Benchmark count     | ~12 (full coverage of core paths) |
| Comparison target   | dsh TypeScript running the same set of micro-benches |
| Performance bar     | **must not be > 30% slower than dsh** (e.g. dsh 1ms, ma-harness ≤ 1.3ms) |
| Output              | `target/criterion/*/report/` (HTML), text summary into weekly |
| How to run          | `cargo bench --workspace` or `cargo bench -p ma_harness_cordis` |

---

## 1. Performance assumptions

| Path                          | dsh (TypeScript)       | ma-harness (Rust)              | Expected |
|-------------------------------|------------------------|--------------------------------|----------|
| ctx.set / get (typed key)     | ~0.5µs (Map lookup)    | ~50ns (HashMap)                | **10x faster** |
| ctx.service (TypeId lookup)   | ~1µs (class instance Map) | ~80ns (HashMap)              | **10x faster** |
| ctx.emit (10 listeners)       | ~5µs (forEach + call)  | ~500ns (fn ptr call x 10)      | **10x faster** |
| ctx.plugin install            | ~10µs (new + lifecycle)| ~2µs (Arc + state set)        | **5x faster**  |
| ctx.fork (10 services)        | ~15µs (deep clone)     | ~200ns (Arc clone x 10)        | **75x faster** |
| AgentLoop 1 step              | ~3ms (V8 + TS)         | ~50µs (Rust)                   | **60x faster** |
| EventLog append (1000 events) | ~5ms (array push)      | ~50µs (Vec + lock-free)        | **100x faster**|
| ToolRegistry call (1 tool)    | ~2ms (IPC overhead in dsh) | ~100µs (in-process)         | **20x faster** |

**Worst case**: the reentrancy-guard cost in `ctx.emit` (thread_local read)
might tie dsh's `forEach`, but won't be slower.

**Core assumption**: Rust's zero-cost abstractions + Arc sharing let
ma-harness outperform dsh by 5-100x at the meta-framework layer; this is
one of the main values of the Rust rewrite.

## 2. Benchmark scope

**What to measure** (core hot paths):

1. ctx typed key set/get (high frequency, plugin config read)
2. ctx.service / inject (high frequency, plugin collaboration)
3. ctx.emit (high frequency, event-driven core)
4. ctx.plugin install / uninstall (medium frequency, startup)
5. ctx.fork / dispose (medium frequency, sub-agent + scope)
6. EventLog append (high frequency, triggered by every emit)
7. EventLog read_all (low frequency, but large volume)
8. AgentLoop 1 step (high frequency, agent main loop)
9. StubModelAdapter call (high frequency, LLM placeholder)
10. ToolRegistry call (high frequency, tool dispatch)
11. PluginRegistry load (low frequency, startup)
12. ctx.listener 10 vs 100 (verify listener count is linear)

**What NOT to measure** (cost > value):

- Real model adapter (needs LLM API key; can't run)
- Real sqlite (I/O bound; not the meta-framework's concern)
- Network (reqwest's own bench is enough)
- gRPC server (tonic's own bench is enough)
- proc-macro at compile time (cannot be runtime benched)

## 3. Benchmark matrix

### 3.1 `ma_harness_cordis/benches/core.rs`

| Bench                          | Operation                                | Data |
|--------------------------------|------------------------------------------|------|
| `ctx_set_typed_key`            | `ctx.set::<MyKey>(value)` 1000 times     | String 8B |
| `ctx_get_typed_key`            | `ctx.get::<MyKey>()` 1000 times           | String 8B |
| `ctx_inject_service`           | `ctx.inject::<Svc>(arc)` 1000 times      | 1 service |
| `ctx_service_lookup`           | `ctx.service::<Svc>()` 1000 times         | 1 service |
| `ctx_emit_no_listeners`        | `ctx.emit(MyEvent)` 1000 times            | 0 listeners |
| `ctx_emit_10_listeners`        | `ctx.emit(MyEvent)` 1000 times            | 10 listeners |
| `ctx_emit_100_listeners`       | `ctx.emit(MyEvent)` 1000 times            | 100 listeners |
| `ctx_plugin_install_uninstall` | install + uninstall 1000 times            | 1 plugin |
| `ctx_fork_with_10_services`    | `ctx.fork()` 1000 times                   | 10 services |
| `ctx_dispose`                  | `ctx.dispose()` 1000 times                | scope 5 listeners |

### 3.2 `ma_harness_core/benches/agent.rs`

| Bench                          | Operation                                | Data |
|--------------------------------|------------------------------------------|------|
| `event_log_append_1k`          | append 1000 SessionEvent                  | simple event |
| `event_log_read_all_10k`       | read_all 10000 events                     | append-only |
| `agent_loop_1_step`            | AgentLoop 1 step (no tool)                | StubModel |
| `agent_loop_1_step_with_tool`  | AgentLoop 1 step + 1 tool call             | StubModel + 1 tool |
| `stub_model_adapter_call`      | StubModel.call 1000 times                 | fixed response |

### 3.3 `ma_harness_seam/benches/plugin.rs`

| Bench                          | Operation                                | Data |
|--------------------------------|------------------------------------------|------|
| `plugin_registry_register`     | PluginRegistry.register 1000 times        | 10 plugins |
| `plugin_registry_get_by_name`  | PluginRegistry.by_name 1000 times         | 100 plugins |
| `seam_ctx_build`               | build_seam_ctx 1000 times                 | 5 services + 3 plugins |

## 4. Criterion usage

Each bench is an independent fn; criterion auto-adds warmup + multiple samples:

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

**Config** (`Cargo.toml [bench]`):

```toml
[[bench]]
name = "core"
harness = false  # use criterion, not libtest
```

## 5. How to run

```bash
# All benches
cargo bench --workspace

# Single crate
cargo bench -p ma_harness_cordis

# Single bench
cargo bench -p ma_harness_cordis --bench core -- ctx_set_typed_key

# Quick run (dev-time, default 100 samples, criterion default 100)
cargo bench -p ma_harness_cordis -- --quick
```

Output:

- `target/criterion/<bench_name>/report/` — HTML report (chart + summary)
- `target/criterion/<bench_name>/base/estimates.json` — raw data
- `target/criterion/<bench_name>/base/sample.json` — single sample

## 6. Comparison with dsh

**dsh TypeScript bench** (using `tinybench`, same operations):

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

**Comparison table** (produced in Week 11, template):

| Bench                       | dsh (V8)     | ma-harness (Rust) | Multiplier |
|-----------------------------|--------------|-------------------|------------|
| ctx_set_typed_key           | 0.5µs        | 50ns              | 10x        |
| ctx_service_lookup          | 1.2µs        | 80ns              | 15x        |
| ctx_emit_10_listeners       | 5.3µs        | 520ns             | 10x        |
| AgentLoop 1 step            | 3.1ms        | 48µs              | 64x        |
| EventLog append x 1000      | 4.8ms        | 52µs              | 92x        |
| **Geometric mean**          | —            | —                 | **~30x**   |

**Criteria**:

- Any bench **slower than dsh** → optimize immediately (consider removing
  Arc clone, using Rc for one-shot paths, etc.)
- Any bench **> 30% slower than dsh** → P1 optimization backlog
- Geometric mean < 10x → Phase 2 invest in optimization

## 7. Benchmark governance

**Forbidden**:

- `sleep` / `yield` / `tokio::time::sleep` in bench — unreproducible
- `print` in bench — output pollution
- global mutable state in bench (except criterion's own state) — unreproducible
- bench runs with < 100 samples — too noisy, data is untrustworthy

**Required**:

- Each bench comments the **data size** (e.g. `// 1000 sets, 8-byte value`)
- Each bench comments the **warmup** (criterion defaults to 3s warmup, no need to change)
- New bench must be in the PR description "why is this bench important"
- Deleting a bench must be in the commit message "why is it no longer needed"

## 8. Out of scope

- **No** continuous integration (CI running bench is too slow, usually nightly)
- **No** memory bench (criterion measures time; memory uses `dhat` separately)
- **No** async bench (criterion does not directly support; use `tokio-test` block_on,
  but this has a gap with real runtime; see Phase 2 `criterion-async` upgrade)
- **No** concurrent bench (meta-framework core is single-thread fast path;
  concurrency is in the plugin layer)

## 9. Phase 2 upgrades

| Current                          | Phase 2 |
|----------------------------------|---------|
| criterion 0.5 (libtest)          | criterion-async 0.5 (tokio runtime) |
| sync bench                       | async bench (AgentLoop async path)  |
| single bench file                | bench template + auto-generation    |
| manual run                       | `cargo bench` auto PR comment (GitHub Action) |

## 10. Relationship to other docs

- `conformance-design.md` — behavioral equivalence, orthogonal to performance
- `docs/weekly/004-w07-w09.md` — Week 7-9 done; Week 10-11 runs bench
- `docs/tech-stack.md` § "Testing stack" — criterion selection rationale

---

## Notes for future contributors

Adding a new bench:

1. First confirm there's a **real hot path** (don't bench imagined scenarios)
2. Use `black_box()` to prevent the compiler from optimizing it away
3. Run 3 times, take the median (criterion does this; don't do it manually)
4. In the commit message, write clearly "why this bench"

If a bench is slow:

1. First look at `target/criterion/<name>/report/` violin plot
2. Check whether samples are bimodal (GC / scheduling jitter) — add warmup
3. Look at flamegraph (`cargo flamegraph --bench core`) — find hotspots
4. Don't rush to optimize; confirm it's a hot path (not a cold path)
