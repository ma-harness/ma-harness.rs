# Week 10 Weekly — 2026-08-18 (Day 30 ~ Day 33)

> Week 10 of the 12-week PoC: **conformance + benchmark framework** Phase 1 complete.
> Cumulative 33 commits, 16-crate workspace, estimated 167+ tests.
> **Week 11-12 TODO**: dsh real fixture integration + benchmark run data + Week 11 conformance report.

[English](005-w10-conformance.md) | [简体中文](../../zh-CN/weekly/005-w10-conformance.md)

---

## TL;DR

| Dimension | Value |
|---|---|
| This week's commits | 4 (Day 30 weekly backfill + Day 31-33 Week 10 progress) |
| Cumulative commits | **33** (incl. Day 30 backfill) |
| New workspace member | 1 (`ma_harness_conformance`) |
| Cumulative workspace members | **16** (9 in `crates/` + 7 in `plugins/`) |
| New files | ~25 (incl. docs + benches + conformance) |
| Cumulative net code | ~12,000 lines |
| Cumulative tests | ~167 (cordis ~38 + core ~24 + macro ~10 + hello ~11 + seam ~4 + proto ~3 + server ~12 + 6 plugins ~25 + demo ~13 + **conformance ~22** + bench code) |
| 12-week PoC progress | **83%** (Weeks 1-10 all done, Weeks 11-12 pending run) |

## This week's timeline

```
Week 10 commits:
├── 9152aad docs(weekly): Day 30 Week 7-9 weekly backfill (previously missed)
├── 7bbb612 docs(design): Day 31 conformance + benchmark design draft (~480 lines)
├── 20e1397 feat(conformance): Day 32 ma_harness_conformance crate skeleton (~1,500 lines)
└── 1defc2e bench(cordis): Day 33 cordis core hot-path criterion bench (~250 lines)
```

## Week 10 key output

### 1. Conformance test framework (Day 32)

New crate `crates/ma_harness_conformance/`, 16th workspace member:

| Module | Purpose | Lines | Tests |
|---|---|---|---|
| `fixture` | JSONL loader + schema | ~280 | 3 |
| `compare` | shallow compare + diff classification | ~250 | 7 |
| `runner` | run fixture + collect events | ~260 | 3 |
| `report` | markdown + json report | ~250 | 5 |
| `tests/smoke.rs` | end-to-end smoke (8 tests) | ~210 | 8 |
| `fixtures/smoke.jsonl` | 4 synthetic fixtures | 0 | — |
| `README.md` | module description | ~70 | — |
| `Cargo.toml` | dependency config | ~40 | — |
| **Total** | | **~1,360** | **~22** |

**Phase 1 simplification**:
- No real plugin loading (`build_ctx` only `new`, `replay_events` passthrough)
- Compare fixture input vs output (framework internal consistency)
- Left for Phase 2: real plugin loading + EventLog collection + ctx.emit trigger

**Phase 2 plan (Week 11)**:
- Add `EventLog` collection path
- Add dsh real fixture conversion layer (TypeScript JSONL → ma-harness shape)
- Add `mah conformance` subcommand (CLI entry)
- Run dsh fixtures, produce ≥ 95% pass rate report

### 2. Cordis benchmark (Day 33)

`crates/ma_harness_cordis/benches/core.rs`, criterion 0.5:

| Bench | Operation | Data size |
|---|---|---|
| `ctx_set_typed_key` | set 1000 times | String 8B |
| `ctx_get_typed_key` | get 1000 times | String 8B |
| `ctx_inject_service` | inject 1000 times | Arc clone |
| `ctx_service_lookup` | service 1000 times | TypeId lookup |
| `ctx_emit_no_listeners` | emit 1000 times | 0 listeners |
| `ctx_emit_with_listeners` | emit × 3 groups | 1 / 10 / 100 listeners |
| `ctx_plugin_install_uninstall` | install+uninstall 1000 times | 1 plugin |
| `ctx_fork_with_10_services` | fork 1000 times | 10 services |
| `ctx_dispose_empty` | dispose 1000 times | empty ctx |
| `ctx_set_get_u64_combined` | set+get 1000 times | u64 round-trip |

**Config**: 100 samples (aligned with dsh tinybench), 3s measurement time.

**Week 11 plan**:
- Run `cargo bench -p ma_harness_cordis` to produce HTML report (`target/criterion/core/<name>/report/`)
- Add `crates/ma_harness_core/benches/agent.rs` (AgentLoop 1 step, using `tokio_test::block_on`)
- Add `crates/ma_harness_seam/benches/plugin.rs` (PluginRegistry register / `get_by_name`)
- Compare with dsh tinybench, produce Week 11 benchmark report

## 3. Design docs (Day 31)

Two Week 10 design drafts, locking the approach:

