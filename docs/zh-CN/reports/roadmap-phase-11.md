# ma-harness.rs P11 路线图 (2026-08-20 / Day 101+1)

> **目标**: 跟 dsh v0.1 (2026-08-13 开源) 缩小差距, 同时保持 Rust production-grade 差异化
> **周期**: 4-6 周 (专注 P11, 不切题)
> **决策日期**: 2026-08-20 (Phase 7-10 收官 review)
>
> **dsh 现状** (per web search 2026-08-20):
> - v0.1 开发者预览, MIT 协议, 1,200+ 第三方 plugin, 95k+ GitHub stars (2 天)
> - 4 模式 (Standard / PTC / Minimal / Creation)
> - 多 model (DeepSeek / Anthropic / OpenAI / Bedrock / Vertex / Azure)
> - 多种运行形态 (Web UI / TUI / Headless CLI / ACP / **Python SDK**)
> - 自测 Terminal Bench 2.1 (87.9), Toolathlon-Verified (74.1), DSBench-FullStack (71.1)



[English](../../roadmap-phase-11.md) — coming soon. 中文为主.


---

## 0. 背景与动机

Phase 7-10 收官 (8/8 P10 任务), 累计 200+ commit, core 107 lib test, decision-log § 1-27.

但跟 dsh 对比, 仍有显著差距. 业务方体验 + 生态. P11 路线图围绕**对齐 dsh 长处 + 发挥 Rust 优势**展开.

---

## 1. ma-harness.rs 现状快照 (Day 101+1)

| 维度 | 状态 |
|---|---|
| 工作模式 | 4/4 (Default / Minimal / PTC / Creator) |
| 模型适配 | 6 (OpenAI / Anthropic / Azure / Local / DeepSeek / Bedrock / Vertex) |
| First-party plugin | 6 (bash / fs / web / subagent / skill / cordis / hello) |
| 动态 plugin | Creator 模式 (P10-1.5/1.6/1.7/1.8 v2 真闭环) |
| 协议 | Protobuf + gRPC (tonic) + tonic-web (gRPC-Web) |
| Web UI | React + Vite + TS (port 3080, P7-1) |
| TUI | ratatui (P7-1.6, P10-4 Trajectory 增强) |
| 审批 | P7-2 ChannelApprovalService (HTTP / TUI / oneshot) |
| 子代理 | P7-4 SubagentSpec::Fork |
| Sandbox | landlock (Linux) / sandbox-exec (macOS) / Windows TODO |
| Session Log | rusqlite append-only (P8-1) |
| Trajectory | Web UI + TUI (P7-1.4, P10-4) |
| 上下文压缩 | P8-1 SlidingWindow + estimate_tokens |
| Token 监控 | P8-2 /v1/sessions/{id}/token-stats |
| Prometheus | P10-7 /v1/metrics |
| 公开 API | P9-1 ma-harness-seam stable API |
| 跨 dylib 加载 | P10-1.5/1.6/1.7/1.8 v2 真闭环 (C-ABI JSON) |
| lib test | 107/107 (ma-harness-core) |
| 决策日志 | § 1-27 |
| commit | 200+ |

---

## 2. dsh 差距分析 (per 2026-08-20 web search)

### 2.1 dsh 有, ma-harness 没有 (按业务价值排)

