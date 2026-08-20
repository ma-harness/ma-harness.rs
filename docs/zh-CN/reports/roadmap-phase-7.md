# ma-harness.rs Phase 7+ 路线图 (2026-08-19 / Day 101+)

> **目标**: 跟 deepseek-harness 功能对齐, 同时保持我们 "production-grade Rust 重写" 的差异化定位
> **周期**: 6-8 周 (专注 Phase 7, 不切题)
> **决策日期**: 2026-08-19 (用户 review 通过, 4 个 P0)



[English](../../roadmap-phase-7.md) — coming soon. 中文为主.


---

## 0. 背景与动机

Phase 6 收官 (P6-1 mah run-stream / P6-2 OpenAI SSE / P6-3 Anthropic SSE / P6-4 streaming perf / P6-5 TUI 增强 / P6-6 salvo 0.93 / P6-7 salvo 0.95), 累计 130+ commit, 303 lib test, salvo 0.95 latest, rustc 1.94 latest。

但跟 deepseek-harness (dsh) v0.1.0-rc.5 (2026-08-13 开源, 5 万 Star) 对比, 仍有显著功能差距, 业务方体验落后。Phase 7+ 路线图围绕**借鉴 dsh 长处 + 发挥我们 Rust 优势**展开。

---

## 1. 整体路线 (按业务价值 + 工作量排优先级)

### Phase 7: 基础 + Web UI + 审批 (6-8 周)

**P7-0: 技术债清理 (1 天)**
- P7-10.6: 修 4 个 pre-existing broken test
  - `ma-harness-plugin-macro/tests/macros_compile.rs` trybuild (缺 tokio dev-dep)
  - `plugins/ma-harness-plugin-hello/tests/end_to_end.rs:18` HelloService::name trait scope
  - `crates/ma-harness-conformance/tests/smoke.rs:213` FixtureEvent not found
  - `crates/ma-harness-cordis/src/key.rs:104` CtxKey<T>::new doctest should_panic 不 panic

**P7-1: Web UI (2-3 周) — 关键差异化**
- P7-1.1: **React + Vite + TypeScript** 框架选型 (用户 2026-08-19 确认)
- P7-1.2: Web UI 骨架 (port 3080, Vite dev server + proxy → tonic server)
- P7-1.3: Session 列表 + Detail view (gRPC-web)
- P7-1.4: Trajectory 视图 (System/User/Assistant/Tool 时间线)
- P7-1.5: Token 监控 widget (首延迟/吞吐/缓存命中)
- P7-1.6: 工作区选择 + 启动 (Settings → Workspace)
- P7-1.7: Session 创建 + 实时事件推送 (gRPC streaming → SSE → UI)
- P7-1.8: 集成测试 (Playwright e2e)

**P7-2: 审批流程 (1 周) — 安全关键**
- P7-2.1: 设计 `ctx.approval` 服务 trait
- P7-2.2: cordis 加 `ApprovalPolicy` 枚举 (Never / Ask / Always)
- P7-2.3: 工具执行管道 pre-execute hook (5 阶段)
- P7-2.4: TUI approval 提示 (y/n)
- P7-2.5: HTTP approval 端点 (POST /v1/approvals/{tool_call_id})
- P7-2.6: 审批审计 log
- P7-2.7: 集成测试 (5 scenarios)

**P7-3: 工具执行管道升级 (1.5 周) — 配合 P7-2**
- P7-3.1: 重构 `ctx.tools.execute` 走 7 阶段管道
- P7-3.2: Timeout + Retry 配置
- P7-3.3: Pre-execute hook (改写 tool call 参数)
- P7-3.4: Post-execute hook (修改结果或追加上下文)
- P7-3.5: Result 规范化

**P7-4: 子代理 fork (1 周)**
- P7-4.1: `SubagentSpec::Fork { inherit_history: bool }` 设计
- P7-4.2: Fork 模式从 parent 复制 events
- P7-4.3: Ralph handoff 协议
- P7-4.4: subagent plugin 升级支持 fork

**P7-5: Trajectory 视图增强 (1 周) — 跟 P7-1.4 配套**
- P7-5.1: TUI Trajectory 增强 (多列布局, 时间线)
- P7-5.2: 事件类型着色
- P7-5.3: 事件 payload 展开
- P7-5.4: Event 搜索
- P7-5.5: 持久化筛选

### Phase 8: 优化 + 多语言 (4-6 周)

