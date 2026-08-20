# Conformance Report — Week 11 (2026-08-18)

> **状态**: 模板, 待网络通后跑 dsh 真实 fixture 填充.
> 关联: [`conformance-design.md`](./conformance-design.md) § 6 报告格式.



[English](../conformance-report-week11.md) — coming soon. 中文为主.


---

## 1. 执行摘要 (待填)

- **总 fixture 数**: TBD
- **通过**: TBD
- **失败**: TBD
- **通过率**: TBD (目标 ≥ 95%)
- **总耗时**: TBD

## 2. Fixture 分类统计 (待填)

| 分类 | 总数 | 通过 | 失败 | 通过率 |
|---|---|---|---|---|
| tool_call | TBD | TBD | TBD | TBD% |
| agent_run | TBD | TBD | TBD | TBD% |
| session_lifecycle | TBD | TBD | TBD | TBD% |
| event_ordering | TBD | TBD | TBD | TBD% |
| error_path | TBD | TBD | TBD | TBD% |
| **合计** | TBD | TBD | TBD | TBD% |

## 3. 失败 fixture 详情 (待填)

### 3.1 [fixture name]

- **Diff**: TBD
- **根因**: TBD
- **修复方案**: TBD
- **状态**: KNOWN_DIFF / TO_FIX / SKIP

### 3.2 [fixture name]

...

## 4. 已知差异 (platform / 序列化 / 字段)

| 差异 | 根因 | 接受? |
|---|---|---|
| 时间戳格式不同 | dsh 跟 ma-harness 时区处理 | ✅ 跳过比对 |
| UUID 每次重放不同 | 协议不强制 | ✅ 跳过比对 |
| 字段顺序不同 | serde 默认 | ✅ 浅比对已容错 |
| CRLF vs LF (Windows) | echo 平台差异 | ✅ 跳过 |
| EventType enum 命名 | dsh 可能用 kebab-case | ⏳ 待 dsh fixture 校准 |

## 5. 跑法 (供 reviewer 复现)

```bash
# 跑合成 fixture (无网络, 立即可跑)
cargo test -p ma_harness_conformance

# 跑 dsh 真实 fixture (需要网络)
git clone <dsh-repo> /tmp/dsh-fixtures
cargo test -p ma_harness_conformance -- --nocapture

# 写报告
cargo run -p ma_harness_conformance --bin run-conformance -- --fixtures fixtures/ --output target/
```

## 6. 跟 Week 10 设计的差距

| 设计 | 实际 | 差距 | 影响 |
|---|---|---|---|
| Fixture 格式 v1 | ✅ 实现 | 无 | 0 |
| Runner 真 EventLog | ✅ 实现 | 无 | 0 |
| Compare 浅比对 | ✅ 实现 | 无 | 0 |
| Report Markdown + JSON | ✅ 实现 | 无 | 0 |
| dsh 真实 fixture | ⏳ 待跑 | 网络阻塞 | 网络通后 0 |
| ≥ 95% pass rate | ⏳ 待验证 | 同上 | 同上 |

---

## 给后来人

跑 conformance 报告时:
1. 跑前清 `target/conformance-report.*`
2. 失败 fixture 优先看 runner panic 还是 compare diff
3. KNOWN_DIFF 跟 TO_FIX 分开列, 报告给业务方看时只显示 TO_FIX
4. 平台差异 (Windows CRLF / 路径) 用 KNOWN_DIFF 标, 业务方知道就行

如果通过率 < 95%:
1. 看分类统计, 哪个类别挂得多
2. 抽样 5 个失败 fixture, 看根因是不是同一类
3. 跑分类 → 改 ma-harness / fixture converter / 比对规则
4. 重新跑, 验证 ≥ 95%
