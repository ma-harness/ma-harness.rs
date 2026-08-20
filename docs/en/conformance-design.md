# Conformance Test Design (Week 10)

[English](conformance-design.md) | [简体中文](../zh-CN/conformance-design.md)

> **Purpose**: Verify that `ma-harness` produces the same output as
> DeepSeek Harness (dsh) under the same input, ensuring semantic equivalence.
> **Status**: Week 10 design draft; implementation in progress.
> **Related docs**: `benchmark-design.md`, `ma-harness-arch-map.md`,
> `docs/weekly/004-w07-w09.md`

---

## TL;DR

| Dimension           | Value |
|---------------------|-------|
| Crate               | `ma_harness_conformance` (new, 15 → 16 members) |
| Fixture format      | JSONL (one fixture per line) |
| Fixture source      | 1. dsh's `tests/fixtures/*.jsonl` (converted); 2. ma-harness own (smoke) |
| Comparison dim      | event sequence (event type + payload schema, not byte-for-byte) |
| How to run          | `cargo test -p ma_harness_conformance` (single thread) or standalone binary |
| Report              | Markdown + JSON, written to `target/conformance-report.{md,json}` |
| Target pass rate    | **≥ 95%** (Week 11 reported metric) |
| Failure granularity | event level (skip equal events, list first diff) |

---

## 1. What Conformance is NOT

Explicitly **not** doing these:

- **No** byte-for-byte comparison — dsh's output timestamps, UUIDs, and
  serialization order differ from ma-harness; we only compare "event type
  + key fields".
- **No** model adapter calls — only ctx + event log + tool registry; no real
  LLM is invoked.
- **No** performance comparison — performance is in `benchmark-design.md`;
  conformance is about behavior.
- **No** UI comparison — dsh is a TS library, ma-harness is a Rust library;
  no shared UI surface.
- **No** plugin-by-plugin equivalence — application plugins (bash/fs/web) are
  ma-harness first-party only; dsh doesn't have these.

## 2. What Conformance IS

Run a fixed set of "input event sequences" (trace) and compare the two
sides' "output event sequences":

```
        dsh fixture (input events)        ma-harness fixture (input events)
                    │                                   │
                    ▼                                   ▼
            dsh TypeScript code               ma-harness Rust code
                    │                                   │
                    ▼                                   ▼
        output events (TypeScript)         output events (Rust)
                    │                                   │
                    └────────►  compare ◄───────────────┘
                                   │
                                   ▼
                          pass / fail + diff
```

**Key design**: a fixture is a trace (event stream), not a unit test.
The Conformance runner replays events, captures actual events, and compares
against the expected.

## 3. Fixture format (v1)

Each fixture is one JSON line:

```json
{
  "name": "tool_call_bash_echo",
  "category": "tool_call",
  "description": "Bash tool echoes 'hello'",
  "input": {
    "session_id": "fixture-001",
    "plugins": ["bash"],
    "events": [
      {"type": "RunStart", "payload": {"prompt": "echo hello"}, "timestamp_ms": null},
      {"type": "ToolCall", "payload": {"tool": "bash", "args": {"command": "echo hello"}}, "timestamp_ms": null},
      {"type": "ToolResult", "payload": {"tool": "bash", "result": "hello\n"}, "timestamp_ms": null},
      {"type": "RunEnd", "payload": {"status": "ok"}, "timestamp_ms": null}
    ]
  },
  "expected": {
    "events": [
      {"type": "RunStart", "payload_match": {"prompt": "echo hello"}},
      {"type": "ToolCall", "payload_match": {"tool": "bash"}},
      {"type": "ToolResult", "payload_match": {"tool": "bash", "result": "hello\n"}},
      {"type": "RunEnd", "payload_match": {"status": "ok"}}
    ],
    "final_state_match": {
      "event_count": 4,
      "model_visible_count": 3
    }
  }
}
```

Fields:

- `name` (string, required) — unique fixture name, used in the report
- `category` (enum, required) — `tool_call` | `agent_run` | `session_lifecycle` | `event_ordering` | `error_path`
- `description` (string, optional) — human-readable description
- `input` (object, required):
  - `session_id` (string) — arbitrary, used for logging
  - `plugins` (array of string) — plugin names to load at fixture start
  - `events` (array) — events fed to the runner
- `expected` (object, required):
  - `events` (array) — expected output events (compared in order)
    - `payload_match` (object) — shallow compare; field present + equal;
      missing fields are accepted
  - `final_state_match` (object) — assertion on ctx state after running the fixture

**Why `payload_match` (shallow compare)**:

- dsh's timestamp differs from ma-harness; forcing equality produces false
  positives.
- dsh's serialization fields may be more or fewer; shallow compare allows
  ma-harness to include extra fields (e.g. `tracing_id`).
- Only compare the fields the fixture author cares about; fixture expressiveness
  is higher.

## 4. Runner flow

```
run_fixture(fixture) -> Result<ConformanceResult, RunnerError>:
    1. ctx = new()
    2. for plugin in fixture.input.plugins:
           plugin_loader.load(plugin).install(ctx)  // real plugin, not mock
    3. event_log = EventLog::new()  // append-only
    4. for event in fixture.input.events:
           event_log.append(decode(event))
           ctx.emit(decode(event))  // trigger listener
    5. actual_events = event_log.read_all()
    6. compare(actual_events, fixture.expected)
    7. return ConformanceResult { passed, diffs, ... }
```

**Key decisions**:

- **Real plugin, not mock**: consistent with integration test, exposes
  PSR-4 style "integration gap".
- **Synchronous emit**: ma-harness's emit is sync, fixture is also sync; we
  don't introduce async timing confusion.