**P8-1: 上下文压缩 (1 周)**
- P8-1.1: EventLog 压缩策略设计
- P8-1.2: LLM 驱动 summary 压缩
- P8-1.3: derive_messages 优化
- P8-1.4: 压缩审计 log
- P8-1.5: 集成测试

**P8-2: Token 监控 (3 天)**
- P8-2.1: 采集 streaming 指标
- P8-2.2: 缓存命中率统计
- P8-2.3: TUI metrics widget
- P8-2.4: HTTP `/v1/metrics` Prometheus endpoint

**P8-3: 多模型适配扩展 (1 周)**
- P8-3.1: Azure OpenAI 适配
- P8-3.2: DeepSeek 适配 (内置 v4-pro/flash)
- P8-3.3: Bedrock/Vertex 适配 (业务方驱动)

**P8-4: 模式扩展 (1 周)**
- P8-4.1: Minimal 模式 (config 切只 bash+edit)
- P8-4.2: Profile 隔离 (类似 dsh profiles)
- P8-4.3: PTC 模式 (业务方驱动, 优先级低)

### Phase 9: 架构 + 创新 (4-6 周)

**P9-1: Capability Seam 三角色 (2 周)**
- P9-1.1: `ServiceDef / ServiceProvider / Consumer` 三个 trait
- P9-1.2: cordis 重构支持三角色
- P9-1.3: LLM adapter 走 Capability Seam
- P9-1.4: Session store 走 Capability Seam
- P9-1.5: 回归测试

**P9-2: Creator 模式 (1 周)**
- P9-2.1: 运行时自省机制
- P9-2.2: 内存试插件
- P9-2.3: preset 编写指导

**P9-3: 内部 SDK 集成深化** (业务方驱动)
- 跟 GM 工具 (活动/道具/走马灯) 深度集成
- 业务方提需求触发

---

## 2. P7-1 Web UI 详细规划

### 2.1 技术选型

| 层 | 选型 | 理由 |
|---|---|---|
| **前端框架** | React 18 + TypeScript 5 | 生态最熟, 文档最多, 招人容易, 业务方常见 |
| **构建工具** | Vite 5 | dev server 快, HMR 流畅, WebAssembly 支持 |
| **路由** | React Router 6 | 标准选择, dsh/TRAE/Claude Code 都用类似 |
| **状态管理** | Zustand | 轻量, 适合 dashboard, 不需要 Redux 复杂度 |
| **UI 组件** | Radix UI + TailwindCSS | 无样式 baseline, Tailwind 强定制 |
| **数据获取** | TanStack Query (React Query) | cache + refetch + 实时订阅 |
| **WebSocket/SSE** | 原生 EventSource + fetch | 简单, gRPC streaming 桥接 |
| **gRPC-Web 桥** | tonic-web (tonic 0.12 自带) | 类型化, 跟现有 proto 复用 |
| **测试** | Playwright e2e + Vitest unit | 跟 dsh 一致 |

### 2.2 架构

```
ma-harness/
├── crates/
│   ├── ma-harness-server/        (gRPC + HTTP, 加 tonic-web)
│   └── ma-harness-web/           (新增 — React 前端, port 3080)
└── ui/                           (新增 — React 项目根)
    ├── src/
    │   ├── routes/
    │   │   ├── Sessions.tsx     (P7-1.3)
    │   │   ├── SessionDetail.tsx (P7-1.3 + P7-1.4 Trajectory)
    │   │   └── Settings.tsx     (P7-1.6 Workspace + API Key)
    │   ├── components/
    │   │   ├── EventStream.tsx  (P7-1.7 实时推送)
    │   │   ├── Trajectory.tsx   (P7-1.4)
    │   │   └── MetricsWidget.tsx (P7-1.5)
    │   ├── api/
    │   │   └── grpc.ts          (tonic-web client wrapper)
    │   ├── App.tsx
    │   └── main.tsx
    ├── vite.config.ts            (port 3080 + proxy /api → tonic :50050)
    ├── package.json
    └── tsconfig.json
```

### 2.3 gRPC-Web 桥 (P7-1.2 关键)

```rust
// crates/ma-harness-server/src/web_bridge.rs (新增)
use tonic_web::enable;

pub fn add_grpc_web(router: Router) -> Router {
    // 把现有 tonic gRPC 路由暴露给浏览器
    enable(router)
}
```

Vite dev proxy:
```typescript
// ui/vite.config.ts
export default {
  server: {
    port: 3080,
    proxy: {
      '/api': {
        target: 'http://localhost:50050',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
}
```

### 2.4 任务拆分 (P7-1.x)