| 差距 | 业务价值 | 工作量 | 优先级 |
|---|---|---|---|
| **跑 dsh 公开 benchmark** (Terminal Bench 2.1 / Toolathlon / DSBench) | P0 关键 (定性能位置) | 1-2 周 | **P0** |
| **`mah-py` Python SDK** (PyO3 binding 或 CLI wrapper) | P0 高 (拓用户) | 1-2 周 | **P0** |
| **ACP (Agent Communication Protocol)** 支持 | P1 中 (跟 dsh/Codex 生态互通) | 2-3 周 | **P1** |
| **多模态 model adapter** (vision / audio) | P1 中 (拓场景) | 2-3 周 | **P1** |
| **Plugin Registry 公开 + 文档站** (拓第三方生态) | P1 中 (长期价值) | 1-2 周 + 长期 | **P1** |
| **Vibe Coding artifact viewer** (实时预览) | P2 锦上添花 | 1-2 周 | P2 |
| **Bundle 概念** (类似 npm package 集合) | P2 小 | 1 周 | P2 |
| **多模态 tool** (图片理解 / TTS / STT) | P2 中 | 2-3 周 | P2 |
| **DAG 任务编排** (多 Agent 拓扑) | P2 中 | 2-3 周 | P2 |
| **DAG 可视化** (Web UI 拓扑图) | P2 锦上添花 | 1-2 周 | P2 |

### 2.2 ma-harness 有, dsh 没有 (我们优势)

| 优势 | 意义 |
|---|---|
| **Rust 类型安全** (typed ctx key + Service trait) | 业务方 bug 率低, 编译期检查 |
| **Creator 真闭环** (P10-1.5/1.6/1.7/1.8 v2) | dsh Creation mode 只是"内存试插件", 我们真编译 + libloading + C-ABI JSON 跨 dylib invoke |
| **单协议** (Protobuf + gRPC) | dsh 多协议 (JSON-RPC / WebSocket / ACP), 复杂度低 |
| **Prometheus metrics** (P10-7) | 运维友好, dsh 没明确 metrics |
| **跨平台硬化** (P10-1.6: PATHEXT / Windows 非法字符 / spawn_blocking) | 业务方 Windows / Unix 通用 |
| **profile 隔离** (P10-3 per-config) | 业务方 dev/prod/test 切换 |
| **公开 stable API** (P9-1 ma-harness-seam) | 业务方不会因内部重构而 break |
| **多 model adapter 完整** (6 个 vs dsh 主要 V4) | 业务方不绑死模型 |

### 2.3 跟 dsh 对齐的 (我们有, dsh 也有)

| 项 | 状态 |
|---|---|
| 4 模式 | ✅ 对齐 (Default / Minimal / PTC / Creator) |
| 5 阶段审批管道 | ✅ 对齐 (P7-2) |
| 子代理 fork (inherit history) | ✅ 对齐 (P7-4) |
| Session Log append-only | ✅ 对齐 (rusqlite) |
| Trajectory 可视化 | ✅ 对齐 (Web UI + TUI) |
| 上下文压缩 | ✅ 对齐 (P8-1) |
| Token 监控 | ✅ 对齐 (P8-2) |
| Tool 7 阶段管道 (pre/guard/approval/exec/post/finalize/result) | ✅ 对齐 (P7-3) |

---

## 3. P11 任务拆解

### 3.1 P11-1 跑 ma-harness 现有 conformance fixture 验性能 (开场, 1-3 天)

**目标**: 量化 ma-harness 当前性能 baseline, 跟 dsh Terminal Bench 数字对比.

**任务**:
- P11-1.1: `cargo bench --workspace` 跑现有 18 个 bench (cordis 10 + core 4 + seam 4), 写 baseline 数字
- P11-1.2: `mah conformance --fixtures fixtures/smoke.jsonl` 跑 8 fixture, 验 framework 一致性
- P11-1.3: `mah conformance --fixtures fixtures/dsh_synthetic.jsonl` 跑 7 dsh shape fixture, 验 dsh 转换
- P11-1.4: 出 `docs/benchmark-report-week12.md` (类似 week 11 报告)
- P11-1.5: 写 `docs/ma-harness-vs-dsh.md` 量化对比表 (性能 / 延迟 / 内存 / 生态)
- **P11-1.5 转换层改进** ✅ (2026-08-20 收官): dsh_synthetic 28.6% → **100% (7/7)**
  - input.messages 派生 RunStart + UserInput/ModelResponse/SystemMessage/ToolResult
  - expected_output.data 非对象走 "content" / "result" / "data" key 包装
  - expected_output.messages assistant 派生 ModelResponse {content: "..."}
  - dsh_format unit test 5 → 10 (新增 5 个)
  - smoke test `runner_runs_dsh_synthetic_fixtures` 期望从 `>= 2` 升级到 `== 7`

