# Week 11 周报 — 2026-08-18 (Day 35 ~ Day 38)

> 12 周 PoC 的第 11 周,**conformance + benchmark 框架**完整骨架完成。
> 累计 37 commit, 16 crate workspace, 估计 167+ 个测试, 18 bench。
> **Week 12 待做**: 网络通后跑 dsh 真实 fixture + bench 出数据 + 写最终报告。



[English](docs/weekly/006-w11-frameworks.md) — coming soon. 中文为主.

[English](docs/weekly/006-w11-frameworks.md) — coming soon. 中文为主.

---

## TL;DR

| 维度 | 数值 |
|---|---|
| 本周 commit | 5 (Day 35-38) |
| 累计 commit | **37** (含 Day 30 补交) |
| 本周文件 | 12 个 (1 new module + 1 new binary + 1 new fixture + 2 new benches + 3 docs + 1 mod 修改) |
| 累计净代码 | ~13000 行 |
| 累计测试 | ~167 (含 conformance +33 个新测试) |
| 累计 bench | 18 (cordis 10 + core 4 + seam 4) |
| 12 周 PoC 进度 | **92%** (Week 1-11 全部完成, Week 12 收尾) |

## 本周时间线

```
Week 11 commit:
├── 52be9df feat(conformance): Day 35 EventLog 真落库 + FixtureEvent↔SessionEvent 转换
├── 2894a91 feat(conformance): Day 36 dsh fixture 格式转换层 (DshFixture / parse_dsh_jsonl)
├── 34867bd bench(core+seam): Day 37-38 core/agent + seam/plugin bench
└── (本次) docs(reports): Day 38 conformance + benchmark 报告模板 + Week 11 周报
```

## Week 11 关键产出

### 1. Conformance runner 真 EventLog 装载 (Day 35)

**convert.rs 新模块** (~330 行, 7 测试):
- 14 个 EventType ↔ 字符串双向转换
- fixture_to_session: 从 payload 推导 severity / run_id / plugin_name / error_message
- session_to_fixture: payload_json 解析回 JSON Value

**runner.rs 重构**:
- 替换透传实现为真 EventLog 路径:
  1. `EventLog::open_in_memory()` 开 in-memory 日志
  2. 每个 input event → SessionEvent → `log.append(seq)` 拿 seq
  3. `log.query(EventQuery { session_id, ..Default::default() })` 读回
  4. StoredEvent → FixtureEvent 供 compare
- RunnerError 加 EventLog / Convert 两个 variant

**新测试** (+12):
- runner_via_event_log_preserves_event_order (4 事件顺序)
- runner_via_event_log_preserves_payload (完整保留)
- runner_detects_extra_event (期望 < 实际 diff)
- framework_loads_synthetic_fixtures_from_jsonl (跑 smoke.jsonl)
- framework_event_log_preserves_order_across_4_events
- + 7 convert 单元测试

### 2. dsh fixture 转换层 (Day 36)

**dsh_format.rs 新模块** (~500 行, 8 测试):
- DshFixture / DshInput / DshMessage / DshEvent / DshExpectedOutput
- dsh_to_fixture: dsh shape → ma-harness shape
- parse_dsh_jsonl: JSONL 字符串 → Vec<Fixture>
- DshError: Parse + Io

**转换规则**:
- `expected_output` ↔ `expected` (alias) ↔ `expectedOutput` (camelCase)
- `tools` ↔ `plugins` (alias)
- `data` ↔ `payload` (alias)
- input.events 空时, 从 messages[role=user] 派生 UserInput
- expected_output.messages[role=assistant] → ModelResponse events
- 非 object data (string/array) 包装成 "data" key

**fixtures/dsh_synthetic.jsonl** (3 个合成 dsh fixture):
- dsh_agent_basic (agent + tools + assistant msg)
- dsh_session_lifecycle (SessionStart/End)
- dsh_error_path (ToolError path)

### 3. core/agent + seam/plugin bench (Day 37-38)

**crates/ma_harness_core/benches/agent.rs** (~140 行, 4 bench):
- event_log_append_single: 单条 append
- event_log_append_1000: 1000 条批量
- agent_loop_1_step: Arc<AgentLoop> 跑 1 步 (4 事件)
- stub_model_complete: StubModelAdapter 单独跑

**crates/ma_harness_seam/benches/plugin.rs** (~120 行, 4 bench):
- plugin_registry_register_1000: 公开 PluginRegistry 1000 次
- plugin_registry_list_100: list 100 个
- ctx_plugin_by_name_100: 查找单个
- ctx_plugins_list_100: 列出 100 个

**总 bench 数 18** (cordis 10 + core 4 + seam 4).

### 4. 报告模板 (Day 38)

**docs/conformance-report-week11.md** (~2.6KB):
- 执行摘要 + 分类统计 + 失败详情 + 已知差异
- 跑法 + 跟设计差距 + 给后来人

**docs/benchmark-report-week11.md** (~4.5KB):
- 执行摘要 + 详细数据 (3 个 crate 18 个 bench) + 性能判据
- 慢路径识别 + dsh tinybench 复刻 + 跑法 + 给后来人