| 任务 | 工作量 | 优先级 |
|---|---|---|
| P7-1.1 React + Vite + TypeScript 项目初始化 | 0.5 天 | P0 |
| P7-1.2 tonic-web 集成 + Vite proxy 配置 | 1 天 | P0 |
| P7-1.3 Session 列表 + Detail view (gRPC client) | 2 天 | P0 |
| P7-1.4 Trajectory 视图 (事件流 + 着色 + 展开) | 2 天 | P1 |
| P7-1.5 Token 监控 widget (首延迟 / 吞吐 / 缓存) | 1 天 | P2 |
| P7-1.6 Settings 页面 (Workspace + API Key) | 1 天 | P1 |
| P7-1.7 Session 创建 + 实时事件推送 (SSE) | 2 天 | P0 |
| P7-1.8 Playwright e2e 测试 (5 scenarios) | 1 天 | P1 |
| **小计** | **2-3 周 (1 人)** | |

---

## 3. P7-2 审批流程详细规划

### 3.1 设计

```rust
// crates/ma-harness-cordis/src/approval.rs (新增)
pub trait ApprovalService: Send + Sync {
    /// 工具调用前审批
    async fn request_approval(
        &self,
        ctx: &Context,
        request: &ApprovalRequest,
    ) -> Result<ApprovalDecision>;
}

pub struct ApprovalRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub risk_level: RiskLevel,  // Low / Medium / High / Critical
    pub context: String,        // human-readable 描述
}

pub enum ApprovalDecision {
    Approved,
    Denied { reason: String },
    AutoApprove,  // policy 决定
}

pub enum ApprovalPolicy {
    Never,         // 不审批
    Ask,           // 逐次询问
    Always,        // 永远 ask (debug 用)
    Whitelist(Vec<String>),  // 白名单内 auto-approve
}

pub enum RiskLevel {
    Low,      // read-only, log
    Medium,   // write workspace
    High,     // delete / system
    Critical, // 配置变更, 安全敏感
}
```

### 3.2 任务拆分 (P7-2.x)

| 任务 | 工作量 | 优先级 |
|---|---|---|
| P7-2.1 ApprovalService trait + 类型定义 | 0.5 天 | P0 |
| P7-2.2 cordis 加 ApprovalPolicy 枚举 | 0.5 天 | P0 |
| P7-2.3 工具执行管道 pre-execute hook 集成 | 1 天 | P0 |
| P7-2.4 TUI approval 提示 (y/n + reason) | 1 天 | P0 |
| P7-2.5 HTTP `/v1/approvals/{tool_call_id}` 端点 | 0.5 天 | P1 |
| P7-2.6 审批审计 log (who/when/decision/reason) | 0.5 天 | P1 |
| P7-2.7 集成测试 (5 scenarios) | 0.5 天 | P1 |
| **小计** | **1 周 (1 人)** | |

### 3.3 默认风险等级

| 工具 | 风险等级 | 审批要求 |
|---|---|---|
| hello | Low | Never auto-approve |
| bash (read) | Low | Never auto-approve |
| bash (write/delete) | High | Ask |
| fs (read) | Low | Never |
| fs (write) | Medium | Ask |
| fs (delete) | High | Ask |
| web (fetch) | Medium | Whitelist (业务方自配) |
| subagent (spawn) | Medium | Whitelist (deep agent) |
| subagent (fork) | High | Ask |
| skill (load) | Low | Never |
| cordis (plugin mgmt) | Critical | Always ask |

---

## 4. P7-3 工具执行管道升级

### 4.1 当前状态 vs 目标

| 阶段 | 当前 | 目标 (P7-3) |
|---|---|---|
| pre-execute (策略/守卫) | ❌ | ✅ |
| 单调守卫 (deny/abstain) | ❌ | ✅ |
| ctx.approval (审批) | ❌ | ✅ (P7-2) |
| tools/execute (超时/重试) | ⚠️ 无 timeout | ✅ timeout + retry |
| 工具执行体 | ✅ | ✅ |
| 文件系统守卫 | ❌ | ✅ |
| tools/post-execute | ❌ | ✅ (改写/追加) |
| 结果规范化 (finalizeContent) | ❌ | ✅ |
| tools/result (不可变) | ✅ | ✅ |

### 4.2 任务拆分