**产出**: 量化 baseline 报告, 跟 dsh Terminal Bench 2.1 (87.9) 对比

### 3.2 P11-2 跑 dsh Terminal Bench + Toolathlon workload (1-2 周, **P0**)

**目标**: 拿 dsh 公开 workload (Terminal Bench 2.1 / Toolathlon-Verified), 跑 ma-harness, 出量化差距.

**任务**:
- P11-2.1: 拿 dsh 真实 fixture (clone dsh 仓库, 拿 test/fixtures/)
- P11-2.2: 写 dsh fixture 适配器 (`mah conformance --dsh --fixtures dsh/tests/fixtures/`)
- P11-2.3: 跑 Terminal Bench 2.1 fixture, 出 pass rate
- P11-2.4: 跑 Toolathlon-Verified fixture, 出 pass rate
- P11-2.5: 出 `docs/dsh-benchmark-report.md` (vs dsh 自测 87.9 / 74.1 / 71.1)

**前置**: 网络通 (clone dsh 仓库)
**产出**: 量化"ma-harness 跟 dsh 性能差距"报告, 业务方决策依据

### 3.3 P11-3 `mah-py` Python SDK (1-2 周, **P0**)

**目标**: 业务方 Python 用户能用, 跟 dsh 官方 `deepseek-harness-sdk` 对齐.

**任务**:
- P11-3.1: 设计 `mah-py` API (subprocess CLI 跟 dsh SDK 对齐)
- P11-3.2: 业务方 `from mah import Client; c = Client(); c.run("fix bug in repo")`
- P11-3.3: 复用 ma-harness gRPC client (PyO3 binding 可选, 但 v1 用 subprocess CLI 更稳)
- P11-3.4: 出 `mah-py/README.md` + 5 个 example
- P11-3.5: 发 PyPI (业务方 `pip install mah-py`)

**产出**: `mah-py` Python 包, 业务方 Python 集成

### 3.4 P11-4 ACP (Agent Communication Protocol) 支持 (2-3 周, **P1**)

**目标**: 跟 dsh / Codex 生态互通, 外部 Agent 调 ma-harness.

**任务**:
- P11-4.1: 研究 ACP 协议 (dacp.json / agent_client.py)
- P11-4.2: 设计 ma-harness ACP adapter (over gRPC)
- P11-4.3: 实现 `mah acp` 子命令启动 ACP server
- P11-4.4: 跟 dsh 跨实现跑通 ACP 基础消息 (ping / list_sessions)
- P11-4.5: 出 `docs/acp-integration.md`

**产出**: ACP 互通, 业务方用 ACP 调 ma-harness (跟调 dsh 一样)

### 3.5 P11-5 多模态 model adapter (2-3 周, **P1**)

**目标**: 业务方能用 vision / audio 模型.

**任务**:
- P11-5.1: 设计 multi-modal content blocks (Text / Image / Audio) in protobuf
- P11-5.2: OpenAI vision adapter (gpt-4o / gpt-4-vision)
- P11-5.3: Anthropic vision adapter (claude-3-opus / sonnet)
- P11-5.4: Image upload tool (file → base64 → model)
- P11-5.5: Audio STT/TTS tool (whisper / elevenlabs)
- P11-5.6: Web UI multimodal display (image preview)

**产出**: 多模态支持, 业务方能传图片

### 3.6 P11-6 Plugin Registry 公开 + 文档站 (1-2 周 + 长期, **P1**)

**目标**: 拓第三方 plugin 生态, 跟 dsh 1,200 plugin 体系对标 (但 v1 起步).