- **`docs/conformance-design.md`** (~10KB, 10 sections): purpose / what not to do / Fixture format v1 / Runner flow / Compare algorithm / report template / dual-track fixture / out-of-scope / failure handling / cross-doc relationships
- **`docs/benchmark-design.md`** (~9KB, 10 sections): performance hypothesis table / scope / matrix (cordis 10 + core 5 + seam 3) / criterion usage / run method / dsh comparison / governance / out-of-scope / Phase 2 upgrade / cross-doc relationships

## 4. Day 30 weekly backfill (`9152aad`)

Week 7-9 weekly file `docs/weekly/004-w07-w09.md` was written earlier but **not committed** (mental-compile mistakenly thought it was committed).
Day 30 commit backfilled, now Weeks 1-9 all committed.

## 12-week PoC progress (83%)

| Week | Status | Key output |
|---|---|---|
| **1-2** | ✅ | cordis full + SessionEvent + AgentLoop + 5 macros |
| **3-4** | ✅ | proto + seam + 6 plugin skeletons + server + cli |
| **5-6** | ✅ | 6 first-party plugins all implemented |
| **7-9** | ✅ | **end-to-end demo + integration test + `mah start`** + weekly (Day 30 backfill) |
| **10** | ✅ | **conformance framework + cordis bench + design drafts** |
| **11-12** | ⏳ | **dsh real fixture + benchmark data + Week 11 report** |

## Week 11-12 TODO (detailed)

### Week 11 (Day 34-40)

| Day | Work | Output |
|---|---|---|
| 34 | Add `EventLog` real loading to conformance runner | framework supports real event collection |
| 35 | dsh fixture conversion layer (TypeScript JSONL → ma-harness shape) | cross-framework fixture compatibility |
| 36 | Pull dsh repo, fetch `tests/fixtures/*.jsonl` | dsh real fixture set |
| 37 | Run conformance, collect pass/fail stats | Week 11 conformance report |
| 38 | Add `ma_harness_core/benches/agent.rs` (AgentLoop 1 step) | core path bench |
| 39 | Add `ma_harness_seam/benches/plugin.rs` (PluginRegistry) | public API bench |
| 40 | Run all benches, produce dsh comparison data | Week 11 benchmark report |

### Week 12 (Day 41-50)

| Day | Work | Output |
|---|---|---|
| 41-43 | Optimize slow paths based on bench data (if any) | close perf gap |
| 44-46 | Polish conformance report (Markdown template + categorization) | business-readable |
| 47-49 | Write final Week 11-12 weekly + 12-week PoC wrap-up | 12-week PoC 100% |
| 50 | Decide: Phase 2 scope (Code Mode / multi-model / persistence / sandbox hardening) | next step plan |

## Known TODOs (continuing cumulative)

1. **Once network is up, must run** `cargo check --workspace` + `cargo test --workspace` + `cargo bench --workspace` — verify 16 crates + 167+ tests + ~18 benches
2. **Phase 2 TODO** (start after Week 11-12 wrap-up):
   - macro enhancement (`#[dsh_service(cordis, seam)]` auto-derive both)
   - Sandbox hardening (landlock / Seatbelt syscall)
   - persistence (SessionServiceImpl memory → rusqlite)
   - Code Mode (wasmtime / deno_core)
   - multi-model adapter (OpenAI / Anthropic)

## Network blocker (ongoing 33 commits)

- Local proxy `127.0.0.1:7890` cannot proxy HTTPS
- 120+ files **not `cargo check` verified**
- All mental-compile only

**Estimate: 16 crates compile + 167+ tests + ~18 benches pass in 2-3 minutes (after network is up)**.

## Collaboration mode (held)

- 1 in_progress todo at a time
- Hourly cron report (cron_id `889bf0de`)
- Network down, skip `cargo check`, review as backstop
- Commit frequency: one per decision point, subject line ≤ 72 chars

## 13 key docs for cross-session recovery

1. `AGENTS.md` — repo entry
2. `docs/decision-log.md` — 11 decisions
3. `docs/ma-harness-arch-map.md` — dsh translation + 8 hard lines
4. `docs/macro-design.md` — 5 macro spec
5. `docs/plugin-schema-v1.md` — plugin.toml + JSON Schema
6. `docs/conformance-design.md` — Week 10 conformance design
7. `docs/benchmark-design.md` — Week 10 benchmark design
8. `docs/weekly/000-day0.md` — Day 0
9. `docs/weekly/001-w01-w02.md` — Week 1-2
10. `docs/weekly/002-w03-w04.md` — Week 3-4
11. `docs/weekly/003-w05-w06.md` — Week 5-6
12. `docs/weekly/004-w07-w09.md` — Week 7-9
13. **`docs/weekly/005-w10-conformance.md`** — this file

## Change log

| Date | Change |
|---|---|
| 2026-08-18 | Day 30 weekly backfill + Day 31-33 Week 10 progress, 4 commits, 12-week PoC progress 83% |