| 任务 | 工作量 |
|---|---|
| P7-3.1 重构 `ctx.tools.execute` 走 7 阶段管道 | 2 天 |
| P7-3.2 Timeout + Retry 配置 (per-tool config) | 1 天 |
| P7-3.3 Pre-execute hook (改写 tool call 参数) | 1 天 |
| P7-3.4 Post-execute hook (修改结果或追加上下文) | 1 天 |
| P7-3.5 Result 规范化 (统一 Markdown 表格 / 错误格式) | 0.5 天 |
| **小计** | **1.5 周 (1 人)** |

---

## 5. P7-4 子代理 fork 详细规划

### 5.1 spawn vs fork 区别

| 维度 | spawn (现有) | fork (P7-4 新增) |
|---|---|---|
| 上下文 | 新会话 | 继承 parent 历史 events |
| 工作区 | 共享 | 共享 |
| 适用场景 | 并行独立任务 | 延续性任务, 探索分支 |
| 状态传递 | 显式 handoff | 自动继承 + handoff |

### 5.2 任务拆分

| 任务 | 工作量 |
|---|---|
| P7-4.1 `SubagentSpec::Fork { inherit_history: bool }` 设计 | 0.5 天 |
| P7-4.2 Fork 模式从 parent session 复制 events | 1 天 |
| P7-4.3 Ralph handoff 协议 (结构化报告) | 1 天 |
| P7-4.4 subagent plugin 升级支持 fork | 1 天 |
| **小计** | **1 周 (1 人)** |

---

## 6. P7-5 Trajectory 视图增强

### 6.1 当前 vs 目标

| 维度 | TUI Detail 现状 | P7-5 目标 |
|---|---|---|
| 布局 | 单 session event list | 多列 (Session 列表 / Event 时间线 / Event 详情) |
| 着色 | ❌ | ✅ Error 红 / Warn 黄 / Info 蓝 |
| 折叠 | ❌ | ✅ j/l 折叠/展开 payload |
| 搜索 | ❌ | ✅ grep by type / payload |
| 持久化筛选 | ❌ | ✅ 时间范围 / 类型 / session_id |

### 6.2 任务拆分

| 任务 | 工作量 |
|---|---|
| P7-5.1 TUI Trajectory 增强 (多列布局, 时间线) | 2 天 |
| P7-5.2 事件类型着色 | 0.5 天 |
| P7-5.3 事件 payload 展开 (j/l) | 1 天 |
| P7-5.4 Event 搜索 (按 session_id / type / payload grep) | 1 天 |
| P7-5.5 持久化筛选 (按时间范围/类型) | 0.5 天 |
| **小计** | **1 周 (1 人)** |

---

## 7. Phase 7 总工作量和依赖图

```
P7-0 (1 day)
   ↓
P7-1 Web UI (2-3 weeks)
   ├── P7-1.1 (0.5d)
   ├── P7-1.2 (1d)
   ├── P7-1.3 (2d)         ← 核心: Session list + Detail
   ├── P7-1.7 (2d)         ← 核心: 实时事件推送
   ├── P7-1.4 (2d)         ← Trajectory 视图
   ├── P7-1.5 (1d)
   ├── P7-1.6 (1d)
   └── P7-1.8 (1d)         ← Playwright e2e

P7-2 Approval (1 week)
   ├── P7-2.1 (0.5d)       ← trait
   ├── P7-2.2 (0.5d)       ← policy enum
   ├── P7-2.3 (1d)         ← 核心: pre-execute hook
   ├── P7-2.4 (1d)         ← TUI prompt
   ├── P7-2.5 (0.5d)
   ├── P7-2.6 (0.5d)
   └── P7-2.7 (0.5d)

P7-3 Tool Pipeline (1.5 weeks)   ← depends on P7-2
   ├── P7-3.1 (2d)         ← 核心: 7-stage pipeline
   ├── P7-3.2 (1d)
   ├── P7-3.3 (1d)
   ├── P7-3.4 (1d)
   └── P7-3.5 (0.5d)

P7-4 Subagent Fork (1 week)
   ├── P7-4.1 (0.5d)
   ├── P7-4.2 (1d)
   ├── P7-4.3 (1d)
   └── P7-4.4 (1d)

P7-5 Trajectory (1 week)        ← 跟 P7-1.4 配套
   ├── P7-5.1 (2d)
   ├── P7-5.2 (0.5d)
   ├── P7-5.3 (1d)
   ├── P7-5.4 (1d)
   └── P7-5.5 (0.5d)
```

**总: 6.5-8.5 周 (1 人)**, 跟用户预期 6-8 周吻合。

并行优化:
- P7-0 (1 天) 跟 P7-1.1 (0.5 天) 可同时开 (P7-0 不依赖 P7-1)
- P7-5.1-5.5 跟 P7-1.4 配套, 可放最后

