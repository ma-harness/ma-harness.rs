//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-guard`
//! **Crate ident** (`use` 路径): `ma_harness_guard`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-guard = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_guard::{LoopGuard, MaxStepsGuard, LoopEvent, GuardChain};
//! use std::sync::Arc;
//!
//! let mut chain = GuardChain::new();
//! chain.add_guard(Arc::new(MaxStepsGuard::new(50))).await;
//!
//! let event = LoopEvent::StepCompleted;
//! let decision = chain.observe(&event).await;
//! if decision.is_abort() {
//!     tracing::warn!("Agent aborted by guard: {}", decision.reason().unwrap_or(""));
//! }
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-guard
//!
//! # 设计 (Design) — P14.11
//!
//! **目标**: 抽象 loop-hygiene (跟 dsh `packages/guard/` 1:1 对等).
//! 业务方
//! - 用 `LoopGuard` 监听 `LoopEvent` (StepCompleted / ToolCalled / ToolResult)
//! - `MaxStepsGuard` 限制 N 步无进展 → abort
//! - `RepeatedArgsGuard` 检测同 tool 同 args 重复 N 次 → abort
//! - `GuardChain` 串多个 guard, 任何一个 abort 就 abort
//!
//! **背景**: 见 [dsh-feature-parity-table §11] "conformance on real benchmarks" (待 P17+).
//! ma-harness 之前有 `ToolSchema::Timeout` 但无 loop detection, infinite loop 风险.
//!
//! **核心抽象**:
//! - [`LoopEvent`] enum (StepCompleted / ToolCalled / ToolResult / StepStarted)
//! - [`GuardDecision`] (Continue / Abort with reason)
//! - [`LoopGuard`] trait (observe + provider_name + reset)
//! - [`MaxStepsGuard`] (P14.11.1 主交付)
//! - [`RepeatedArgsGuard`] (P14.11.1 主交付, args hash)
//! - [`GuardChain`] (in-memory 串行组合)
//!
//! **6 质量属性**:
//! - 可复用: LoopGuard trait, 业务方可加 ToolTimeoutGuard / LlmCostGuard
//! - 可维护: 模块化分块, event / decision / guard / chain 集中 lib.rs
//! - 鲁棒: 边界 case 显式 (steps=0, args hash collision, empty tool name)
//! - 安全: 不 eval event payload, 静态 string
//! - 可测: 7 测试覆盖 happy / abort / chain / reset
//! - 可扩展: GuardChain 可加任意 guard
//!
//! # 限制 (Limitations) — P14.11.1
//!
//! - 仅 2 guard (MaxSteps + RepeatedArgs), ToolTimeoutGuard 留 P14.11.2
//! - guard 决策不自动 trigger abort (业务方查 GuardDecision.is_abort())
//! - LoopEvent 简化版 (P14.11.2 加 tool_call_id / duration / 等)
//!
//! [dsh-feature-parity-table §11]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#11-conformance--real-benchmarks

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

// ============================================================================
// LoopEvent
// ============================================================================

/// Loop event (业务方喂给 guard).
///
/// **业务方场景**: agent loop 每步 / 每次 tool call 触发事件, guard 监听决策.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LoopEvent {
    /// Step 开始了 (agent turn 开始)
    StepStarted,
    /// Step 完成了 (model 返回 assistant message)
    StepCompleted,
    /// Tool call 触发了
    ToolCalled {
        /// Tool 名 (e.g. "bash_run")
        tool_name: String,
        /// Tool 参数 (JSON string, business 用 serde_json::to_string)
        args_hash: String,
    },
    /// Tool call 完成了
    ToolResult {
        /// Tool 名
        tool_name: String,
        /// 成功 / 失败 / 错误
        success: bool,
    },
}

// ============================================================================
// GuardDecision
// ============================================================================

/// Guard 决策.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    /// 继续 (没触发 abort 条件)
    Continue,
    /// 中止 (业务方应 stop agent loop)
    Abort {
        /// 中止原因
        reason: String,
    },
}

impl GuardDecision {
    /// 是否 abort
    pub fn is_abort(&self) -> bool {
        matches!(self, GuardDecision::Abort { .. })
    }

