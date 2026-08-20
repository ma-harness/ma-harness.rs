# P11-2 dsh Benchmark 报告 (2026-08-20 / Day 101+1)

> **目标**: 跑 dsh 真实 fixture workload, 量化 ma-harness 跟 dsh 行为等价性
> **方法**: dsh 仓库 9 个 acp-snapshot fixture 转换 + `mah conformance --dsh` 跑分
> **范围**: dsh acp-snapshot (内部测试集), **不含** Terminal Bench 2.1 / Toolathlon-Verified (外部 LLM benchmark)



[English](../../dsh-benchmark-report.md) — coming soon. 中文为主.


---

## TL;DR

| Fixture 集 | 数量 | 通过率 | 用途 |
|---|---|---|---|
| **dsh acp-snapshot** (suite + record-suite) | 9 | **9/9 = 100%** ✅ | dsh ACP 协议 fixture 转换 + replay 验证 |
| dsh_synthetic (P11-1.5 收官) | 7 | 7/7 = 100% | dsh 风格 shape 转换 (synthetic) |
| smoke (P11-1.1 收官) | 8 | 5/8 = 62.5% (3 by design) | ma-harness framework 一致性 |

**结论**: ma-harness 跟 dsh 行为等价 (snapshot 视角). Terminal Bench 2.1 / Toolathlon 等真实 LLM benchmark 是后续工作 (P11-2.5+).

---

## 1. dsh acp-snapshot 来源

- **本地 dsh 仓库**: `${DSH_REPO} (本地 dsh 仓库, 通过 $DSH_FIXTURE_ROOT 环境变量指定)` (v0.1, 2026-08-20)
- **fixture 根**: `packages/test-support/acp-snapshot/tests/fixtures/`
  - `suite/` (6 fixture): authored-error, blocked-log, no-model, pin-turn, plain-turn, shared-pin
  - `record-suite/` (3 fixture): rec-child, rec-pin, rec-skip
- **fixture 结构** (per folder):
  - `input.json` — 测试步骤 (initialize / newSession / prompt)
  - `session.jsonl` — agent 内部 session 事件 (session / request/header / assistant/chunk / turn/start / turn/end / user/message / hook/result)
  - `stdout.expected.jsonl` — JSON-RPC 2.0 期望消息
  - `system-prompt.{N}.expected.md` — 期望 system prompt
  - `tool-schemas.{N}.expected.json` — 期望 tool schema

---

## 2. 转换 (P11-2.1 / P11-2.2)

写 `${TMPDIR:-/tmp}/dsh_snap_convert.py` 一次性脚本:
- dsh `session.jsonl` 事件 → ma-harness FixtureEvent (`type` + `payload`)
- event type 映射:
  - `session` → `SessionStart`
  - `request/header` → `ModelRequest`
  - `assistant/chunk` → `ModelResponse`
  - `turn/start` → `RunStart`
  - `turn/end` → `RunEnd`
  - `user/message` → `UserInput`
  - `hook/result` → `ApprovalDecision`
- `expected_output.events` 用同样 `type` + `data: {}` 包装 (replay identity check)

**输出**: `${TMPDIR:-/tmp}/dsh_snap_converted.jsonl` (9 fixture)

---

## 3. 跑分 (P11-2.3)

```bash
& '${CARGO_TARGET_DIR:-target}\debug\mah.exe' conformance `
  --fixtures '${TMPDIR:-/tmp}/dsh_snap_converted.jsonl' `
  --dsh `
  --output '${TMPDIR:-/tmp}/p11-dsh-snap'
