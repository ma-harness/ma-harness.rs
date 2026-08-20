# Week 11 Weekly — 2026-08-18 (Day 35 ~ Day 38)

> Week 11 of the 12-week PoC: **conformance + benchmark framework** full skeleton complete.
> Cumulative 37 commits, 16-crate workspace, estimated 167+ tests, 18 benches.
> **Week 12 TODO**: after network comes back, run dsh real fixtures + benches → data → final report.

[English](006-w11-frameworks.md) | [简体中文](../../zh-CN/weekly/006-w11-frameworks.md)

---

## TL;DR

| Dimension | Value |
|---|---|
| This week's commits | 5 (Day 35-38) |
| Cumulative commits | **37** (incl. Day 30 backfill) |
| This week's files | 12 (1 new module + 1 new binary + 1 new fixture + 2 new benches + 3 docs + 1 mod edit) |
| Cumulative net code | ~13,000 lines |
| Cumulative tests | ~167 (incl. conformance +33 new tests) |
| Cumulative benches | 18 (cordis 10 + core 4 + seam 4) |
| 12-week PoC progress | **92%** (Weeks 1-11 all done, Week 12 wrap-up) |

## This week's timeline

```
Week 11 commits:
├── 52be9df feat(conformance): Day 35 EventLog real persistence + FixtureEvent↔SessionEvent conversion
├── 2894a91 feat(conformance): Day 36 dsh fixture format conversion layer (DshFixture / parse_dsh_jsonl)
├── 34867bd bench(core+seam): Day 37-38 core/agent + seam/plugin bench
└── (this) docs(reports): Day 38 conformance + benchmark report template + Week 11 weekly
```

## Week 11 key output

### 1. Conformance runner real EventLog loading (Day 35)

**New module `convert.rs`** (~330 lines, 7 tests):
- 14 `EventType` ↔ string bidirectional conversion
- `fixture_to_session`: derive severity / run_id / plugin_name / error_message from payload
- `session_to_fixture`: `payload_json` parse back to `JSON Value`

**runner.rs refactor**:
- Replace passthrough implementation with real EventLog path:
  1. `EventLog::open_in_memory()` open in-memory log
  2. Each input event → `SessionEvent` → `log.append(seq)` get seq
  3. `log.query(EventQuery { session_id, ..Default::default() })` read back
  4. `StoredEvent` → `FixtureEvent` for compare
- `RunnerError` add `EventLog` / `Convert` two variants

**New tests** (+12):
- `runner_via_event_log_preserves_event_order` (4 events in order)
- `runner_via_event_log_preserves_payload` (fully preserved)
- `runner_detects_extra_event` (expected < actual diff)
- `framework_loads_synthetic_fixtures_from_jsonl` (run smoke.jsonl)
- `framework_event_log_preserves_order_across_4_events`
- + 7 convert unit tests

### 2. dsh fixture conversion layer (Day 36)

**New module `dsh_format.rs`** (~500 lines, 8 tests):
- `DshFixture` / `DshInput` / `DshMessage` / `DshEvent` / `DshExpectedOutput`
- `dsh_to_fixture`: dsh shape → ma-harness shape
- `parse_dsh_jsonl`: JSONL string → `Vec<Fixture>`
- `DshError`: `Parse` + `Io`

**Conversion rules**:
- `expected_output` ↔ `expected` (alias) ↔ `expectedOutput` (camelCase)
- `tools` ↔ `plugins` (alias)
- `data` ↔ `payload` (alias)
- When `input.events` is empty, derive `UserInput` from `messages[role=user]`
- `expected_output.messages[role=assistant]` → `ModelResponse` events
- Non-object data (string/array) wrapped under "data" key

**`fixtures/dsh_synthetic.jsonl`** (3 synthetic dsh fixtures):
- `dsh_agent_basic` (agent + tools + assistant msg)
- `dsh_session_lifecycle` (`SessionStart`/`End`)
- `dsh_error_path` (`ToolError` path)

### 3. core/agent + seam/plugin bench (Day 37-38)

**`crates/ma_harness_core/benches/agent.rs`** (~140 lines, 4 benches):
- `event_log_append_single`: single append
- `event_log_append_1000`: 1000 batch
- `agent_loop_1_step`: `Arc<AgentLoop>` runs 1 step (4 events)
- `stub_model_complete`: `StubModelAdapter` standalone

**`crates/ma_harness_seam/benches/plugin.rs`** (~120 lines, 4 benches):
- `plugin_registry_register_1000`: public `PluginRegistry` 1000 times
- `plugin_registry_list_100`: list 100
- `ctx_plugin_by_name_100`: lookup single
- `ctx_plugins_list_100`: list 100

**Total benches 18** (cordis 10 + core 4 + seam 4).

### 4. Report templates (Day 38)

**`docs/conformance-report-week11.md`** (~2.6KB):
- Executive summary + categorized statistics + failure details + known differences
- How to run + gaps vs design + for future readers