建议执行顺序 (1 人串行):
1. P7-0 (1 天) — 清技术债
2. P7-1.1 + P7-1.2 (1.5 天) — Web UI 起步
3. P7-2.1 + P7-2.2 + P7-2.3 (2 天) — Approval 起步
4. P7-1.3 (2 天) + P7-2.4 (1 天) — Session list + TUI approval
5. P7-1.7 (2 天) — 实时事件推送
6. P7-1.4 (2 天) — Trajectory
7. P7-2.5 + P7-2.6 + P7-2.7 (1.5 天) — Approval HTTP + test
8. P7-3.1 (2 天) — 核心管道
9. P7-3.2 + P7-3.3 + P7-3.4 + P7-3.5 (3.5 天) — hooks + 规范化
10. P7-4.1 → 4.4 (3.5 天) — Subagent fork
11. P7-1.5 + P7-1.6 + P7-1.8 (3 天) — 收尾
12. P7-5.1 → 5.5 (5 天) — Trajectory 增强
13. decision-log § 22-27 收官

---

## 8. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| Web UI 选 React 但业务方偏好 Svelte | 中 | 选 React 生态最广, 招人容易, 业务方常见 |
| gRPC-Web 桥配置复杂 | 中 | 跟现有 tonic 复用, 单文件改动 |
| 审批流程防误删效果有限 | 低 | 配合 P7-3 工具管道 + 沙箱 landlock 3 层防护 |
| P7-3 工具管道大改破坏现有 plugin | 中 | 增量改造, 每步保留 backward-compat |
| P7-4 fork 实现复杂 (event log 复制) | 中 | 简单实现: SQLite ATTACH DATABASE 复制 |
| P7-5 跟 P7-1.4 Trajectory 重复 | 低 | P7-1.4 简版, P7-5 完整版 |
| Phase 7 完成时 dsh 已发 v0.2 | 中 | 路线图保持弹性, 业务方驱动优先级调整 |

---

## 9. 成功指标 (Phase 7 收官时)

| 指标 | 目标 |
|---|---|
| 累计 lib test | 303 → 380+ (+77) |
| 累计 commit | 130+ → 200+ |
| 累计 doc section | decision-log § 1-21 → § 1-27 |
| Web UI 上线 | port 3080 可访问, Session 列表/Detail/Trajectory 完整 |
| 审批流程上线 | 11 个内置工具全部走 5 阶段管道, 默认策略生效 |
| 子代理 fork | subagent plugin 支持 `--mode fork` |
| Trajectory 视图 | TUI 多列布局, Web UI 时间线 |
| 测试 | 303/303 + 77 新 test, 0 fail |
| 编译 | 0 error, < 2min 全量, salvo 0.95 + rustc 1.94 |

---

## 10. 关键决策 (2026-08-19 用户 review 通过)

| 决策 | 选择 | 理由 |
|---|---|---|
| Web UI 优先级 | P0 - Phase 7 立刻做 | 跟 dsh 差距最大, 业务方体验跨档 |
| Web UI 框架 | React + Vite + TypeScript | 生态最熟, 招人容易, 文档最多 |
| 审批流程优先级 | P0 - Phase 7 立刻做 | 防 AI 误删, 安全关键 |
| 整体节奏 | 专注 Phase 7, 6-8 周不切题 | 业务方需求插队会破坏节奏 |

---

## 11. Phase 8+ 简化路线 (等 Phase 7 收官 review)

- **Phase 8** (4-6 周): 上下文压缩 / Token 监控 / 多模型扩展 / 模式扩展
- **Phase 9** (4-6 周): Capability Seam / Creator 模式 / 内部 SDK 集成深化
- **Phase 10+** (业务方驱动): GM 工具深度集成 / 性能回归 / 文档

---

## 12. 给后来人

- 路线图按业务价值 + 工作量排, 优先级可调但 P7-0/1/2 不能动
- Web UI 是 dsh 关键差异化, 我们 P7-1 必须 6 周内上线
- 审批流程防误删是 GM 工具救命稻草, 优先级 P0
- 工具管道 P7-3 配合 P7-2 走 7 阶段, 是 3 层防护的最后一道
- 子代理 fork 跟 spawn 互补, 长任务必备
- Trajectory 视图跟 dsh 一样是 debugging 关键
- 路线图保持弹性, 业务方有强需求触发可调整优先级
- decision-log 持续更新, 路线图变更必写 decision
