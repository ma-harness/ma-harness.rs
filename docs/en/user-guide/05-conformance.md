# 05 — Conformance

> **Goal**: validate that your agent behaves correctly by running it
> against a set of expected input/output fixtures, and comparing to
> `dsh` if you want.

[English](05-conformance.md) | [简体中文](../../zh-CN/user-guide/05-conformance.md)

## Prerequisites

- `mah` CLI installed (see [01-installation.md](01-installation.md))
- A set of fixtures (JSONL) describing input events and expected output
- ~15 minutes

## What is conformance?

A **fixture** is a single test case: input events + expected output events.
The framework:
1. Loads each fixture
2. Runs the agent against the input
3. Compares the actual output to the expected output
4. Reports pass/fail per fixture

A **JSONL file** (one fixture per line) is the standard format.

## Step-by-step

### Step 1 — Run the bundled smoke tests

We ship 8 synthetic smoke fixtures:

```bash
mah conformance --fixtures crates/ma-harness-conformance/fixtures/smoke.jsonl
```

Expected output:

```
[INFO] Loading 8 fixtures from crates/ma-harness-conformance/fixtures/smoke.jsonl
[INFO] Running conformance...
  ✓ synthetic_tool_call_echo        PASS  (3 events)
  ✓ synthetic_run_start_end         PASS  (4 events)
  ✓ synthetic_agent_with_tool       PASS  (5 events)
  ✗ synthetic_extra_event_failure   FAIL  (expected: 4 events, got: 5)
  ✓ synthetic_empty_input           PASS
  ✓ synthetic_session_lifecycle     PASS
  ✓ synthetic_error_path            PASS
  ✓ synthetic_model_request_response PASS

[INFO] 7/8 passed (87.5%)
[INFO] Report saved to: target/conformance-report.md
[INFO] JSON saved to: target/conformance-report.json
```

The one failure is **expected** — it tests that the framework catches
"extra events" (a deliberate test of the comparator).

### Step 2 — Read the report

The Markdown report:

```bash
cat target/conformance-report.md
```

```markdown
# Conformance Report

**Run time**: 2026-08-20T16:30:00Z
**Total**: 8 fixtures
**Passed**: 7
**Failed**: 1 (expected)

## Failures

### synthetic_extra_event_failure

**Expected** (4 events): RunStart → ModelRequest → ModelResponse → RunEnd
**Actual** (5 events): RunStart → ModelRequest → ModelResponse → ModelResponse → RunEnd

The framework correctly identified an extra ModelResponse.
```

### Step 3 — Write your own fixture

Create `my-fixture.jsonl` (one JSON per line):

```json
{"input": {"events": [{"type": "UserInput", "payload": {"content": "what is 2+2?"}}]}, "expected": {"events": [{"type": "ModelResponse", "payload": {"content": "4"}}]}}
{"input": {"events": [{"type": "UserInput", "payload": {"content": "hello"}}]}, "expected": {"events": [{"type": "ModelResponse", "payload": {"content": "world"}}]}}
```

Run it:

```bash
mah conformance --fixtures my-fixture.jsonl
```

### Step 4 — Compare to dsh (advanced)

If you've cloned the [dsh repository](https://github.com/deepseek-ai/dsh),
you can run their fixtures through `mah`:

```bash
# Set the path
export DSH_FIXTURE_ROOT=/path/to/dsh/tests/fixtures

# Run with dsh conversion layer
mah conformance --fixtures $DSH_FIXTURE_ROOT --dsh
```

The `--dsh` flag activates the dsh → ma-harness conversion layer (handles
aliases like `expectedOutput` → `expected`, `tools` → `plugins`).

Output (P11-2 verified 9/9 dsh fixtures pass):

```
[INFO] Loading 9 dsh fixtures from $DSH_FIXTURE_ROOT
[INFO] Running conformance with dsh conversion layer...
  ✓ authored-error        PASS
  ✓ blocked-log           PASS
  ✓ no-model              PASS
  ✓ pin-turn              PASS
  ✓ plain-turn            PASS
  ✓ shared-pin            PASS
  ✓ rec-child             PASS
  ✓ rec-pin               PASS
  ✓ rec-skip              PASS

[INFO] 9/9 passed (100%) ✓
```

### Step 5 — Custom output directory

By default, reports go to `target/`. To change:

```bash
mah conformance --fixtures my.jsonl --output reports/
ls reports/
# conformance-report.md  conformance-report.json
```

## Fixture format reference

```json
{
  "input": {
    "events": [
      {"type": "UserInput", "payload": {"content": "..."}},
      {"type": "ModelResponse", "payload": {"content": "..."}}
    ]
  },
  "expected": {
    "events": [
      {"type": "ModelResponse", "payload": {"content": "..."}}
    ]
  }
}
```

Supported event types: `SessionStart`, `RunStart`, `UserInput`,
`ModelRequest`, `ModelResponse`, `ToolCall`, `ToolResult`,
`ToolError`, `RunEnd`, `SessionEnd`, `ApprovalRequest`, `ApprovalDecision`.

For dsh-style fixtures, see [docs/en/operations/registry-pages.md](../operations/registry-pages.md)
(or the dsh Format guide in the [architecture docs](../ma-harness-arch-map.md)).

## Verify

After step 1:

- 7/8 fixtures pass (1 expected fail)
- Reports in `target/conformance-report.{md,json}`

After step 4:

- 9/9 dsh fixtures pass (P11-2 verified)

## What's next

- **Investigate** a failing fixture: read the report, look at expected vs
  actual events, fix your agent or update the fixture
- **Integrate** conformance into CI: see [03-server.md](03-server.md) for
  GH Actions pattern
- **Run nightly** with the [dsh repository](https://github.com/deepseek-ai/dsh)
  for regression detection

## Troubleshooting

### All fixtures fail with "framework error"

The framework itself may have regressed. Check:

```bash
cargo test --package ma-harness-conformance
# All 44+ tests should pass
```

If not, that's a framework bug — open an issue.

### "fixture file not found"

Use absolute paths, or paths relative to the directory you run from:

```bash
# Relative
mah conformance --fixtures ./test.jsonl

# Absolute (recommended in CI)
mah conformance --fixtures /opt/mah-harness/fixtures/all.jsonl
```

### dsh conversion gives "unknown event type"

You're using an event type `mah` doesn't support. Check the list above
or in [docs/en/operations/registry-pages.md](../operations/registry-pages.md).