- **No model adapter**: fixture directly emits ToolCall/ToolResult; we don't
  go through the ModelRequest chain.

## 5. Compare algorithm

```rust
fn compare_events(actual: &[SessionEvent], expected: &[ExpectedEvent]) -> Vec<Diff> {
    let mut diffs = Vec::new();
    let n = max(actual.len(), expected.len());
    for i in 0..n {
        let a = actual.get(i);
        let e = expected.get(i);
        match (a, e) {
            (None, Some(_)) => diffs.push(Diff::MissingEvent { index: i }),
            (Some(_), None) => diffs.push(Diff::ExtraEvent { index: i }),
            (Some(actual_event), Some(expected_event)) => {
                if actual_event.event_type().as_str() != expected_event.event_type {
                    diffs.push(Diff::TypeMismatch { index: i, ... });
                }
                for (key, expected_value) in &expected_event.payload_match {
                    if !actual_event.payload().contains_key(key) {
                        diffs.push(Diff::MissingField { index: i, key });
                    } else if actual_event.payload()[key] != *expected_value {
                        diffs.push(Diff::FieldMismatch { index: i, key, ... });
                    }
                }
            }
            (None, None) => break,
        }
    }
    diffs
}
```

**Output**:

- The first diff tells you why it failed.
- All diffs are listed (so you can debug, not fix one and re-run for the next).

## 6. Report

```rust
pub struct ConformanceReport {
    pub summary: ReportSummary,
    pub results: Vec<ConformanceResult>,
}

pub struct ReportSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,  // passed / total
    pub duration_ms: u64,
}
```

Two formats:

- **Markdown** (`target/conformance-report.md`) — for humans, table + diff list
- **JSON** (`target/conformance-report.json`) — for machines, CI integration

Report template (excerpt):

```markdown
# Conformance Report — 2026-08-18

**Pass rate**: 38 / 40 = 95.0% ✅ (target ≥ 95%)

## Failed fixtures

### tool_call_bash_echo_with_spaces
- Diff[3]: FieldMismatch key="result", expected="hello world\n", actual="hello world\r\n"
- Reason: CRLF vs LF (Windows echo behavior)
- Status: KNOWN_DIFF (platform-dependent, accepted)
```

## 7. Fixture source (two tracks)

**Track A: synthetic fixtures (smoke)**

- 5-10 fixtures ship with the repo; verify the framework itself
- Run in any network condition
- Independent of dsh; only validates ma-harness internal consistency

**Track B: dsh real fixtures (conformance)**

- Pull from dsh repo's `tests/fixtures/*.jsonl`
- Format conversion: dsh's TypeScript shape → ma-harness JSONL shape
- Failures = real problems; list and analyze manually
- Week 10 implements the framework; Week 11 pulls dsh fixtures and runs

## 8. Out of scope

- **No** fuzz testing (proptest is separate, see `docs/tech-stack.md` § "Testing stack")
- **No** real model adapter calls (`StubModelAdapter` is enough)
- **No** cross-process conformance (server vs CLI binary); everything is in-process
- **No** persistence layer conformance (`SessionServiceImpl` is Phase 2)

## 9. Failure handling

- **Runner panic**: caught + fixture marked as "error" (not "fail"); listed separately in the report
- **Plugin load failure**: fixture marked "skip" (listed in the "skipped" section)
- **Compare after first diff**: still list **all** diffs; do not short-circuit
  (debug-friendly)
- **Fixture parse failure**: reported at load time; not entering the runner

## 10. Relationship to other docs

- `benchmark-design.md` — performance comparison, orthogonal to conformance
- `ma-harness-arch-map.md` § 10 "Hook & Listener mapping" — source of conformance
  event types
- `docs/weekly/004-w07-w09.md` — Week 7-9 done; Week 10 starts conformance

---

## Notes for future contributors

When writing a new fixture:

1. Use `name` to describe the scenario (e.g. `tool_call_bash_unicode`, not `tc_001`)
2. Pick the closest `category`; don't invent a new enum
3. In `payload_match`, only list the **fields you care about**; ma-harness adding
   extra fields is fine
4. Run `cargo test -p ma_harness_conformance -- --nocapture` to see the actual diff
5. On failure, first see whether it's "Runner panic" or "Compare diff" — the
   former is a framework bug, the latter is a fixture problem

To run conformance:

```bash
# Synthetic fixtures (no network)
cargo test -p ma_harness_conformance

# All fixtures (including dsh, requires fixtures/dsh/ directory)
cargo run -p ma_harness_conformance --bin run-conformance -- --fixtures fixtures/ --output target/
```

## 11. P11+ updates (Week 10 之后)

- **P11-1.5**: `convert_input` derivation now emits full event chain (RunStart + UserInput + ModelResponse); 2 failing unit tests + 1 smoke test fixed; dsh_synthetic 28.6% → 100%.
- **P11-2**: dsh 9 acp-snapshot fixtures (real dsh repo, not converted synth) → 9/9 = 100% via Python conversion script (`dsh_snap_convert.py`). Adds the `--dsh` flag to `mah conformance`.
- **P12-1**: `DshFixtureCache` — mtime-based invalidation for the dsh_snap.jsonl; 4/4 cache tests.
- **P12-3**: `docs/README.md` becomes the master index; `mkdocs.yml` v2 prepared for static site.
- **P12-9**: `mah conformance` exit code: pass rate < 95% → exit 1 (CI gating).

See `docs/p11-final-report.md`, `docs/p12-final-report.md`, and
`docs/dsh-benchmark-report.md` for the current snapshot.