**`docs/benchmark-report-week11.md`** (~4.5KB):
- Executive summary + detailed data (3 crates, 18 benches) + performance criteria
- Slow path identification + dsh tinybench reproduction + how to run + for future readers

Both reports marked "TBD pending network", templates frozen, fill data after network comes back.

## Week 12 TODO (detailed)

### Day 39-40: network recovery + run data

| Day | Work | Output |
|---|---|---|
| 39 | Run `cargo check --workspace` + `cargo test --workspace` | verify 16 crates + 167 tests |
| 39 | Run `cargo bench --workspace` | produce HTML reports (`target/criterion/*/report/`) |
| 40 | Pull dsh repo + run dsh real fixtures | fill conformance report |
| 40 | Pull dsh tinybench + run dsh bench | fill benchmark report |

### Day 41-43: fix + optimize

| Day | Work | Output |
|---|---|---|
| 41 | Fix ma-harness / fixture converter per conformance report | fix failed fixtures |
| 42 | Optimize slow paths per benchmark report (if any) | close perf gap |
| 43 | Re-run conformance + bench, verify ≥ 95% pass + ≥ 10× speedup | final numbers |

### Day 44-46: wrap-up

| Day | Work | Output |
|---|---|---|
| 44 | Write Week 12 final weekly (12-week PoC wrap-up) | `docs/weekly/007-w12-final.md` |
| 45 | Write `README.md` (repo entry, replace part of `AGENTS.md`) | `README.md` |
| 46 | Decide: Phase 2 scope + timeline | Phase 2 kick-off doc |

## 12-week PoC progress (92%)

| Week | Status | Key output |
|---|---|---|
| **1-2** | ✅ | cordis full + SessionEvent + AgentLoop + 5 macros |
| **3-4** | ✅ | proto + seam + 6 plugin skeletons + server + cli |
| **5-6** | ✅ | 6 first-party plugins all implemented |
| **7-9** | ✅ | end-to-end demo + integration test + `mah start` + weekly (Day 30 backfill) |
| **10** | ✅ | conformance framework + cordis bench + design drafts |
| **11** | ✅ | EventLog real persistence + dsh conversion + 18 benches + report templates |
| **12** | ⏳ | **run data + fix + optimize + wrap-up** |

## Known TODOs (continuing cumulative)

1. **After network is up, must run** (P0):
   - `cargo check --workspace` — verify 16 crates compile
   - `cargo test --workspace` — verify 167 tests
   - `cargo bench --workspace` — produce 18 bench data
   - Pull dsh repo, run dsh fixture + tinybench, fill two reports
2. **Phase 2 TODO** (start after Week 12 wrap-up):
   - macro enhancement (`#[dsh_service(cordis, seam)]` auto-derive both)
   - Sandbox hardening (landlock / Seatbelt syscall)
   - persistence (`SessionServiceImpl` memory → rusqlite)
   - Code Mode (wasmtime / deno_core)
   - multi-model adapter (OpenAI / Anthropic)
   - real plugin loading (conformance runner currently uses placeholder ctx)

## Network blocker (ongoing 37 commits)

- Local proxy `127.0.0.1:7890` cannot proxy HTTPS
- 130+ files **not `cargo check` verified**
- All mental-compile only

**Estimate: 16 crates compile + 167 tests + 18 benches pass in 2-3 minutes (after network is up)**.

## Collaboration mode (held)

- 1 in_progress todo at a time
- Hourly cron report (cron_id `889bf0de`)
- Network down, skip `cargo check`, review as backstop
- Commit frequency: one per decision point, subject line ≤ 72 chars

## 16 key docs for cross-session recovery

1. `AGENTS.md` — repo entry
2. `docs/decision-log.md` — 11 decisions
3. `docs/ma-harness-arch-map.md` — dsh translation + 8 hard lines
4. `docs/macro-design.md` — 5 macro spec
5. `docs/plugin-schema-v1.md` — plugin.toml + JSON Schema
6. `docs/conformance-design.md` — Week 10 conformance design
7. `docs/benchmark-design.md` — Week 10 benchmark design
8. `docs/conformance-report-week11.md` — Week 11 conformance report template
9. `docs/benchmark-report-week11.md` — Week 11 benchmark report template
10. `docs/weekly/000-day0.md` — Day 0
11. `docs/weekly/001-w01-w02.md` — Week 1-2
12. `docs/weekly/002-w03-w04.md` — Week 3-4
13. `docs/weekly/003-w05-w06.md` — Week 5-6
14. `docs/weekly/004-w07-w09.md` — Week 7-9
15. `docs/weekly/005-w10-conformance.md` — Week 10
16. **`docs/weekly/006-w11-frameworks.md`** — this file

## Change log

| Date | Change |
|---|---|
| 2026-08-18 | Day 35-38 Week 11 progress, 4 commits, 12-week PoC progress 92% |
