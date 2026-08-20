# ma_harness_conformance

[English](README.md) | [简体中文](README.zh-CN.md)

> Conformance test framework for ma-harness.
> Status: **P11+ complete** (framework + synthetic fixtures + dsh 9/9 + dsh_synthetic 7/7).
> Related: [`docs/conformance-design.md`](../../docs/conformance-design.md)

## Purpose

Verify that `ma-harness` produces semantically-equivalent output to DeepSeek
Harness (dsh) given the same trace input.
**Target**: pass rate **≥ 95%** (P11-2 / Week 11 metric).

## Modules

| Module    | Role |
|-----------|------|
| `fixture` | Fixture schema (JSONL) + loader |
| `runner`  | Run a fixture, collect actual events |
| `compare` | Compare actual vs expected, produce diffs |
| `report`  | Aggregate pass/fail, emit markdown + json |
| `dsh_format` (P11-1.5) | Convert dsh `session.jsonl` events → ma-harness `FixtureEvent` |
| `cache` (P12-1) | mtime-based `DshFixtureCache` for re-runs |

## How to run

```bash
# Run the framework's built-in synthetic fixtures (no dsh, no network)
cargo test -p ma_harness_conformance

# Run smoke test
cargo test -p ma_harness_conformance --test smoke

# Run dsh fixtures (synthetic, P11-1.5 7/7)
mah conformance --fixtures crates/ma-harness-conformance/fixtures/dsh_synthetic.jsonl --dsh

# Run real dsh acp-snapshot fixtures (P11-2 9/9)
DSH_FIXTURE_ROOT=${DSH_REPO}/packages/test-support/acp-snapshot/tests/fixtures \
    mah conformance --fixtures crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl --dsh
```

## Fixture format

One JSON per line; see `docs/conformance-design.md` § 3 for fields.

Minimal fixture:

```json
{
  "name": "my_test",
  "category": "tool_call",
  "input": {
    "session_id": "s1",
    "plugins": ["bash"],
    "events": [
      {"type": "ToolCall", "payload": {"tool": "bash"}}
    ]
  },
  "output": {
    "events": [
      {"type": "ToolCall", "payload_match": {"tool": "bash"}}
    ]
  }
}
```

For dsh-style fixtures, see `docs/dsh-benchmark-report.md` and
`crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl`
(9 real acp-snapshot fixtures).

## Reports

Auto-generated after a run:

- `target/conformance-report.md` — Markdown (human-readable)
- `target/conformance-report.json` — JSON (CI integration)

`mah conformance` exits with code 0 if pass rate ≥ 95%, otherwise exit 1
(CI gating, P12-9).

## Milestones

| Milestone                         | Status     | Scope |
|-----------------------------------|------------|-------|
| **Phase 1** (Week 10)              | ✅ done    | framework skeleton + synthetic fixtures + compare + report |
| **P11-1.5**                       | ✅ done    | dsh_synthetic 7/7 via `convert_input` (RunStart + UserInput + ModelResponse chain) |
| **P11-2**                         | ✅ done    | dsh 9 acp-snapshot fixtures → 9/9 = 100% (replay identity) |
| **P12-1**                         | ✅ done    | `DshFixtureCache` mtime invalidation |
| **P12-9**                         | ✅ done    | `mah conformance` exit 1 on < 95% pass rate |

## Out of scope

- Real model adapter (uses stub)
- Persistence layer
- Cross-process (server vs cli)
- Plugin-by-plugin equivalence comparison (application plugins are first-party only)