    /// 是否 continue
    pub fn is_continue(&self) -> bool {
        matches!(self, GuardDecision::Continue)
    }

    /// 拿 abort reason (None if Continue)
    pub fn reason(&self) -> Option<&str> {
        match self {
            GuardDecision::Continue => None,
            GuardDecision::Abort { reason } => Some(reason),
        }
    }
}

// ============================================================================
// GuardError
// ============================================================================

/// Guard capability 错误.
#[derive(Debug, Error)]
pub enum GuardError {
    /// Validation failed
    #[error("guard validation failed: {0}")]
    Validation(String),
}

// ============================================================================
// LoopGuard trait
// ============================================================================

/// Loop guard (跟 dsh `packages/guard/` 对等).
///
/// **业务方用**: 实现 `observe(event)` 决定 Continue / Abort, agent loop 每步调一下.
#[async_trait]
pub trait LoopGuard: Send + Sync + 'static {
    /// 观察 event, 返回 Continue / Abort
    async fn observe(&self, event: &LoopEvent) -> GuardDecision;

    /// 重置 state (新 session 开始时调)
    async fn reset(&self);

    /// Provider 标识 (日志 / 调试)
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// MaxStepsGuard (P14.11.1 主交付)
// ============================================================================

/// Max steps guard: 累计 step_completed 数, 超过 max 触发 abort.
///
/// **业务方场景**: agent 跑 50 步还没 progress, 终止避免 infinite loop.
pub struct MaxStepsGuard {
    max_steps: usize,
    state: Mutex<MaxStepsState>,
}

struct MaxStepsState {
    step_count: usize,
}

impl MaxStepsGuard {
    /// 创建一个 MaxStepsGuard
    ///
    /// # Panics
    /// max_steps = 0 时 panic (业务方必须设个合理上限)
    pub fn new(max_steps: usize) -> Self {
        assert!(max_steps > 0, "max_steps must be > 0");
        Self {
            max_steps,
            state: Mutex::new(MaxStepsState { step_count: 0 }),
        }
    }

    /// 拿 max_steps 配置
    pub fn max_steps(&self) -> usize {
        self.max_steps
    }
}

#[async_trait]
impl LoopGuard for MaxStepsGuard {
    async fn observe(&self, event: &LoopEvent) -> GuardDecision {
        let mut state = self.state.lock().await;
        if matches!(event, LoopEvent::StepCompleted) {
            state.step_count += 1;
            if state.step_count > self.max_steps {
                return GuardDecision::Abort {
                    reason: format!(
                        "max steps exceeded: {} > {}",
                        state.step_count, self.max_steps
                    ),
                };
            }
        }
        GuardDecision::Continue
    }

    async fn reset(&self) {
        let mut state = self.state.lock().await;
        state.step_count = 0;
    }

    fn provider_name(&self) -> &'static str {
        "max-steps"
    }
}

// ============================================================================
// RepeatedArgsGuard (P14.11.1 主交付)
// ============================================================================

/// Repeated args guard: 同样 tool_name + args_hash 调 N 次触发 abort.
///
/// **算法**:
/// - 跟踪 `HashMap<(tool_name, args_hash) -> VecDeque<Instant>>`
/// - 每次 ToolCalled 事件, push timestamp
/// - deque 长度 > max_repeats → abort
pub struct RepeatedArgsGuard {
    max_repeats: usize,
    state: Mutex<RepeatedArgsState>,
}

struct RepeatedArgsState {
    calls: HashMap<(String, String), usize>,
}

impl RepeatedArgsGuard {
    /// 创建一个 RepeatedArgsGuard
    ///
    /// # Panics
    /// max_repeats = 0 时 panic
    pub fn new(max_repeats: usize) -> Self {
        assert!(max_repeats > 0, "max_repeats must be > 0");
        Self {
            max_repeats,
            state: Mutex::new(RepeatedArgsState {
                calls: HashMap::new(),
            }),
        }
    }

    /// 拿 max_repeats 配置
    pub fn max_repeats(&self) -> usize {
        self.max_repeats
    }
}

