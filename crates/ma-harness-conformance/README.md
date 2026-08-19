# ma_harness_conformance

> Conformance test framework for ma-harness.
> 状态: **Week 10 Phase 1 完成** (framework 骨架 + 合成 fixture 跑通)。
> 关联: [`docs/conformance-design.md`](../../docs/conformance-design.md)

## 目的

验证 ma-harness 在相同 trace 输入下, 跟 DeepSeek Harness (dsh) 产生语义等价的输出。
**目标**: 通过率 **≥ 95%** (Week 11 报告指标)。

## 模块

| 模块 | 作用 |
|---|---|
| `fixture` | Fixture schema (JSONL) + 加载器 |
| `runner` | 跑 fixture, 收集实际事件 |
| `compare` | 比对实际 vs 期望, 产出 diff |
| `report` | 汇总 pass/fail, 输出 markdown + json |

## 跑法

```bash
# 跑 framework 自带的合成 fixture (无 dsh, 无网络)
cargo test -p ma_harness_conformance

# 跑 smoke test
cargo test -p ma_harness_conformance --test smoke
```

## Fixture 格式

一行一个 JSON, 字段见 `docs/conformance-design.md` § 3。

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

## 报告

跑完自动生成:
- `target/conformance-report.md` — Markdown (人类看)
- `target/conformance-report.json` — JSON (CI 集成)

## Phase 1 vs Phase 2

| 阶段 | 状态 | 范围 |
|---|---|---|
| **Phase 1** (Week 10) | ✅ Done | framework 骨架 + 合成 fixture + compare + report |
| **Phase 2** (Week 11) | ⏳ TODO | 真 plugin 装载 + EventLog 收集 + dsh 真实 fixture 接入 |

## 不在 scope

- 真实 model adapter (用 stub)
- 持久化层
- 跨进程 (server vs cli)
- plugin-by-plugin 等价比较 (业务方 plugin 是 first-party)
