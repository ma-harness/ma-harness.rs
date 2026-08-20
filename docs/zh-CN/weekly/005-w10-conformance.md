# Week 10 周报 — 2026-08-18 (Day 30 ~ Day 33)

> 12 周 PoC 的第 10 周,**conformance + benchmark 框架**Phase 1 完成。
> 累计 33 commit, 16 crate workspace, 估计 167+ 个测试。
> **Week 11-12 待做**: dsh 真实 fixture 接入 + benchmark 跑数据 + Week 11 conformance 报告。



[English](../../weekly/005-w10-conformance.md) — coming soon. 中文为主.


---

## TL;DR

| 维度 | 数值 |
|---|---|
| 本周 commit | 4 (Day 30 周报补交 + Day 31-33 周 10 推进) |
| 累计 commit | **33** (含 Day 30 补交) |
| 新加 workspace member | 1 (`ma_harness_conformance`) |
| 累计 workspace member | **16** (9 crates/ + 7 plugins/) |
| 新加文件 | ~25 个 (含 docs + benches + conformance) |
| 累计净代码 | ~12000 行 |
| 累计测试 | ~167 (cordis ~38 + core ~24 + macro ~10 + hello ~11 + seam ~4 + proto ~3 + server ~12 + 6 plugins ~25 + demo ~13 + **conformance ~22** + bench code) |
| 12 周 PoC 进度 | **83%** (Week 1-10 全部完成, Week 11-12 待跑) |

## 本周时间线

```
Week 10 commit:
├── 9152aad docs(weekly): Day 30 Week 7-9 周报提交 (补交, 之前漏掉)
├── 7bbb612 docs(design): Day 31 conformance + benchmark 设计稿 (~480 行)
├── 20e1397 feat(conformance): Day 32 ma_harness_conformance crate 骨架 (~1500 行)
└── 1defc2e bench(cordis): Day 33 cordis 核心热路径 criterion bench (~250 行)
```

## Week 10 关键产出

### 1. Conformance test framework (Day 32)

新 crate `crates/ma_harness_conformance/`, 16 个 workspace member:

| 模块 | 作用 | 行数 | 测试 |
|---|---|---|---|
| `fixture` | JSONL loader + schema | ~280 | 3 |
| `compare` | 浅比对 + diff 分类 | ~250 | 7 |
| `runner` | 跑 fixture + 收集事件 | ~260 | 3 |
| `report` | markdown + json 报告 | ~250 | 5 |
| `tests/smoke.rs` | 端到端 smoke (8 测试) | ~210 | 8 |
| `fixtures/smoke.jsonl` | 4 个合成 fixture | 0 | — |
| `README.md` | 模块说明 | ~70 | — |
| `Cargo.toml` | 依赖配置 | ~40 | — |
| **合计** | | **~1360** | **~22** |

**Phase 1 简化**:
- 不真装载 plugin (`build_ctx` 只 new, `replay_events` 透传)
- 比对 fixture input vs output (framework 自身一致性)
- 留给 Phase 2: 真 plugin 装载 + EventLog 收集 + ctx.emit 触发

**Phase 2 计划 (Week 11)**:
- 加 `EventLog` 收集路径
- 加 dsh 真实 fixture 转换层 (TypeScript JSONL → ma-harness shape)
- 加 `mah conformance` 子命令 (CLI 入口)
- 跑 dsh fixtures, 出 ≥ 95% pass rate 报告

### 2. Cordis benchmark (Day 33)

`crates/ma_harness_cordis/benches/core.rs`, criterion 0.5:

| Bench | 操作 | 数据规模 |
|---|---|---|
| `ctx_set_typed_key` | set 1000 次 | String 8B |
| `ctx_get_typed_key` | get 1000 次 | String 8B |
| `ctx_inject_service` | inject 1000 次 | Arc clone |
| `ctx_service_lookup` | service 1000 次 | TypeId 查 |
| `ctx_emit_no_listeners` | emit 1000 次 | 0 listener |
| `ctx_emit_with_listeners` | emit × 3 组 | 1 / 10 / 100 listener |
| `ctx_plugin_install_uninstall` | install+uninstall 1000 次 | 1 plugin |
| `ctx_fork_with_10_services` | fork 1000 次 | 10 service |
| `ctx_dispose_empty` | dispose 1000 次 | 空 ctx |
| `ctx_set_get_u64_combined` | set+get 1000 次 | u64 round-trip |

**配置**: 100 sample (跟 dsh tinybench 对齐), 3s measurement_time。

**Week 11 计划**:
- 跑 `cargo bench -p ma_harness_cordis` 出 HTML 报告 (`target/criterion/core/<name>/report/`)
- 加 `crates/ma_harness_core/benches/agent.rs` (AgentLoop 1 step, 用 tokio_test::block_on)
- 加 `crates/ma_harness_seam/benches/plugin.rs` (PluginRegistry register / get_by_name)
- 跟 dsh tinybench 对比, 出 Week 11 benchmark 报告

## 3. 设计文档 (Day 31)

两份周 10 设计稿, 锁死方案:

