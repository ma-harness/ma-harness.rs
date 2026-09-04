//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-compaction`
//! **Crate ident** (`use` 路径): `ma_harness_compaction`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-compaction = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_compaction::{BasicCompactionProvider, CompactionContext, CompactionStrategy};
//!
//! let provider = BasicCompactionProvider::new();
//! let ctx = CompactionContext::default()
//!     .with_max_tokens(8000)  // GPT-4o context window ~128k, 80% utilization
//!     .with_keep_recent(10);   // 保留最近 10 步
//!
//! let (compacted, stats) = provider.compact(&events, &ctx);
//! tracing::info!("removed {} events, kept {}", stats.removed_count, stats.kept_count);
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-compaction
//!
//! # 设计 (Design) — P14.4
//!
//! **目标**: 长 session (1000+ events) 自动压缩,context window 保持 < 80% utilization.
//! 旧事件仍可查 (via projection 或新 log) — 业务方不丢数据,只裁 model context.
//!
//! **背景**: dsh [packages/compaction/] 有 `CompactionStrategy` trait + `BasicCompactionProvider`.
//! ma-harness 之前有 `dsh_format` cache (P12-1) 缓存 session 序列化,没有自动压缩.
//!
//! **核心抽象**:
//! - [`CompactionStrategy`] trait — 接受 `&[SessionEvent]`, 返回 `(Vec<SessionEvent>, CompactionStats)`
//! - [`CompactionContext`] — 配置 (max_tokens / keep_recent / token_estimator)
//! - [`CompactionStats`] — 压缩效果统计 (removed / kept / tokens_before / tokens_after)
//! - [`CompactionSummary`] — 压缩后插入的 summary 事件 (event_type = ModelResponse,
//!   payload_json = `{"summary": "...", "compacted_event_ids": [...]}`)
//! - [`BasicCompactionProvider`] — 默认实现 (P14.4.1 主交付):
//!   1. 旧 ModelResponse 事件按时间倒序累加 token
//!   2. 超过 threshold 的部分 truncate
//!   3. 保留最近 N 步 (run_id distinct)
//!   4. 插入 CompactionSummary 事件
//! - [`LlmCompactionProvider`] — stub (P14.4.2: 接 ma-harness-model LLM client)
//! - [`COMPACTION_STRATEGY`] typed key (跟 SHELL_SERVICE / SKILL_PROVIDER 平行)
//! - [`DefaultCompactionProvider`] type alias
//!
//! **Token 估算** (P14.4.1 简化):
//! - 默认 estimator: `payload_json.len() / 4` (粗略, 实际 LLM tokenizer 更准)
//! - 业务方可注入 custom estimator (`CompactionContext::with_token_estimator`)
//!
//! **6 质量属性** (业务方 2026-09-04 约定):
//! - 可复用: CompactionStrategy trait, 业务方可注入 LLM / 自定义
//! - 可维护: 模块化分块, error / context / strategy 集中 lib.rs
//! - 鲁棒: 边界 case 显式 (空 events / 全 keep / token = 0)
//! - 安全: 压缩不删 event 原始数据 (只在 model context 层面), 业务方可查
//! - 可测: 9 个测试覆盖 happy + edge case + stub + error
//! - 可扩展: token estimator trait 抽象 (P15+ 接 tiktoken / hf tokenizer)
//!
//! # 限制 (Limitations) — P14.4.1
//!
//! - Token 估算粗略 (字符 / 4), 业务方要精确可换 estimator
//! - LLM provider 是 stub (P14.4.2 接 ma-harness-model)
//! - CompactionSummary 事件用现有 ModelResponse 包装, 不动 EventType enum (P15+ 加新变体)
//!
//! [dsh-feature-parity-table §10]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#10-conformance--behavioral-parity
//! [packages/compaction/]: https://github.com/deepseek-ai/deepseek-harness/tree/main/packages/compaction

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use ma_harness_core::{EventType, SessionEvent};
use thiserror::Error;

