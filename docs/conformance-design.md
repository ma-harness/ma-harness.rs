# Conformance Test Design (Week 10)

> **目的**: 验证 ma-harness 的行为跟 DeepSeek Harness (dsh) 在相同输入下产生相同输出,确保语义等价。
> **状态**: Week 10 设计稿,实现中。
> **关联文档**: `benchmark-design.md`、`ma-harness-arch-map.md`、`docs/weekly/004-w07-w09.md`

---

## TL;DR

| 维度 | 数值 |
|---|---|
| Crate | `ma_harness_conformance` (新加, 15 → 16 members) |
| Fixture 格式 | JSONL (一行一个 fixture) |
| Fixture 来源 | 1. dsh 的 `tests/fixtures/*.jsonl` 转换 2. ma-harness 自己合成 (smoke) |
| 比对维度 | 事件序列 (event type + payload schema, 不强求 byte-for-byte) |
| 跑法 | `cargo test -p ma_harness_conformance` (单线程) 或独立 binary |
| 报告 | Markdown + JSON, 存 `target/conformance-report.{md,json}` |
| 目标通过率 | **≥ 95%** (Week 11 报告指标) |
| 失败粒度 | 事件级 (跳过相等的, 列出第一个 diff) |

---

## 1. Conformance 不是什么

明确**不**做这些:

- **不**做 byte-for-byte 比较 — dsh 输出时间戳、UUID、序列化顺序跟 ma-harness 不同, 只比"事件类型 + 关键字段"
- **不**跑 model adapter — 只跑 ctx + event log + tool registry, 不调真实 LLM
- **不**做性能比较 — 性能在 `benchmark-design.md`, conformance 只看行为
- **不**做 UI 比较 — dsh 是 TS 库, ma-harness 是 Rust 库, 没有共享 UI surface
- **不**做 plugin-by-plugin 等价 — 业务方插件 (bash/fs/web) 是 ma-harness first-party 独有, dsh 没这些

## 2. Conformance 是什么

跑一组固定的"输入事件序列" (trace), 比对两边的"输出事件序列":

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

**关键设计**: fixture 是 trace (事件流), 不是 unit test。Conformance runner 重放事件, 捕获实际事件, 比对期望。

## 3. Fixture 格式 (v1)

每个 fixture 是一行 JSON:

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

字段:
- `name` (string, required) — fixture 唯一名, 用于报告
- `category` (enum, required) — `tool_call` | `agent_run` | `session_lifecycle` | `event_ordering` | `error_path`
- `description` (string, optional) — 给人类看的说明
- `input` (object, required):
  - `session_id` (string) — 任意, 用于日志
  - `plugins` (array of string) — fixture 启动时装载的 plugin 名
  - `events` (array) — 喂给 runner 的事件序列
- `expected` (object, required):
  - `events` (array) — 期望输出事件 (按顺序比对)
    - `payload_match` (object) — 浅比较, 字段存在 + 等值; 缺失字段 = 接受
  - `final_state_match` (object) — 跑完 fixture 后 ctx 状态的断言

**为什么用 `payload_match` 浅比较**:
- dsh 时间戳跟 ma-harness 不同, 强制相等会假阳性
- dsh 序列化字段可能多/少, 浅比较允许 ma-harness 多塞字段 (如 tracing_id)
- 只比较"fixture 作者关心"的字段, fixture 表达力更强

## 4. Runner 流程

```
run_fixture(fixture) -> Result<ConformanceResult, RunnerError>:
    1. ctx = new()
    2. for plugin in fixture.input.plugins:
           plugin_loader.load(plugin).install(ctx)  // 真 plugin, 不用 mock
    3. event_log = EventLog::new()  // append-only
    4. for event in fixture.input.events:
           event_log.append(decode(event))
           ctx.emit(decode(event))  // 触发 listener
    5. actual_events = event_log.read_all()
    6. compare(actual_events, fixture.expected)
    7. return ConformanceResult { passed, diffs, ... }
```

