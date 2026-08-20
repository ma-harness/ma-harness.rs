//! Core bench: ma_harness_cordis 核心热路径性能 baseline.
//!
//! 跑法: `cargo bench -p ma_harness_cordis --bench core`
//! 输出: `target/criterion/core/<bench_name>/report/index.html`
//!
//! 设计: 见 `docs/benchmark-design.md` § 3.1。

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ma_harness_cordis::{Context, CtxKey, Listener, ListenerEvent, Service};
use std::sync::Arc;

// ============================================================================
// Test types
// ============================================================================

/// 测试 typed key
static BENCH_KEY: CtxKey<String> = CtxKey::new_unchecked("bench_key");
static BENCH_KEY_INT: CtxKey<u64> = CtxKey::new_unchecked("bench_key_int");

/// 测试用 service
struct BenchService;

impl Service for BenchService {
    type Ctx = Context;
    type Error = std::convert::Infallible;
    fn install(_ctx: &Context) -> Result<Self, Self::Error> {
        Ok(BenchService)
    }
    fn name(&self) -> &str {
        "bench_service"
    }
}

/// 测试用 event
#[derive(Clone)]
struct BenchEvent {
    payload: u64,
}

impl ListenerEvent for BenchEvent {}

/// 测试用 listener
struct BenchListener {
    counter: std::sync::atomic::AtomicU64,
}

impl BenchListener {
    fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl Listener<BenchEvent> for BenchListener {
    fn handle(&self, _ctx: &Context, _event: &BenchEvent) {
        self.counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 测试用 plugin
struct BenchPlugin {
    name: String,
}

impl ma_harness_cordis::Plugin for BenchPlugin {
    fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
        Ok(())
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn uninstall(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// ============================================================================
// Bench: typed key
// ============================================================================

fn bench_ctx_set_typed_key(c: &mut Criterion) {
    let ctx = Context::new();
    c.bench_function("ctx_set_typed_key", |b| {
        b.iter(|| {
            ctx.set(BENCH_KEY, black_box("hello".to_string()));
        });
    });
}

fn bench_ctx_get_typed_key(c: &mut Criterion) {
    let ctx = Context::new();
    ctx.set(BENCH_KEY, "hello".to_string());
    c.bench_function("ctx_get_typed_key", |b| {
        b.iter(|| {
            let _ = black_box(ctx.get(BENCH_KEY));
        });
    });
}

// ============================================================================
// Bench: service
// ============================================================================

fn bench_ctx_inject_service(c: &mut Criterion) {
    let ctx = Context::new();
    let svc = Arc::new(BenchService);
    c.bench_function("ctx_inject_service", |b| {
        b.iter(|| {
            ctx.inject(black_box(svc.clone()));
        });
    });
}

fn bench_ctx_service_lookup(c: &mut Criterion) {
    let ctx = Context::new();
    ctx.inject::<BenchService>(Arc::new(BenchService));
    c.bench_function("ctx_service_lookup", |b| {
        b.iter(|| {
            let _ = black_box(ctx.service::<BenchService>());
        });
    });
}

// ============================================================================
// Bench: emit
// ============================================================================

fn bench_ctx_emit_no_listeners(c: &mut Criterion) {
    let ctx = Context::new();
    let event = BenchEvent { payload: 42 };
    c.bench_function("ctx_emit_no_listeners", |b| {
        b.iter(|| {
            ctx.emit(black_box(event.clone()));
        });
    });
}

fn bench_ctx_emit_with_n_listeners(c: &mut Criterion) {
    let mut group = c.benchmark_group("ctx_emit_with_listeners");
    for n in [1usize, 10, 100] {
        let ctx = Context::new();
        for _ in 0..n {
            ctx.on(Arc::new(BenchListener::new()));
        }
        let event = BenchEvent { payload: 42 };
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                ctx.emit(black_box(event.clone()));
            });
        });
    }
    group.finish();
}

// ============================================================================
// Bench: plugin
// ============================================================================

fn bench_ctx_plugin_install_uninstall(c: &mut Criterion) {
    let ctx = Context::new();
    // 先装一个, 保证 register 不冲突 (plugin 用 unique name)
    c.bench_function("ctx_plugin_install_uninstall", |b| {
        b.iter(|| {
            let name = format!("bench_plugin_{}", uuid::Uuid::new_v4());
            let plugin = BenchPlugin { name };
            ctx.plugin(black_box(plugin)).unwrap();
            let name = ctx.plugins().last().unwrap().clone();
            ctx.uninstall_plugin(&name).unwrap();
        });
    });
}

// ============================================================================
// Bench: fork
// ============================================================================

fn bench_ctx_fork_with_10_services(c: &mut Criterion) {
    let ctx = Context::new();
    for _ in 0..10 {
        ctx.inject::<BenchService>(Arc::new(BenchService));
    }
    c.bench_function("ctx_fork_with_10_services", |b| {
        b.iter(|| {
            let _ = black_box(ctx.fork());
        });
    });
}

fn bench_ctx_dispose(c: &mut Criterion) {
    c.bench_function("ctx_dispose_empty", |b| {
        b.iter(|| {
            let ctx = Context::new();
            black_box(ctx.dispose()).unwrap();
        });
    });
}

// ============================================================================
// Bench: typed key u64 (small value)
// ============================================================================

fn bench_ctx_set_get_u64(c: &mut Criterion) {
    let ctx = Context::new();
    let mut counter = 0u64;
    c.bench_function("ctx_set_get_u64_combined", |b| {
        b.iter(|| {
            counter = counter.wrapping_add(1);
            ctx.set(BENCH_KEY_INT, counter);
            let _ = ctx.get(BENCH_KEY_INT);
        });
    });
}

// ============================================================================
// Group + Main
// ============================================================================

criterion_group!(
    name = cordis_benches;
    config = Criterion::default()
        .sample_size(100)        // 100 sample, 跟 dsh bench 对齐
        .measurement_time(std::time::Duration::from_secs(3));
    targets =
        bench_ctx_set_typed_key,
        bench_ctx_get_typed_key,
        bench_ctx_inject_service,
        bench_ctx_service_lookup,
        bench_ctx_emit_no_listeners,
        bench_ctx_emit_with_n_listeners,
        bench_ctx_plugin_install_uninstall,
        bench_ctx_fork_with_10_services,
        bench_ctx_dispose,
        bench_ctx_set_get_u64,
);

criterion_main!(cordis_benches);