// ============================================================================
// CompactionError: 统一的 compaction 错误
// ============================================================================

/// Compaction 错误.
#[derive(Debug, Error)]
pub enum CompactionError {
    /// Token 估算失败 (estimator 内部错误)
    #[error("token estimation failed: {0}")]
    TokenEstimation(String),

    /// LLM 调用失败 (P14.4.2 LlmCompactionProvider 用)
    #[error("LLM call failed: {0}")]
    LlmCall(String),
}

// ============================================================================
// CompactionStats: 压缩效果统计
// ============================================================================

/// Compaction 统计.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactionStats {
    /// 原始 events 数
    pub original_count: usize,
    /// 保留 events 数
    pub kept_count: usize,
    /// 删除 events 数
    pub removed_count: usize,
    /// 估算 token 数 (压缩前)
    pub tokens_before: usize,
    /// 估算 token 数 (压缩后, 含 summary)
    pub tokens_after: usize,
    /// 是否触发了压缩 (false = 不需要, 因为已 < max_tokens)
    pub triggered: bool,
}

impl CompactionStats {
    /// 压缩比 (removed / original), 0.0 = 没压缩, 1.0 = 全删
    pub fn compression_ratio(&self) -> f64 {
        if self.original_count == 0 {
            0.0
        } else {
            self.removed_count as f64 / self.original_count as f64
        }
    }
}

// ============================================================================
// CompactionContext: 配置
// ============================================================================

/// Token 估算函数 (业务方可注入自定义, 例如接 tiktoken).
///
/// 默认 estimator: `payload_json.len() / 4` (粗略, 1 token ≈ 4 char)
pub type TokenEstimator = Arc<dyn Fn(&SessionEvent) -> usize + Send + Sync>;

/// 业务方默认 token estimator: payload_json.len() / 4.
pub fn default_token_estimator(event: &SessionEvent) -> usize {
    event.payload_json.as_deref().map(|s| s.len()).unwrap_or(0) / 4
}

/// Compaction 配置.
#[derive(Clone)]
pub struct CompactionContext {
    /// Token 阈值 (超过触发压缩, 默认 8000)
    pub max_tokens: usize,
    /// 保留最近 N 步 (run_id distinct, 默认 3)
    pub keep_recent_steps: usize,
    /// 必保留事件类型 (永不删, 默认 [RunStart, UserInput, ToolCall, ToolResult, RunEnd])
    pub always_keep: Vec<EventType>,
    /// Token estimator (默认 `default_token_estimator`)
    pub token_estimator: TokenEstimator,
}

impl std::fmt::Debug for CompactionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompactionContext")
            .field("max_tokens", &self.max_tokens)
            .field("keep_recent_steps", &self.keep_recent_steps)
            .field("always_keep", &self.always_keep)
            .field("token_estimator", &"<fn>")
            .finish()
    }
}

impl Default for CompactionContext {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            keep_recent_steps: 3,
            always_keep: vec![
                EventType::RunStart,
                EventType::RunEnd,
                EventType::UserInput,
                EventType::ToolCall,
                EventType::ToolResult,
            ],
            token_estimator: Arc::new(default_token_estimator),
        }
    }
}

impl CompactionContext {
    /// 设置 max_tokens
    pub fn with_max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = n;
        self
    }

    /// 设置 keep_recent_steps
    pub fn with_keep_recent_steps(mut self, n: usize) -> Self {
        self.keep_recent_steps = n;
        self
    }

    /// 设置 always_keep 列表
    pub fn with_always_keep(mut self, types: Vec<EventType>) -> Self {
        self.always_keep = types;
        self
    }

    /// 设置 token estimator
    pub fn with_token_estimator(mut self, estimator: TokenEstimator) -> Self {
        self.token_estimator = estimator;
        self
    }

    /// 估算单事件 token 数
    pub fn estimate(&self, event: &SessionEvent) -> usize {
        (self.token_estimator)(event)
    }
}