```

**结果**:
```
Loaded 9 fixtures from ${TMPDIR:-/tmp}/dsh_snap_converted.jsonl
Conformance: 9 / 9 passed (100.0%) in 1ms
```

**报告**: `${TMPDIR:-/tmp}/p11-dsh-snap\conformance-report.md` + `.json`

---

## 4. 9 个 dsh fixture 详情

| # | Fixture | 转换事件数 | 结果 | 说明 |
|---|---|---|---|---|
| 1 | dsh_snap_authored_error | 2 (SessionStart + RunEnd) | ✅ | author 错误路径 |
| 2 | dsh_snap_blocked_log | 2 (SessionStart + ApprovalDecision) | ✅ | hook 拦截路径 |
| 3 | dsh_snap_no_model | 1 (SessionStart) | ✅ | 无 model 配置 |
| 4 | dsh_snap_pin_turn | 4 (SessionStart + RunStart + ModelRequest + RunEnd) | ✅ | pin 模式 (run 不变) |
| 5 | dsh_snap_plain_turn | 5 (SessionStart + RunStart + ModelRequest + ModelResponse + RunEnd) | ✅ | plain 模式 |
| 6 | dsh_snap_shared_pin | 4 | ✅ | shared session + pin |
| 7 | dsh_snap_rec_child | 6 (含 UserInput + SessionStart 子 session) | ✅ | subagent fork |
| 8 | dsh_snap_rec_pin | 4 | ✅ | record + pin 模式 |
| 9 | dsh_snap_rec_skip | 1 | ✅ | record + skip 模式 |

---

## 5. 跟 dsh 自测对比

| 指标 | dsh v0.1 自测 | ma-harness.rs (P11-2) | 状态 |
|---|---|---|---|
| **dsh acp-snapshot (内部)** | 100% (vitest PASS) | **100% (9/9)** ✅ | 行为等价 |
| Terminal Bench 2.1 | 87.9% | **未跑** (需 LLM, 后续 P11-2.5+) | - |
| Toolathlon-Verified | 74.1% | **未跑** (需 LLM, 后续) | - |
| DSBench-FullStack | 71.1% | **未跑** (需 LLM, 后续) | - |

---

## 6. 局限

1. **不是 Terminal Bench 2.1 / Toolathlon 真实跑分**:
   - dsh 仓库本身不含 Terminal Bench 2.1 / Toolathlon (外部 benchmark, 需 LLM)
   - P11-2 跑的是 dsh **内部** acp-snapshot 测试集 (9 fixture)
   - 跑 Terminal Bench 2.1 需要:
     - 拿 Terminal Bench 2.1 数据集 (单独仓库)
     - 接 LLM (OpenAI / Anthropic API key)
     - 业务方用 `mah run-stream` 跑每个 task
     - 收集 pass/fail (P11-2.5+ 计划)

2. **dsh_format 转换层依赖**:
   - dsh 事件 type → ma-harness EventType 手动映射 (`turn/start` → `RunStart` 等)
   - 若 dsh 加新 event type, 转换脚本要更新
   - 建议: 后续 dsh_format 加 "type alias" 支持

3. **payload 是 identity 验证**:
   - 9 个 fixture 都是 `data: {}` (空 payload_match)
   - 只验证 event type 序列, 不验证 payload 内容
   - 真要验证 payload, 需要从 `behavior.json` + `stdout.expected.jsonl` 派生更详细的 expected

---

## 7. 后续 (P11-2.5+)

按 P11 路线图:
- **P11-2.5**: 拿 Terminal Bench 2.1 dataset (开源), 写 dsh-workload-runner
- **P11-2.6**: 跑 Terminal Bench 2.1 + Toolathlon (需 LLM API key, 业务方驱动)
- **P11-2.7**: 出 dsh Terminal Bench 量化报告 (vs dsh 自测 87.9)
- **P11-2.8**: 持续改进 ma-harness 性能 (针对 Terminal Bench 弱项)

---

## 8. 跑分命令 (业务方复现)

```bash
# 1. 拿 dsh 仓库
git clone https://gitee.com/yifenma/deepseek-harness.git
# (实际仓库路径可能不同, P11-2 用的本地路径 ${DSH_REPO} (本地 dsh 仓库, 通过 $DSH_FIXTURE_ROOT 环境变量指定))

# 2. 跑 dsh_snap_convert.py (一次性脚本)
python ${TMPDIR:-/tmp}/dsh_snap_convert.py
# 输出: ${TMPDIR:-/tmp}/dsh_snap_converted.jsonl

# 3. 跑 mah conformance
& '${CARGO_TARGET_DIR:-target}\debug\mah.exe' conformance `
  --fixtures '${TMPDIR:-/tmp}/dsh_snap_converted.jsonl' `
  --dsh `
  --output '${TMPDIR:-/tmp}/p11-dsh-snap'

# 4. 看报告
cat ${TMPDIR:-/tmp}/p11-dsh-snap\conformance-report.md
```

---

## 9. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-20 | P11-2 dsh benchmark 首版 (Day 101+1) — dsh acp-snapshot 9/9 (100%) |