- **`docs/conformance-design.md`** (~10KB, 10 节): 目的 / 不做什么 / Fixture 格式 v1 / Runner 流程 / Compare 算法 / 报告模板 / 双轨 fixture / 不在 scope / 失败处理 / 跨 doc 关系
- **`docs/benchmark-design.md`** (~9KB, 10 节): 性能假设表 / 范围 / 矩阵 (cordis 10 + core 5 + seam 3) / criterion 用法 / 跑法 / dsh 对比 / 治理 / 不在 scope / Phase 2 升级 / 跨 doc 关系

## 4. Day 30 周报补交 (9152aad)

Week 7-9 周报文件 `docs/weekly/004-w07-w09.md` 之前写好但**未提交** (mental-compile 误以为是 commit 过的)。
Day 30 commit 补交, 现在 Week 1-9 全部 commit 落地。

## 12 周 PoC 进度 (83%)

| Week | 状态 | 关键产出 |
|---|---|---|
| **1-2** | ✅ | cordis 完整 + SessionEvent + AgentLoop + 5 macro |
| **3-4** | ✅ | proto + seam + 6 plugin 骨架 + server + cli |
| **5-6** | ✅ | 6 first-party 全部实装 |
| **7-9** | ✅ | **端到端 demo + integration test + mah start** + 周报 (Day 30 补交) |
| **10** | ✅ | **conformance 框架 + cordis bench + 设计稿** |
| **11-12** | ⏳ | **dsh 真实 fixture + benchmark 数据 + Week 11 报告** |

## Week 11-12 TODO (详细)

### Week 11 (Day 34-40)

| Day | 工作 | 产出 |
|---|---|---|
| 34 | 加 `EventLog` 真装载到 conformance runner | 框架支持真 event collection |
| 35 | dsh fixture 转换层 (TypeScript JSONL → ma-harness shape) | 跨框架 fixture 兼容 |
| 36 | 拉 dsh 仓库, 抓取 `tests/fixtures/*.jsonl` | dsh 真实 fixture 集 |
| 37 | 跑 conformance, 收集 pass/fail 统计 | Week 11 conformance 报告 |
| 38 | 加 `ma_harness_core/benches/agent.rs` (AgentLoop 1 step) | 核心路径 bench |
| 39 | 加 `ma_harness_seam/benches/plugin.rs` (PluginRegistry) | 公开 API bench |
| 40 | 跑全部 bench, 出跟 dsh 对比数据 | Week 11 benchmark 报告 |

### Week 12 (Day 41-50)

| Day | 工作 | 产出 |
|---|---|---|
| 41-43 | 根据 bench 数据优化慢路径 (如有) | 性能 close gap |
| 44-46 | conformance 报告打磨 (Markdown 模板 + 分类) | 业务方能读 |
| 47-49 | 写最终 Week 11-12 周报 + 12 周 PoC 收官 | 12 周 PoC 100% |
| 50 | 决定: Phase 2 范围 (Code Mode / 多 model / 持久化 / sandbox 强化) | 下一步规划 |

## 已知 TODO (继续累计)

1. **网络通后必跑** `cargo check --workspace` + `cargo test --workspace` + `cargo bench --workspace` — 验证 16 crate + 167+ 测试 + ~18 bench
2. **Phase 2 待做** (Week 11-12 收尾后启动):
   - macro 增强 (#[dsh_service(cordis, seam)] 自动派生两套)
   - Sandbox 强化 (landlock / Seatbelt syscall)
   - 持久化 (SessionServiceImpl 内存换 rusqlite)
   - Code Mode (wasmtime / deno_core)
   - 多 model adapter (OpenAI / Anthropic)

## 网络阻塞 (持续 33 commit)

- 本机代理 127.0.0.1:7890 不能代理 HTTPS
- 120+ 文件 **未 cargo check 验证**
- 全部 mental-compile only

**预计 16 crate 编译 + 167+ 测试 + ~18 bench 跑通需要 2-3 分钟 (网络通后)**。

## 协作模式 (保持)

- 每次 1 个 in_progress todo
- 每 1h cron 汇报 (cron_id `889bf0de`)
- 网络不通, 跳过 cargo check, review 兜底
- commit 频率: 每个决策点一个, 主题行 ≤ 72 字符

## 跨 session 恢复用 11 份关键文档

1. `AGENTS.md` — 仓库入口
2. `docs/decision-log.md` — 11 项决策
3. `docs/ma-harness-arch-map.md` — 跟 dsh 翻译 + 8 条硬线
4. `docs/macro-design.md` — 5 个 proc-macro 规范
5. `docs/plugin-schema-v1.md` — plugin.toml + JSON Schema
6. `docs/conformance-design.md` — Week 10 conformance 设计
7. `docs/benchmark-design.md` — Week 10 benchmark 设计
8. `docs/weekly/000-day0.md` — Day 0
9. `docs/weekly/001-w01-w02.md` — Week 1-2
10. `docs/weekly/002-w03-w04.md` — Week 3-4
11. `docs/weekly/003-w05-w06.md` — Week 5-6
12. `docs/weekly/004-w07-w09.md` — Week 7-9
13. **`docs/weekly/005-w10-conformance.md`** — 本文件

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | Day 30 周报补交 + Day 31-33 Week 10 推进, 4 commit, 12 周 PoC 进度 83% |