**关键决策**:
- **真 plugin 不 mock**: 跟 integration test 一致, 暴露 PSR-4 风格的"集成 gap"
- **同步 emit**: ma-harness 的 emit 是同步, fixture 也用同步, 不引入 async 时间错乱
- **无 model adapter**: fixture 直接发 ToolCall/ToolResult, 不走 ModelRequest 链路

## 5. Compare 算法

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

**输出**:
- 第一个 diff 让人知道为什么失败
- 全部 diff 让人能调试 (而不是 fix 完一个再看下一个)

## 6. 报告

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

两种格式:
- **Markdown** (`target/conformance-report.md`) — 人类看, 表格 + diff 列表
- **JSON** (`target/conformance-report.json`) — 机器读, CI 集成

报告模板 (excerpt):
```markdown
# Conformance Report — 2026-08-18

**Pass rate**: 38 / 40 = 95.0% ✅ (target ≥ 95%)

## Failed fixtures

### tool_call_bash_echo_with_spaces
- Diff[3]: FieldMismatch key="result", expected="hello world\n", actual="hello world\r\n"
- Reason: CRLF vs LF (Windows echo 行为)
- Status: KNOWN_DIFF (platform-dependent, accepted)
```

## 7. Fixture 来源 (双轨)

**轨道 A: 合成 fixture (smoke)**
- 仓库自带 5-10 个 fixture, 验证 framework 本身
- 任何网络条件下都能跑
- 跟 dsh 无关, 只验 ma-harness 内部一致性

**轨道 B: dsh 真实 fixture (conformance)**
- 从 dsh 仓库 `tests/fixtures/*.jsonl` 拉
- 格式转换: dsh 的 TypeScript shape → ma-harness 的 JSONL shape
- 跑不通过的 = 真问题, 列出来手动分析
- Week 10 实现 framework, Week 11 拉 dsh fixture 跑

## 8. 不在 scope

- **不**做 fuzz testing (proptest 是单独的, 见 `docs/tech-stack.md` §"测试栈")
- **不**做 model adapter 真实调用 (StubModelAdapter 够用)
- **不**做跨进程 conformance (server vs CLI binary), 都是 in-process
- **不**做持久化层的 conformance (SessionServiceImpl 是 Phase 2)

## 9. 失败处理

- **Runner panic**: 捕获 + 标记 fixture 为 "error" (不是 "fail"), 报告里单列
- **Plugin 装载失败**: fixture 标 "skip" (列在 "skipped" 段)
- **Compare 第一个 diff 后**: 仍然列**所有** diff, 不短路 (debug 友好)
- **Fixture parse 失败**: 在加载阶段报, 不进 runner

## 10. 跟其它 doc 的关系

- `benchmark-design.md` — 性能比较, 跟 conformance 正交
- `ma-harness-arch-map.md` § 12 "Hook 与 Listener 映射" — conformance 事件类型的来源
- `docs/weekly/004-w07-w09.md` — Week 7-9 完成, Week 10 开始 conformance

---

## 给后来人

写新 fixture 时:
1. 用 `name` 描述场景 (e.g. `tool_call_bash_unicode`, 不 `tc_001`)
2. `category` 选最贴近的, 不创造新 enum
3. `payload_match` 只写**关心的字段**, 别的让 ma-harness 多塞无所谓
4. 跑 `cargo test -p ma_harness_conformance -- --nocapture` 看实际 diff
5. 失败先看 "Runner panic" 还是 "Compare diff", 前者是 framework 问题, 后者是 fixture 问题

跑 conformance:
```bash
# 合成 fixture (无网络)
cargo test -p ma_harness_conformance

# 全部 fixture (含 dsh, 需要 fixtures/dsh/ 目录)
cargo run -p ma_harness_conformance --bin run-conformance -- --fixtures fixtures/ --output target/
```