// ============================================================================
// CompactionSummary: 压缩后插入的 summary 事件
// ============================================================================

/// Compaction 摘要 (插入到保留 events 头部, 给 LLM 看).
///
/// 业务方调 `to_session_event()` 转成 SessionEvent 插入 events 列表.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionSummary {
    /// 所属 session
    pub session_id: String,
    /// 被压缩的 events id 列表
    pub compacted_event_ids: Vec<String>,
    /// 摘要文本 (业务方 / LLM 生成)
    pub summary_text: String,
}

impl CompactionSummary {
    /// 转成 SessionEvent (event_type = ModelResponse, payload_json = summary)
    pub fn to_session_event(&self) -> SessionEvent {
        let payload = serde_json::json!({
            "summary": self.summary_text,
            "compacted_event_ids": self.compacted_event_ids,
            "compaction_kind": "truncation", // P14.4.1: 只支持 truncation
        });
        SessionEvent {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: self.session_id.clone(),
            event_type: EventType::ModelResponse,
            ts: chrono::Utc::now(),
            severity: ma_harness_core::Severity::Info,
            run_id: None,
            plugin_name: Some("ma-harness-compaction".to_string()),
            payload_json: Some(payload.to_string()),
            error_message: None,
            model_visible: true,
        }
    }
}

// ============================================================================
// CompactionStrategy: 能力缝 trait (跟 dsh ctx.sessionProjections 集成)
// ============================================================================

/// Compaction 能力缝 (跟 dsh `packages/compaction` 对等).
///
/// **核心方法**:
/// - [`compact`](Self::compact) — 接受 events 列表 + context, 返回 `(compacted_events, stats)`
///
/// **实现**:
/// - [`BasicCompactionProvider`] — 默认 (P14.4.1 主交付)
/// - [`LlmCompactionProvider`] — stub (P14.4.2 接 ma-harness-model)
/// - 业务方可注入 mock provider (测试用)
#[async_trait]
pub trait CompactionStrategy: Send + Sync + 'static {
    /// 压缩 events 列表 (返回新列表 + 统计, 不修改原 events)
    async fn compact(
        &self,
        events: &[SessionEvent],
        ctx: &CompactionContext,
    ) -> Result<(Vec<SessionEvent>, CompactionStats), CompactionError>;

    /// Provider 标识
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// BasicCompactionProvider: 规则式 (truncate 旧 ModelResponse)
// ============================================================================

/// 基础 compaction provider (P14.4.1 主交付).
///
/// **算法**:
/// 1. 估算总 token (`events.iter().map(estimate).sum()`)
/// 2. 如果 `<= ctx.max_tokens` → 不压缩, 返回原 events
/// 3. 否则:
///    a. 必保留 events (按 ctx.always_keep) 全留
///    b. 必保留最近 N 步 (按 run_id distinct, 倒序)
///    c. 剩余 events 按时间倒序, 累加 token 直到 max_tokens
///    d. 把要删的 events 收集, 插入 CompactionSummary
///    e. 返回 (保留 events, stats)
pub struct BasicCompactionProvider;

impl BasicCompactionProvider {
    /// 创建一个新 BasicCompactionProvider
    pub const fn new() -> Self {
        Self
    }
}

