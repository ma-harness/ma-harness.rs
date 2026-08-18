//! Core bench: ma_harness_seam PluginRegistry + plugin lifecycle 性能 baseline.
//!
//! 跑法: `cargo bench -p ma_harness_seam --bench plugin`
//!
//! 设计: 见 `docs/benchmark-design.md` § 3.3.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ma_harness_cordis::{Context, Plugin as CordisPlugin};
use ma_harness_seam::{Plugin, PluginRegistry};

/// 测试用 seam plugin
struct BenchPlugin {
    name: String,
}

impl Plugin for BenchPlugin {
    fn install(&self, _ctx: &ma_harness_cordis::Context) -> anyhow::Result<()> {
        Ok(())
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn uninstall(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// 测试用 cordis plugin (seam plugin 不直接给 ctx, 用 cordis 版的)
struct CordisBenchPlugin(String);

impl CordisPlugin for CordisBenchPlugin {
    fn install(&self, _ctx: &Context) -> anyhow::Result<()> {
        Ok(())
    }
    fn name(&self) -> &str {
        &self.0
    }
}

// ============================================================================
// Bench: PluginRegistry
// ============================================================================

fn bench_plugin_registry_register(c: &mut Criterion) {
    c.bench_function("plugin_registry_register_1000", |b| {
        b.iter(|| {
            let mut reg = PluginRegistry::new();
            for _ in 0..1000 {
                let name = format!("bench_plugin_{}", uuid::Uuid::new_v4());
                let p = BenchPlugin { name };
                let _ = black_box(reg.register(p));
            }
        });
    });
}

fn bench_plugin_registry_list(c: &mut Criterion) {
    let mut reg = PluginRegistry::new();
    for i in 0..100 {
        let name = format!("plugin_{i:04}");
        let p = BenchPlugin { name };
        reg.register(p).expect("register");
    }
    c.bench_function("plugin_registry_list_100", |b| {
        b.iter(|| {
            let _ = black_box(reg.list());
        });
    });
}

fn bench_ctx_plugin_by_name(c: &mut Criterion) {
    let ctx = Context::new();
    for i in 0..100 {
        let name = format!("ctx_plugin_{i:04}");
        let p = CordisBenchPlugin(name);
        ctx.plugin(p).expect("ctx.plugin");
    }
    c.bench_function("ctx_plugin_by_name_100", |b| {
        b.iter(|| {
            let _ = black_box(ctx.plugin_by_name("ctx_plugin_0050"));
        });
    });
}

fn bench_ctx_plugins_list(c: &mut Criterion) {
    let ctx = Context::new();
    for i in 0..100 {
        let name = format!("list_plugin_{i:04}");
        let p = CordisBenchPlugin(name);
        ctx.plugin(p).expect("ctx.plugin");
    }
    c.bench_function("ctx_plugins_list_100", |b| {
        b.iter(|| {
            let _ = black_box(ctx.plugins());
        });
    });
}

// ============================================================================
// Group + Main
// ============================================================================

criterion_group!(
    name = seam_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(3));
    targets =
        bench_plugin_registry_register,
        bench_plugin_registry_list,
        bench_ctx_plugin_by_name,
        bench_ctx_plugins_list,
);

criterion_main!(seam_benches);