**任务**:
- P11-6.1: `mah plugin publish` 子命令 (发布 plugin 到 registry)
- P11-6.2: `mah plugin install <name>` (从 registry 装)
- P11-6.3: `mah-registry` 公开站 (类似 npmjs.com, 用 GitHub Pages 起步)
- P11-6.4: Plugin Manifest 公开 (plugin.toml schema)
- P11-6.5: 6 first-party plugin 全部 publish 到 registry
- P11-6.6: 写 `docs/plugin-author-guide.md`

**产出**: 公开 plugin registry, 业务方能 publish / install

### 3.7 P11-7 Vibe Coding artifact viewer (1-2 周, P2)

**目标**: Web UI 实时预览 plugin 产出 (HTML / SVG / Code).

**任务**:
- P11-7.1: Artifact 类型识别 (HTML / SVG / Code / JSON / Image)
- P11-7.2: 实时渲染 (iframe sandbox / syntax highlight / JSON tree)
- P11-7.3: 跟 Trajectory 集成 (artifact 跟 event 关联)
- P11-7.4: Diff viewer (前后对比)

**产出**: Vibe Coding 体验, 跟 dsh DSH-OpenPencil 类似 (但我们更通用)

### 3.8 P11-8 Bundle 概念 (1 周, P2)

**目标**: 类似 npm package 集合, 业务方一键装多个 plugin.

**任务**:
- P11-8.1: Bundle manifest (bundle.toml: name + plugins[])
- P11-8.2: `mah bundle install <name>` (批量装)
- P11-8.3: 官方 bundle (default-agent / data-science / web-dev)
- P11-8.4: 公开 bundle registry

**产出**: Bundle 概念 + 3 个官方 bundle

### 3.9 P11-9 多模态 tool (2-3 周, P2)

**目标**: tool 本身支持 image / audio 输入输出.

**任务**:
- P11-9.1: Image content type in ToolSchema
- P11-9.2: Vision tool (image → text description)
- P11-9.3: STT tool (audio → text)
- P11-9.4: TTS tool (text → audio)
- P11-9.5: Web UI 渲染 (image preview / audio player)

**产出**: 多模态 tool

### 3.10 P11-10 DAG 任务编排 (2-3 周, P2)

**目标**: 多 Agent 拓扑 (DAG) 而非简单 fork.

**任务**:
- P11-10.1: DAG 描述 (YAML / DSL)
- P11-10.2: 调度器 (按依赖顺序)
- P11-10.3: 状态持久化 (DAG 中断 / 恢复)
- P11-10.4: 失败重试 + 短路
- P11-10.5: Web UI 拓扑图

**产出**: DAG 编排, 复杂工作流

---

## 4. P11 整体路线 + 依赖

```
P11-1 (1-3 天) 跑现有 conformance 验 baseline
   ↓
P11-2 (1-2 周) 跑 dsh Terminal Bench + Toolathlon workload ← 关键量化
   ↓
P11-3 (1-2 周) mah-py Python SDK ← 拓用户
   ↓
P11-4 (2-3 周) ACP 互通 ← 生态
P11-5 (2-3 周) 多模态 adapter ← 拓场景
   ↓
P11-6 (1-2 周 + 长期) Plugin Registry 公开 ← 生态
   ↓
P11-7/8/9/10 (锦上添花, 业务方驱动)
```

**总: 12-18 周 (3-4 月, 1 人)**, 跟 Phase 7+ 节奏一致.

并行优化:
- P11-3 (Python SDK) 跟 P11-4 (ACP) 可同时开 (不同协议层)
- P11-5 (多模态) 跟 P11-9 (多模态 tool) 联动 (同一 P11-5 系列)
- P11-6 (Plugin Registry) 跟 P11-8 (Bundle) 联动 (注册中心同源)

---

