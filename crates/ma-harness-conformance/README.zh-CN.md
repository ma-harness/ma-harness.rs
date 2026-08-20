# ma_harness_conformance (中文 / 简体中文)

[English](README.md) | [简体中文](README.zh-CN.md)

> ma-harness 的 conformance test 框架.
> 状态: **P11+ 已完成** (framework + 合成 fixture + dsh 9/9 + dsh_synthetic 7/7).
> 关联: [`docs/conformance-design.md`](../../docs/conformance-design.md)

## 目的

验证 ma-harness 在相同 trace 输入下, 跟 DeepSeek Harness (dsh) 产生语义等价的输出。
**目标**: 通过率 **≥ 95%** (P11-2 / Week 11 报告指标).

## 模块

| 模块 | 作用 |
|---|---|
| `fixture` | Fixture schema (JSONL) + 加载器 |
| `runner` | 跑 fixture, 收集实际事件 |
| `compare` | 比对实际 vs 期望, 产出 diff |
| `report` | 汇总 pass/fail, 输出 markdown + json |
| `dsh_format` (P11-1.5) | 把 dsh `session.jsonl` 事件转成 ma-harness `FixtureEvent` |
| `cache` (P12-1) | 基于 mtime 的 `DshFixtureCache` 用于重新跑 |

## 跑法

```bash
# 跑 framework 自带的合成 fixture (无 dsh, 无网络)
cargo test -p ma_harness_conformance

# 跑 smoke test
cargo test -p ma_harness_conformance --test smoke

# 跑 dsh 合成 fixture (P11-1.5 7/7)
mah conformance --fixtures crates/ma-harness-conformance/fixtures/dsh_synthetic.jsonl --dsh

# 跑 dsh 真实 acp-snapshot fixture (P11-2 9/9)
DSH_FIXTURE_ROOT=${DSH_REPO}/packages/test-support/acp-snapshot/tests/fixtures \
    mah conformance --fixtures crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl --dsh
```

## Fixture 格式

一行一个 JSON, 字段见 `docs/conformance-design.md` § 3.

最小 fixture:
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

dsh 风格 fixture 见 `docs/dsh-benchmark-report.md` 和
`crates/ma-harness-conformance/fixtures/dsh-snap-converted/dsh_snap.jsonl`
(9 个真实 acp-snapshot fixture).

## 报告

跑完自动生成:
- `target/conformance-report.md` — Markdown (人类看)
- `target/conformance-report.json` — JSON (CI 集成)

`mah conformance` 通过率 ≥ 95% 返 exit 0, 否则 exit 1 (CI gating, P12-9).

## 里程碑

| 里程碑 | 状态 | 范围 |
|---|---|---|
| **Phase 1** (Week 10) | ✅ 完成 | framework 骨架 + 合成 fixture + compare + report |
| **P11-1.5** | ✅ 完成 | dsh_synthetic 7/7 通过 `convert_input` (RunStart + UserInput + ModelResponse 链) |
| **P11-2** | ✅ 完成 | dsh 9 acp-snapshot fixture → 9/9 = 100% (replay identity) |
| **P12-1** | ✅ 完成 | `DshFixtureCache` mtime 失效机制 |
| **P12-9** | ✅ 完成 | `mah conformance` 通过率 < 95% 时 exit 1 |

## 不在 scope

- 真实 model adapter (用 stub)
- 持久化层
- 跨进程 (server vs cli)
- plugin-by-plugin 等价比较 (业务方 plugin 是 first-party)
