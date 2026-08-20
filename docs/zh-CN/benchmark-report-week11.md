# Benchmark Report — Week 11 (2026-08-18)

> **状态**: 模板, 待网络通后跑 `cargo bench --workspace` 填充.
> 关联: [`benchmark-design.md`](./benchmark-design.md) § 6 跑法.



[English](../benchmark-report-week11.md) — coming soon. 中文为主.


---

## 1. 执行摘要 (待填)

- **总 bench 数**: 18 (cordis 10 + core 4 + seam 4)
- **dsh 对比数据**: TBD (需要拉 dsh 仓库 + 跑 tinybench)
- **几何平均加速比**: TBD (目标 ≥ 10x)
- **退化 bench**: 0 (任何慢过 dsh 立即 P0 优化)

## 2. 详细数据 (待填)

### 2.1 ma_harness_cordis

| Bench | ma-harness (Rust) | dsh (TypeScript) | 倍数 | 状态 |
|---|---|---|---|---|
| ctx_set_typed_key | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_get_typed_key | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_inject_service | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_service_lookup | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_emit_no_listeners | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_emit_with_1_listener | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_emit_with_10_listeners | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_emit_with_100_listeners | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_plugin_install_uninstall | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_fork_with_10_services | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_dispose_empty | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_set_get_u64_combined | TBD ns | TBD ns | TBD x | 🟡 待跑 |

### 2.2 ma_harness_core

| Bench | ma-harness (Rust) | dsh (TypeScript) | 倍数 | 状态 |
|---|---|---|---|---|
| event_log_append_single | TBD µs | TBD µs | TBD x | 🟡 待跑 |
| event_log_append_1000 | TBD µs | TBD µs | TBD x | 🟡 待跑 |
| agent_loop_1_step | TBD µs | TBD µs | TBD x | 🟡 待跑 |
| stub_model_complete | TBD ns | TBD ns | TBD x | 🟡 待跑 |

### 2.3 ma_harness_seam

| Bench | ma-harness (Rust) | dsh (TypeScript) | 倍数 | 状态 |
|---|---|---|---|---|
| plugin_registry_register_1000 | TBD µs | TBD µs | TBD x | 🟡 待跑 |
| plugin_registry_list_100 | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_plugin_by_name_100 | TBD ns | TBD ns | TBD x | 🟡 待跑 |
| ctx_plugins_list_100 | TBD ns | TBD ns | TBD x | 🟡 待跑 |

## 3. 性能判据 (设计稿)

| 判据 | 阈值 | 实际 | 状态 |
|---|---|---|---|
| 几何平均加速比 | ≥ 10x | TBD | 🟡 待跑 |
| 任何 bench 慢过 dsh | 0 | TBD | 🟡 待跑 |
| 慢过 dsh 30% 的 bench | 0 | TBD | 🟡 待跑 |

## 4. 慢路径识别 (待跑后填)

如果几何平均 < 10x, 找慢路径:
1. 跑 `cargo flamegraph -p ma_harness_cordis --bench core` 出火焰图
2. 看 hot function (是 Arc clone / HashMap lookup / 还是别的)
3. 优化方案:
   - Arc clone 多次 → 一次性传递
   - HashMap lookup 慢 → 考虑 `hashbrown` 默认 / `FxHashMap` 改
   - listener dispatch 慢 → 直接 fn ptr 数组, 不用 AnyListener

## 5. dsh tinybench 复刻 (待跑)

```typescript
// dsh 仓库 benches/micro.bench.ts
import { Bench } from 'tinybench';
import { Context } from '@deepkit/harness';

const bench = new Bench({ iterations: 100 });

const ctx = new Context();

bench.add('ctx_set_typed_key', () => {
  ctx.set('my_key', 'hello');
});

bench.add('ctx_service_lookup', () => {
  ctx.service('MyService');
});

await bench.run();
console.table(bench.table());
```

## 6. 跑法 (供 reviewer 复现)

```bash
# 跑全部 bench (~2-3 分钟)
cargo bench --workspace

# 单个 crate
cargo bench -p ma_harness_cordis
cargo bench -p ma_harness_core
cargo bench -p ma_harness_seam

# 短跑 (开发期, 30s 内)
cargo bench -p ma_harness_cordis -- --quick

# 出 HTML 报告
# 报告在 target/criterion/<bench_name>/report/index.html
```

## 7. 已知 TODO (网络通后)

- [ ] 跑 `cargo bench --workspace` 出 baseline
- [ ] 拉 dsh 仓库, 复刻 12 个 micro-bench 用 tinybench
- [ ] 跑 dsh tinybench, 出对比数据
- [ ] 出最终 benchmark 报告 (替换本文件 TBD)
- [ ] 如果慢过 dsh, 进 P0 优化 backlog

---

## 给后来人

跑 bench 注意事项:
1. **单线程跑** (`-j 1`): 多核并行会让数字不稳定
2. **关掉后台程序**: Chrome / IDE 都会让数字飘
3. **release 模式**: 必须是 release, debug 数字没参考价值
4. **看 P50 不是 mean**: criterion 默认报 P50 (中位数), 抗噪声

写新 bench:
1. 注释**为什么这个 bench 重要** (e.g. `// 高频路径, plugin 配置读`)
2. 用 `black_box()` 防止编译器优化
3. 不要在 bench 内 `println!`, 影响 timing
4. 跑 3 次取稳定那次 (criterion 自己会做)