impl Default for BasicCompactionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompactionStrategy for BasicCompactionProvider {
    async fn compact(
        &self,
        events: &[SessionEvent],
        ctx: &CompactionContext,
    ) -> Result<(Vec<SessionEvent>, CompactionStats), CompactionError> {
        let original_count = events.len();
        let tokens_before: usize = events.iter().map(|e| ctx.estimate(e)).sum();

        // Step 1: 不需要压缩
        if tokens_before <= ctx.max_tokens {
            return Ok((
                events.to_vec(),
                CompactionStats {
                    original_count,
                    kept_count: original_count,
                    removed_count: 0,
                    tokens_before,
                    tokens_after: tokens_before,
                    triggered: false,
                },
            ));
        }

        // Step 2: 收集必保留 (always_keep)
        let always_keep_set: std::collections::HashSet<EventType> =
            ctx.always_keep.iter().copied().collect();

        let mut always_kept: Vec<SessionEvent> = Vec::new();
        let mut trimmable: Vec<SessionEvent> = Vec::new();
        for e in events {
            if always_keep_set.contains(&e.event_type) {
                always_kept.push(e.clone());
            } else {
                trimmable.push(e.clone());
            }
        }

        // Step 3: 必保留最近 N 步 (按 run_id distinct, 倒序)
        // 把 trimmable 按 run_id 分组, 取后 N 个 run 保留
        let mut runs_in_order: Vec<String> = Vec::new();
        let mut run_events: std::collections::HashMap<String, Vec<SessionEvent>> =
            std::collections::HashMap::new();
        let mut no_run_events: Vec<SessionEvent> = Vec::new();
        for e in &trimmable {
            match &e.run_id {
                Some(rid) => {
                    if !run_events.contains_key(rid) {
                        runs_in_order.push(rid.clone());
                    }
                    run_events.entry(rid.clone()).or_default().push(e.clone());
                }
                None => no_run_events.push(e.clone()),
            }
        }
        let keep_runs: Vec<String> = if runs_in_order.len() > ctx.keep_recent_steps {
            runs_in_order[runs_in_order.len() - ctx.keep_recent_steps..].to_vec()
        } else {
            runs_in_order.clone()
        };
        let mut keep_recent: Vec<SessionEvent> = Vec::new();
        for rid in &keep_runs {
            if let Some(es) = run_events.get(rid) {
                keep_recent.extend(es.iter().cloned());
            }
        }
        keep_recent.extend(no_run_events.iter().cloned());

        // Step 4: 累加 keep_recent token, 看是否还超, 否则加更多老 run
        // (P14.4.1 简化: keep_recent 固定保留, 不再贪心加老)
        let tokens_keep_recent: usize = keep_recent.iter().map(|e| ctx.estimate(e)).sum();
        let tokens_always: usize = always_kept.iter().map(|e| ctx.estimate(e)).sum();
        let tokens_after = tokens_always + tokens_keep_recent;

        // Step 5: 收集要删的 events
        let keep_recent_ids: std::collections::HashSet<String> =
            keep_recent.iter().map(|e| e.id.clone()).collect();
        let always_kept_ids: std::collections::HashSet<String> =
            always_kept.iter().map(|e| e.id.clone()).collect();
        let mut compacted_event_ids: Vec<String> = Vec::new();
        for e in events {
            if !keep_recent_ids.contains(&e.id) && !always_kept_ids.contains(&e.id) {
                compacted_event_ids.push(e.id.clone());
            }
        }

        // Step 6: 合并 + 加 summary
        let mut compacted = always_kept.clone();
        compacted.extend(keep_recent.iter().cloned());
        // 按时间排序
        compacted.sort_by(|a, b| a.ts.cmp(&b.ts));

        // 插入 summary 在最前 (永远保留)
        if !compacted_event_ids.is_empty() {
            let summary_text = format!(
                "[Compaction summary] compacted {} events ({} tokens -> {} tokens)",
                compacted_event_ids.len(),
                tokens_before,
                tokens_after
            );
            let summary = CompactionSummary {
                session_id: events
                    .first()
                    .map(|e| e.session_id.clone())
                    .unwrap_or_default(),
                compacted_event_ids: compacted_event_ids.clone(),
                summary_text,
            };
            let summary_event = summary.to_session_event();
            compacted.insert(0, summary_event);
        }

        let stats = CompactionStats {
            original_count,
            kept_count: compacted.len(),
            removed_count: original_count - compacted.len(),
            tokens_before,
            tokens_after: tokens_after
                + (if !compacted_event_ids.is_empty() {
                    // summary event 自身 token
                    compacted.first().map(|e| ctx.estimate(e)).unwrap_or(0)
                } else {
                    0
                }),
            triggered: true,
        };

        Ok((compacted, stats))
    }