#[async_trait]
impl LoopGuard for RepeatedArgsGuard {
    async fn observe(&self, event: &LoopEvent) -> GuardDecision {
        if let LoopEvent::ToolCalled {
            tool_name,
            args_hash,
        } = event
        {
            let mut state = self.state.lock().await;
            let key = (tool_name.clone(), args_hash.clone());
            let count = state.calls.entry(key.clone()).or_insert(0);
            *count += 1;
            if *count > self.max_repeats {
                return GuardDecision::Abort {
                    reason: format!(
                        "tool {} called {} times with same args (max {})",
                        tool_name, count, self.max_repeats
                    ),
                };
            }
        }
        GuardDecision::Continue
    }

    async fn reset(&self) {
        let mut state = self.state.lock().await;
        state.calls.clear();
    }

    fn provider_name(&self) -> &'static str {
        "repeated-args"
    }
}

// ============================================================================
// GuardChain (in-memory 串行组合)
// ============================================================================

/// Guard chain (业务方 CLI / runtime 用).
///
/// **流程**: 业务方按顺序 `add_guard`, 调 `observe(event)` 走 chain,
/// 任一 guard 返回 Abort → 整个 chain Abort.
pub struct GuardChain {
    guards: Mutex<Vec<Arc<dyn LoopGuard>>>,
}

impl GuardChain {
    /// 创建一个空 chain
    pub fn new() -> Self {
        Self {
            guards: Mutex::new(Vec::new()),
        }
    }

    /// 加 guard (按调用顺序)
    pub async fn add_guard(&self, g: Arc<dyn LoopGuard>) {
        let mut guards = self.guards.lock().await;
        guards.push(g);
    }

    /// guard 数量
    pub async fn len(&self) -> usize {
        let guards = self.guards.lock().await;
        guards.len()
    }

    /// 是否空
    pub async fn is_empty(&self) -> bool {
        let guards = self.guards.lock().await;
        guards.is_empty()
    }

    /// 走 chain 观察 event (任一 guard Abort → 整个 Abort, 同时 reset 全部)
    pub async fn observe(&self, event: &LoopEvent) -> GuardDecision {
        let guards = self.guards.lock().await;
        for g in guards.iter() {
            let decision = g.observe(event).await;
            if decision.is_abort() {
                // 触发 abort, 全部 reset
                for g2 in guards.iter() {
                    g2.reset().await;
                }
                return decision;
            }
        }
        GuardDecision::Continue
    }

    /// 重置所有 guard
    pub async fn reset_all(&self) {
        let guards = self.guards.lock().await;
        for g in guards.iter() {
            g.reset().await;
        }
    }
}

