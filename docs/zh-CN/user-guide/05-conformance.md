# 05 — Conformance

> **目标**: 通过跑一组 input/output fixture 验证 agent 行为正确,
> 如果想跟 dsh 对比也可以。

[English](05-conformance.md) | [简体中文](05-conformance.md)

## 前置条件

- 装好 `mah` CLI (见 [01-installation.md](01-installation.md))
- 一组 fixture (JSONL) 描述 input events 和 expected output
- ~15 分钟

## Conformance 是什么

**Fixture** 是单个测试用例: input events + expected output events。框架:
1. 加载每个 fixture
2. 跑 agent 处理 input
3. 比对实际 output 跟 expected output
4. 报告每个 fixture pass/fail

**JSONL file** (一行一个 fixture) 是标准格式。

## 步骤

### 第 1 步 — 跑内置 smoke 测试

我们带 8 个合成 smoke fixture:

```bash
mah conformance --fixtures crates/ma-harness-conformance/fixtures/smoke.jsonl
```

期望输出:

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

唯一一个 fail 是**期望的** — 它测框架能抓到 "extra events" (主动测 comparator)。

### 第 2 步 — 看 report

Markdown report:

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

### 第 3 步 — 写自己的 fixture

建 `my-fixture.jsonl` (一行一 JSON):

```json
{"input": {"events": [{"type": "UserInput", "payload": {"content": "what is 2+2?"}}]}, "expected": {"events": [{"type": "ModelResponse", "payload": {"content": "4"}}]}}
{"input": {"events": [{"type": "UserInput", "payload": {"content": "hello"}}]}, "expected": {"events": [{"type": "ModelResponse", "payload": {"content": "world"}}]}}
```

跑:

```bash
mah conformance --fixtures my-fixture.jsonl
```

### 第 4 步 — 跟 dsh 对比 (进阶)

如果你 clone 了 [dsh 仓库](https://github.com/deepseek-ai/dsh),
可以把它们的 fixture 跑过 `mah`:

```bash
# 设路径
export DSH_FIXTURE_ROOT=/path/to/dsh/tests/fixtures

# 跑 (启用 dsh 转换层)
mah conformance --fixtures $DSH_FIXTURE_ROOT --dsh
```

`--dsh` flag 激活 dsh → ma-harness 转换层 (处理 alias 如 `expectedOutput` → `expected`,
`tools` → `plugins`)。

输出 (P11-2 验证 9/9 dsh fixture 通过):

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

### 第 5 步 — 自定义输出目录

默认 report 写到 `target/`。要改:

```bash
mah conformance --fixtures my.jsonl --output reports/
ls reports/
# conformance-report.md  conformance-report.json
```

## Fixture 格式参考

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

支持 event type: `SessionStart`, `RunStart`, `UserInput`,
`ModelRequest`, `ModelResponse`, `ToolCall`, `ToolResult`,
`ToolError`, `RunEnd`, `SessionEnd`, `ApprovalRequest`, `ApprovalDecision`。

dsh 风格 fixture 看 [docs/zh-CN/operations/registry-pages.md](../operations/registry-pages.md)
(或 [架构文档](../ma-harness-arch-map.md) 里的 dsh Format 章节)。

## 验证

第 1 步后:

- 7/8 fixture 通过 (1 个期望 fail)
- report 在 `target/conformance-report.{md,json}`

第 4 步后:

- 9/9 dsh fixture 通过 (P11-2 验证)

## 下一步

- **查** fail fixture: 读 report,对比 expected vs actual events,修 agent 或更新 fixture
- **集成** conformance 到 CI: 见 [03-server.md](03-server.md) 里的 GH Actions 模式
- **每晚跑** 配合 [dsh 仓库](https://github.com/deepseek-ai/dsh) 做 regression 检测

## Troubleshooting

### 所有 fixture 都 fail 报 "framework error"

框架本身可能回归了。检查:

```bash
cargo test --package ma-harness-conformance
# 44+ test 应该都过
```

没全过就是 framework bug,提 issue。

### "fixture file not found"

用绝对路径,或相对当前目录的路径:

```bash
# 相对
mah conformance --fixtures ./test.jsonl

# 绝对 (CI 推荐)
mah conformance --fixtures /opt/mah-harness/fixtures/all.jsonl
```

### dsh 转换报 "unknown event type"

你用了 `mah` 不支持的 event type。看上面列表,或
[docs/zh-CN/operations/registry-pages.md](../operations/registry-pages.md)。