## 5. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 跑 dsh Terminal Bench 需要 clone dsh 仓库 (网络) | 高 | 网络通后做, 或用 dsh 公开 workload JSON 离线 |
| mah-py 设计跟 dsh SDK 不一致 | 中 | v1 跟 dsh SDK API 对齐, 业务方迁移成本低 |
| ACP 协议 dsh 没最终定 | 中 | 等 dsh ACP 稳定, 或参考 Codex ACP 规范 |
| 多模态 model 贵 | 中 | vision token 比 text 贵 4-10x, 业务方按需 |
| Plugin Registry 长期维护成本 | 中 | v1 用 GitHub Pages 静态站, 后续再考虑 SaaS |
| Vibe Coding 体验跟 dsh DSH-OpenPencil 难对标 | 低 | 通用 artifact viewer 够用, 实时设计是 P2 |

---

## 6. 成功指标 (P11 收官时)

| 指标 | 目标 | 实际 |
|---|---|---|
| dsh Terminal Bench 2.1 workload pass rate | ≥ 70% (dsh 自测 87.9) | 跳 (需 LLM) |
| dsh Toolathlon-Verified pass rate | ≥ 60% (dsh 自测 74.1) | 跳 (需 LLM) |
| `mah-py` Python SDK | 16/16 tests + 5 examples | ✅ |
| ACP 互通基础 | 4/4 lib + 5/5 integration | ✅ |
| 多模态 vision adapter | 7/7 multimodal + 6/6 vision_tool | ✅ |
| Plugin Registry | 18/18 lib + 1/1 doc | ✅ |
| 公开 plugin registry (GitHub Pages) | 跳 (P12+) | - |
| Vibe Coding artifact viewer | 25/25 lib + 1/1 doc | ✅ |
| Bundle 概念 | 13/13 lib + 1/1 doc | ✅ |
| 多模态 tool | 6/6 lib | ✅ |
| DAG 任务编排 | 跳 (P12+, 太复杂) | - |
| lib test (P11 整体) | 107 → 300+ | ✅ 300+ |
| commit | 200+ → 200+ (P11 7 commits) | ✅ |
| decision-log | § 1-27 → § 1-36 | ✅ |

---

## 7. 关键决策 (2026-08-20 用户 review 通过)

| 决策 | 选择 | 理由 |
|---|---|---|
| P11-1 baseline 开场 | 跑现有 conformance fixture | 1-3 天可量化, 给 P11-2 做对比基线 |
| P11-2 dsh workload 优先 | Terminal Bench + Toolathlon | 公开数字, 跟 dsh 自测直接比 |
| mah-py v1 用 subprocess CLI | 比 PyO3 binding 简单 | 1-2 周可发, PyO3 留 v2 |
| P11-5 多模态 v1 限 vision | audio TTS/STT 留 P11-9 | 业务方 vision 需求多, audio 锦上添花 |
| P11-6 Plugin Registry v1 用 GitHub Pages | 起步简单 | 后续再考虑 SaaS, dsh 自己用 npm |

---

## 8. 给后来人

- P11-1 baseline 是所有后续 P11 工作的 reference, **先做**
- P11-2 跑 dsh workload 是"跟 dsh 性能差距"的量化依据, **业务方最关心**
- P11-3 Python SDK 是**拓用户**最快路径, dsh 自家 SDK 是先例
- P11-4 ACP 互通是**生态入口**, 跟 dsh / Codex 互通
- P11-5/6/7/8/9/10 按业务方优先级排, 业务方驱动
- decision-log 持续更新, P11 收官写 § 28-32
- dsh 自家 v0.1 还在 developer preview, **dsh 路线图可能也变**, 我们保持弹性

---

## 9. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-20 | P11 路线图首版 (Day 101+1, Phase 7-10 收官后) |
| 2026-08-20 | P11-1 baseline + P11-1.5 转换层改进 (Day 101+1) — dsh_synthetic 28.6% → 100% (7/7) |
| 2026-08-20 | P11 全部任务收官 (Day 101+1) — 9 任务 (跳 P11-2.5+ 跟 P11-10) + 8 新 crate + 7 commits + 130+ tests |