impl Default for GuardChain {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Typed keys
// ============================================================================

/// Typed key: `ctx.guards` 注入的 GuardChain.
pub static GUARD_CHAIN: ma_harness_cordis::CtxKey<Arc<GuardChain>> =
    ma_harness_seam::ctx_key!("guard_chain");

// ============================================================================
// Default type aliases
// ============================================================================

/// 平台默认 guard chain (P14.11.1: MaxSteps + RepeatedArgs)
pub type DefaultGuardChain = GuardChain;

// ============================================================================
// 单元测试 (mod tests) — 8 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn max_steps_guard_allows_under_limit() {
        let guard = MaxStepsGuard::new(5);
        for _ in 0..5 {
            let d = guard.observe(&LoopEvent::StepCompleted).await;
            assert!(d.is_continue());
        }
    }

    #[tokio::test]
    async fn max_steps_guard_aborts_at_limit() {
        let guard = MaxStepsGuard::new(3);
        for _ in 0..3 {
            let d = guard.observe(&LoopEvent::StepCompleted).await;
            assert!(d.is_continue());
        }
        let d = guard.observe(&LoopEvent::StepCompleted).await;
        assert!(d.is_abort());
        assert!(d.reason().unwrap().contains("max steps exceeded"));
    }

    #[tokio::test]
    async fn max_steps_guard_reset_returns_to_initial() {
        let guard = MaxStepsGuard::new(2);
        guard.observe(&LoopEvent::StepCompleted).await;
        guard.observe(&LoopEvent::StepCompleted).await;
        guard.reset().await;
        let d = guard.observe(&LoopEvent::StepCompleted).await;
        assert!(d.is_continue(), "after reset, should allow steps again");
    }

    #[tokio::test]
    async fn max_steps_guard_ignores_non_step_events() {
        let guard = MaxStepsGuard::new(1);
        // 10 个 tool calls 不计数
        for i in 0..10 {
            let d = guard
                .observe(&LoopEvent::ToolCalled {
                    tool_name: "bash".into(),
                    args_hash: format!("{i}"),
                })
                .await;
            assert!(d.is_continue());
        }
        // 1 step 还在 limit 内
        let d = guard.observe(&LoopEvent::StepCompleted).await;
        assert!(d.is_continue());
    }

    #[tokio::test]
    async fn repeated_args_guard_aborts_on_nth_call() {
        let guard = RepeatedArgsGuard::new(2);
        let event = LoopEvent::ToolCalled {
            tool_name: "bash".into(),
            args_hash: "same_args".into(),
        };
        assert!(guard.observe(&event).await.is_continue());
        assert!(guard.observe(&event).await.is_continue());
        let d = guard.observe(&event).await;
        assert!(d.is_abort());
    }

    #[tokio::test]
    async fn repeated_args_guard_different_args_dont_count() {
        let guard = RepeatedArgsGuard::new(2);
        for i in 0..5 {
            let d = guard
                .observe(&LoopEvent::ToolCalled {
                    tool_name: "bash".into(),
                    args_hash: format!("args-{i}"),
                })
                .await;
            assert!(d.is_continue(), "different args shouldn't trigger abort");
        }
    }

    #[tokio::test]
    async fn guard_chain_propagates_first_abort() {
        let chain = GuardChain::new();
        chain.add_guard(Arc::new(MaxStepsGuard::new(2))).await;
        chain
            .add_guard(Arc::new(RepeatedArgsGuard::new(100))) // 不应触发
            .await;
        assert_eq!(chain.len().await, 2);

        // 第 1-2 步 OK
        assert!(chain.observe(&LoopEvent::StepCompleted).await.is_continue());
        assert!(chain.observe(&LoopEvent::StepCompleted).await.is_continue());
        // 第 3 步 MaxSteps abort
        let d = chain.observe(&LoopEvent::StepCompleted).await;
        assert!(d.is_abort());
        assert!(d.reason().unwrap().contains("max steps"));
    }

    #[tokio::test]
    async fn guard_chain_empty_always_continues() {
        let chain = GuardChain::new();
        assert!(chain.is_empty().await);
        for _ in 0..100 {
            assert!(chain.observe(&LoopEvent::StepCompleted).await.is_continue());
        }
    }

    #[tokio::test]
    async fn guard_chain_abort_resets_all_guards() {
        let chain = GuardChain::new();
        chain.add_guard(Arc::new(MaxStepsGuard::new(2))).await;
        chain.add_guard(Arc::new(RepeatedArgsGuard::new(2))).await;

        // 触发 MaxSteps abort
        chain.observe(&LoopEvent::StepCompleted).await;
        chain.observe(&LoopEvent::StepCompleted).await;
        let d = chain.observe(&LoopEvent::StepCompleted).await;
        assert!(d.is_abort());

        // reset 之后应该从头开始 (MaxSteps 重新允许 2 步, RepeatedArgs 重新允许 2 次)
        assert!(chain.observe(&LoopEvent::StepCompleted).await.is_continue());
        assert!(chain.observe(&LoopEvent::StepCompleted).await.is_continue());
    }

    #[test]
    #[should_panic(expected = "max_steps must be > 0")]
    fn max_steps_guard_panics_on_zero() {
        let _ = MaxStepsGuard::new(0);
    }

    #[test]
    #[should_panic(expected = "max_repeats must be > 0")]
    fn repeated_args_guard_panics_on_zero() {
        let _ = RepeatedArgsGuard::new(0);
    }

    #[test]
    fn guard_decision_continue_not_abort() {
        let d = GuardDecision::Continue;
        assert!(d.is_continue());
        assert!(!d.is_abort());
        assert!(d.reason().is_none());
    }

    #[test]
    fn guard_decision_abort_has_reason() {
        let d = GuardDecision::Abort {
            reason: "test reason".into(),
        };
        assert!(d.is_abort());
        assert!(!d.is_continue());
        assert_eq!(d.reason(), Some("test reason"));
    }
}