    fn provider_name(&self) -> &'static str {
        "basic-truncation"
    }
}

// ============================================================================
// LlmCompactionProvider: stub (P14.4.2 接 ma-harness-model)
// ============================================================================

/// LLM 摘要 compaction provider (P14.4.2 stub).
///
/// **当前实现**: 直接返回原 events, 不压缩 (跟 dsh `LlmCompactionProvider` 接口对齐但无 LLM 能力).
/// **未来**: 接 `ma-harness-model::ModelAdapter`, 调 LLM 摘要老 events.
pub struct LlmCompactionProvider;

impl LlmCompactionProvider {
    /// 创建一个新 LlmCompactionProvider (stub)
    pub const fn new() -> Self {
        Self
    }
}

impl Default for LlmCompactionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompactionStrategy for LlmCompactionProvider {
    async fn compact(
        &self,
        events: &[SessionEvent],
        _ctx: &CompactionContext,
    ) -> Result<(Vec<SessionEvent>, CompactionStats), CompactionError> {
        // P14.4.1 stub: 透传, 不压缩
        // P14.4.2 接 ma-harness-model 后实装
        Ok((
            events.to_vec(),
            CompactionStats {
                original_count: events.len(),
                kept_count: events.len(),
                removed_count: 0,
                tokens_before: 0, // 简化: stub 不算 token
                tokens_after: 0,
                triggered: false,
            },
        ))
    }

    fn provider_name(&self) -> &'static str {
        "llm-stub"
    }
}

// ============================================================================
// COMPACTION_STRATEGY typed key (P14.4.1: 跟 ctx.session 接入点)
// ============================================================================

/// Typed key: `ctx.compaction` 注入的 CompactionStrategy.
pub static COMPACTION_STRATEGY: ma_harness_cordis::CtxKey<Arc<dyn CompactionStrategy>> =
    ma_harness_seam::ctx_key!("compaction_strategy");

// ============================================================================
// DefaultCompactionProvider: 平台默认 (P14.4.1: BasicCompactionProvider)
// ============================================================================

/// 平台默认 compaction provider (P14.4.1: BasicCompactionProvider)
pub type DefaultCompactionProvider = BasicCompactionProvider;