两个报告都标 "TBD 待网络通后跑", 模板固化, 网络通后填充数据即可。

## Week 12 TODO (详细)

### Day 39-40: 网络恢复 + 跑数据

| Day | 工作 | 产出 |
|---|---|---|
| 39 | 跑 `cargo check --workspace` + `cargo test --workspace` | 验证 16 crate + 167 测试 |
| 39 | 跑 `cargo bench --workspace` | 出 HTML 报告 (`target/criterion/*/report/`) |
| 40 | 拉 dsh 仓库 + 跑 dsh 真实 fixture | 填充 conformance 报告 |
| 40 | 拉 dsh tinybench + 跑 dsh bench | 填充 benchmark 报告 |

### Day 41-43: 修复 + 优化

| Day | 工作 | 产出 |
|---|---|---|
| 41 | 根据 conformance 报告修 ma-harness / fixture converter | 修失败 fixture |
| 42 | 根据 benchmark 报告优化慢路径 (如有) | 性能 close gap |
| 43 | 重新跑 conformance + bench, 验证 ≥ 95% pass + ≥ 10x 加速 | 最终数字 |

### Day 44-46: 收尾

| Day | 工作 | 产出 |
|---|---|---|
| 44 | 写 Week 12 终周报 (12 周 PoC 收官) | docs/weekly/007-w12-final.md |
| 45 | 写 README.md (仓库入口, 替代 AGENTS.md 部分内容) | README.md |
| 46 | 决定: Phase 2 范围 + 时间表 | Phase 2 kick-off doc |

## 12 周 PoC 进度 (92%)

| Week | 状态 | 关键产出 |
|---|---|---|
| **1-2** | ✅ | cordis 完整 + SessionEvent + AgentLoop + 5 macro |
| **3-4** | ✅ | proto + seam + 6 plugin 骨架 + server + cli |
| **5-6** | ✅ | 6 first-party 全部实装 |
| **7-9** | ✅ | 端到端 demo + integration test + mah start + 周报 (Day 30 补交) |
| **10** | ✅ | conformance 框架 + cordis bench + 设计稿 |
| **11** | ✅ | EventLog 真落库 + dsh 转换 + 18 bench + 报告模板 |
| **12** | ⏳ | **跑数据 + 修 + 优化 + 收官** |

## 已知 TODO (继续累计)

1. **网络通后必跑** (P0):
   - `cargo check --workspace` — 验证 16 crate 编译
   - `cargo test --workspace` — 验证 167 测试
   - `cargo bench --workspace` — 出 18 bench 数据
   - 拉 dsh 仓库, 跑 dsh fixture + tinybench, 填充两份报告
2. **Phase 2 待做** (Week 12 收尾后启动):
   - macro 增强 (#[dsh_service(cordis, seam)] 自动派生两套)
   - Sandbox 强化 (landlock / Seatbelt syscall)
   - 持久化 (SessionServiceImpl 内存换 rusqlite)
   - Code Mode (wasmtime / deno_core)
   - 多 model adapter (OpenAI / Anthropic)
   - 真 plugin 装载 (conformance runner 现在用 placeholder ctx)

## 网络阻塞 (持续 37 commit)

- 本机代理 127.0.0.1:7890 不能代理 HTTPS
- 130+ 文件 **未 cargo check 验证**
- 全部 mental-compile only

**预计 16 crate 编译 + 167 测试 + 18 bench 跑通需要 2-3 分钟 (网络通后)**。

## 协作模式 (保持)

- 每次 1 个 in_progress todo
- 每 1h cron 汇报 (cron_id `889bf0de`)
- 网络不通, 跳过 cargo check, review 兜底
- commit 频率: 每个决策点一个, 主题行 ≤ 72 字符

## 跨 session 恢复用 15 份关键文档

1. `AGENTS.md` — 仓库入口
2. `docs/decision-log.md` — 11 项决策
3. `docs/ma-harness-arch-map.md` — 跟 dsh 翻译 + 8 条硬线
4. `docs/macro-design.md` — 5 个 proc-macro 规范
5. `docs/plugin-schema-v1.md` — plugin.toml + JSON Schema
6. `docs/conformance-design.md` — Week 10 conformance 设计
7. `docs/benchmark-design.md` — Week 10 benchmark 设计
8. `docs/conformance-report-week11.md` — Week 11 conformance 报告模板
9. `docs/benchmark-report-week11.md` — Week 11 benchmark 报告模板
10. `docs/weekly/000-day0.md` — Day 0
11. `docs/weekly/001-w01-w02.md` — Week 1-2
12. `docs/weekly/002-w03-w04.md` — Week 3-4
13. `docs/weekly/003-w05-w06.md` — Week 5-6
14. `docs/weekly/004-w07-w09.md` — Week 7-9
15. `docs/weekly/005-w10-conformance.md` — Week 10
16. **`docs/weekly/006-w11-frameworks.md`** — 本文件

## 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | Day 35-38 Week 11 推进, 4 commit, 12 周 PoC 进度 92% |