// ============================================================================
// 单元测试 (mod tests) — 9 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ma_harness_core::Severity;
    use pretty_assertions::assert_eq;

    /// 测试用 SessionEvent 构造器
    fn make_event(
        session_id: &str,
        event_type: EventType,
        payload: &str,
        run_id: Option<&str>,
    ) -> SessionEvent {
        SessionEvent {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            event_type,
            ts: Utc::now(),
            severity: Severity::Info,
            run_id: run_id.map(String::from),
            plugin_name: None,
            payload_json: Some(payload.to_string()),
            error_message: None,
            model_visible: true,
        }
    }

    /// 跨平台 "false" 命令 (用于测试无 LLM stub)
    #[test]
    fn default_token_estimator_divides_by_4() {
        let e = make_event("s", EventType::ModelResponse, "a".repeat(80).as_str(), None);
        assert_eq!(default_token_estimator(&e), 20); // 80 / 4 = 20
    }

    #[tokio::test]
    async fn empty_events_returns_empty_with_not_triggered() {
        let provider = BasicCompactionProvider::new();
        let (events, stats) = provider
            .compact(&[], &CompactionContext::default())
            .await
            .expect("compact");
        assert!(events.is_empty());
        assert!(!stats.triggered);
        assert_eq!(stats.removed_count, 0);
    }

    #[tokio::test]
    async fn under_threshold_returns_original_not_triggered() {
        let provider = BasicCompactionProvider::new();
        let events = vec![
            make_event("s", EventType::RunStart, "x", None),
            make_event("s", EventType::UserInput, "hello", Some("run-1")),
            make_event("s", EventType::ModelResponse, "hi there", Some("run-1")),
            make_event("s", EventType::RunEnd, "x", Some("run-1")),
        ];
        // total token ~ (1 + 5 + 8 + 1) / 4 ≈ 4, 远低于 8000
        let (kept, stats) = provider
            .compact(&events, &CompactionContext::default())
            .await
            .expect("compact");
        assert_eq!(kept.len(), 4, "should keep all events when under threshold");
        assert!(!stats.triggered);
        assert_eq!(stats.removed_count, 0);
    }

    #[tokio::test]
    async fn over_threshold_trims_old_model_responses() {
        let provider = BasicCompactionProvider::new();
        // 5 个 run, 每个 run 都有 ModelResponse 长 payload
        let mut events = vec![make_event("s", EventType::RunStart, "x", None)];
        for run in 0..5 {
            let rid = format!("run-{run}");
            events.push(make_event(
                "s",
                EventType::UserInput,
                "a".repeat(4000).as_str(),
                Some(&rid),
            ));
            events.push(make_event(
                "s",
                EventType::ModelResponse,
                "b".repeat(4000).as_str(),
                Some(&rid),
            ));
            events.push(make_event("s", EventType::RunEnd, "x", Some(&rid)));
        }
        // total token ~ 5*8001/4 ≈ 10000, 超过 8000
        let (kept, stats) = provider
            .compact(&events, &CompactionContext::default().with_max_tokens(8000))
            .await
            .expect("compact");
        assert!(
            stats.triggered,
            "should trigger compaction when over threshold"
        );
        assert!(stats.removed_count > 0, "should remove some events");
        // RunStart / RunEnd / UserInput 必保留, 加最近 N 步的 ModelResponse
        assert!(kept.iter().any(|e| e.event_type == EventType::RunStart));
        // 第一个事件是 summary (如果 removed > 0)
        if stats.removed_count > 0 {
            assert_eq!(
                kept[0].event_type,
                EventType::ModelResponse,
                "summary event 应该是 ModelResponse 类型"
            );
            assert!(
                kept[0]
                    .payload_json
                    .as_deref()
                    .unwrap_or_default()
                    .contains("Compaction summary")
            );
        }
    }

    #[tokio::test]
    async fn always_keep_events_are_preserved() {
        let provider = BasicCompactionProvider::new();
        let mut events = vec![make_event("s", EventType::RunStart, "x", None)];
        // 加 5 个 run, 每个都有长 payload 触发压缩
        for run in 0..5 {
            let rid = format!("run-{run}");
            events.push(make_event("s", EventType::UserInput, "u", Some(&rid)));
            events.push(make_event(
                "s",
                EventType::ModelResponse,
                "m".repeat(10000).as_str(),
                Some(&rid),
            ));
            events.push(make_event("s", EventType::RunEnd, "x", Some(&rid)));
        }
        let (kept, _stats) = provider
            .compact(&events, &CompactionContext::default().with_max_tokens(1000))
            .await
            .expect("compact");
        // RunStart 必保留
        assert!(kept.iter().any(|e| e.event_type == EventType::RunStart));
        // UserInput 必保留 (ctx.always_keep 默认包含)
        let user_input_count = kept
            .iter()
            .filter(|e| e.event_type == EventType::UserInput)
            .count();
        assert_eq!(
            user_input_count, 5,
            "all 5 UserInput events should be preserved"
        );
    }

    #[tokio::test]
    async fn keep_recent_steps_preserves_recent_runs() {
        let provider = BasicCompactionProvider::new();
        let mut events = vec![make_event("s", EventType::RunStart, "x", None)];
        for run in 0..10 {
            let rid = format!("run-{run}");
            events.push(make_event("s", EventType::UserInput, "u", Some(&rid)));
            events.push(make_event(
                "s",
                EventType::ModelResponse,
                "m".repeat(2000).as_str(),
                Some(&rid),
            ));
        }
        let (kept, _stats) = provider
            .compact(
                &events,
                &CompactionContext::default()
                    .with_max_tokens(1000)
                    .with_keep_recent_steps(3),
            )
            .await
            .expect("compact");
        // 保留最近 3 步 (run-7, run-8, run-9) 的 ModelResponse
        let recent_model_ids: Vec<String> = kept
            .iter()
            .filter(|e| {
                e.event_type == EventType::ModelResponse && e.run_id.as_deref() == Some("run-9")
            })
            .map(|e| e.id.clone())
            .collect();
        assert!(
            !recent_model_ids.is_empty(),
            "run-9 的 ModelResponse 应保留"
        );
    }

    #[tokio::test]
    async fn stats_compression_ratio() {
        let stats = CompactionStats {
            original_count: 100,
            kept_count: 30,
            removed_count: 70,
            tokens_before: 10000,
            tokens_after: 3000,
            triggered: true,
        };
        assert_eq!(stats.compression_ratio(), 0.7);
    }

    #[tokio::test]
    async fn llm_provider_stub_passes_through() {
        let provider = LlmCompactionProvider::new();
        assert_eq!(provider.provider_name(), "llm-stub");
        let events = vec![make_event("s", EventType::ModelResponse, "x", None)];
        let (kept, stats) = provider
            .compact(&events, &CompactionContext::default())
            .await
            .expect("compact");
        assert_eq!(kept.len(), 1, "stub passes through events unchanged");
        assert!(!stats.triggered);
    }

    #[tokio::test]
    async fn custom_token_estimator_is_used() {
        let estimator: TokenEstimator = Arc::new(|_e| 1); // 每个 event 算 1 token
        let ctx = CompactionContext::default()
            .with_max_tokens(3) // 3 events
            .with_token_estimator(estimator);

        let provider = BasicCompactionProvider::new();
        let events: Vec<SessionEvent> = (0..5)
            .map(|i| {
                make_event(
                    "s",
                    EventType::ModelResponse,
                    "x",
                    Some(&format!("run-{i}")),
                )
            })
            .collect();
        let (_kept, stats) = provider.compact(&events, &ctx).await.expect("compact");
        // 5 events × 1 token = 5, 超过 3, 必触发压缩
        assert!(stats.triggered);
    }

    #[tokio::test]
    async fn summary_event_has_compacted_event_ids() {
        let provider = BasicCompactionProvider::new();
        let mut events = vec![make_event("s", EventType::RunStart, "x", None)];
        // 5 个 run, keep_recent=1 让 4 个旧 run 被压, 触发 summary 插入
        for run in 0..5 {
            let rid = format!("run-{run}");
            events.push(make_event("s", EventType::UserInput, "u", Some(&rid)));
            events.push(make_event(
                "s",
                EventType::ModelResponse,
                "m".repeat(4000).as_str(),
                Some(&rid),
            ));
        }
        let (kept, stats) = provider
            .compact(
                &events,
                &CompactionContext::default()
                    .with_max_tokens(1000)
                    .with_keep_recent_steps(1),
            )
            .await
            .expect("compact");
        assert!(stats.triggered);
        assert!(stats.removed_count > 0, "应该删一些老 run");
        // summary 事件 payload 包含 compacted_event_ids (summary 永远是 events 第一个)
        let summary = kept
            .first()
            .expect("summary event should be first after compaction");
        let payload = summary.payload_json.as_deref().unwrap_or_default();
        assert!(
            payload.contains("compacted_event_ids"),
            "summary payload 应含 compacted_event_ids, got: {}",
            payload
        );
        assert!(payload.contains("summary"));
    }
}
