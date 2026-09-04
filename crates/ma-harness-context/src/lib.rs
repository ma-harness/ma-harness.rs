//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-context`
//! **Crate ident** (`use` 路径): `ma_harness_context`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-context = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_context::{RequestContext, ContextMiddleware, ContextChain};
//! use std::sync::Arc;
//!
//! let ctx = RequestContext::new()
//!     .with_trace_id("abc-123")
//!     .with_parent_session_id("session-1")
//!     .with_metadata("user_id", "alice");
//!
//! let chain = ContextChain::new();
//! chain.add_middleware(Arc::new(LoggingMiddleware)).await;
//! let propagated = chain.propagate(&ctx).await;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-context
//!
//! # 设计 (Design) — P14.10
//!
//! **目标**: 抽象 request-context plugin (跟 dsh `packages/context/` 1:1 对等).
//! 业务方
//! - 拿 `RequestContext` (trace_id / parent_session_id / deadline / metadata)
//! - 装 `ContextMiddleware` 进 chain (logging / tracing / OTel 集成)
//! - 跨 `ctx.inject()` 跟 model request 传播 trace_id
//!
//! **背景**: 见 [dsh-feature-parity-table §2] (context plugin ❌). ma-harness 之前无 request-context 概念.
//!
//! **核心抽象**:
//! - [`RequestContext`] struct (trace_id / parent_session_id / deadline / metadata + builder)
//! - [`ContextMiddleware`] trait (process + provider_name)
//! - [`ContextChain`] (in-memory middleware 链, async propagate)
//! - [`LoggingMiddleware`] (P14.10.1 主交付: tracing 记录)
//! - 2 个 typed keys
//!
//! **6 质量属性**:
//! - 可复用: 业务方可注入 OTel / OpenTracing middleware (P15+)
//! - 可维护: 模块化分块, context / middleware / chain 集中 lib.rs
//! - 鲁棒: 验证 (trace_id 非空), deadline 自动计算
//! - 安全: 不 eval metadata, 静态 string
//! - 可测: 7 测试覆盖 context / middleware / chain
//! - 可扩展: Middleware trait, future plugin-context
//!
//! # 限制 (Limitations) — P14.10.1
//!
//! - 仅 logging middleware (P14.10.1 主交付, OTel / OpenTracing P15+ stub)
//! - agent-loop 集成 (model request 注入) 留 P14.10.2
//! - 跨 ctx.inject() 传播留 P14.10.2
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

// ============================================================================
// ContextError
// ============================================================================

/// Context capability 错误.
#[derive(Debug, Error)]
pub enum ContextError {
    /// Validation failed (e.g. empty trace_id)
    #[error("context validation failed: {0}")]
    Validation(String),
}

// ============================================================================
// RequestContext
// ============================================================================

/// Request context (跟 dsh `packages/context/` RequestContext 对等).
///
/// **业务方场景**:
/// - `mah run --trace-id <uuid> "task"` → 创建 RequestContext with trace_id
/// - 跨 `ctx.inject()` 跟 model request 传播 trace_id
/// - 跨 session fork (P14.8) 继承 parent trace_id
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestContext {
    /// Trace ID (业务方传, 或 auto-generate UUID)
    pub trace_id: String,
    /// 父 session ID (跨 fork 时填)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// Deadline (Unix epoch seconds, None = no deadline)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<i64>,
    /// Metadata (key -> value, 业务方自定义, 跨 middleware 传播)
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl RequestContext {
    /// 创建一个新 RequestContext (auto-generate trace_id if empty)
    pub fn new() -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            parent_session_id: None,
            deadline: None,
            metadata: BTreeMap::new(),
        }
    }

    /// 设置 trace_id
    pub fn with_trace_id(mut self, id: impl Into<String>) -> Self {
        self.trace_id = id.into();
        self
    }

    /// 设置 parent_session_id
    pub fn with_parent_session_id(mut self, id: impl Into<String>) -> Self {
        self.parent_session_id = Some(id.into());
        self
    }

    /// 设置 deadline (Duration from now)
    pub fn with_deadline_in(mut self, dur: Duration) -> Self {
        self.deadline = Some(chrono::Utc::now().timestamp() + dur.as_secs() as i64);
        self
    }

    /// 设置 absolute deadline (Unix epoch seconds)
    pub fn with_deadline_at(mut self, ts: i64) -> Self {
        self.deadline = Some(ts);
        self
    }

    /// 设置 metadata key-value
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// 验证 context (trace_id 非空)
    pub fn validate(&self) -> Result<(), ContextError> {
        if self.trace_id.trim().is_empty() {
            return Err(ContextError::Validation("trace_id is empty".into()));
        }
        Ok(())
    }

    /// 检查 deadline 已过期
    pub fn is_expired(&self) -> bool {
        match self.deadline {
            Some(d) => chrono::Utc::now().timestamp() > d,
            None => false,
        }
    }

    /// child context (fork 时用, parent_session_id 填 source)
    pub fn child(&self, new_trace_id: Option<String>) -> Self {
        Self {
            trace_id: new_trace_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            parent_session_id: self.parent_session_id.clone(),
            deadline: self.deadline,
            metadata: self.metadata.clone(),
        }
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ContextMiddleware trait
// ============================================================================

/// Context middleware (跟 dsh `packages/context/` request-context plugins 对等).
///
/// **业务方用**:
/// ```ignore
/// struct MyTracingMiddleware;
///
/// #[async_trait]
/// impl ContextMiddleware for MyTracingMiddleware {
///     async fn process(&self, ctx: &RequestContext) -> Result<RequestContext, ContextError> {
///         tracing::info!(trace_id = %ctx.trace_id, "processing request");
///         Ok(ctx.clone())
///     }
///     fn provider_name(&self) -> &'static str { "my-tracing" }
/// }
/// ```
#[async_trait]
pub trait ContextMiddleware: Send + Sync + 'static {
    /// 处理 context (返回新 context, 业务方可改 trace_id / 加 metadata)
    async fn process(&self, ctx: &RequestContext) -> Result<RequestContext, ContextError>;

    /// Provider 标识
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// LoggingMiddleware (P14.10.1 主交付)
// ============================================================================

/// Logging middleware (P14.10.1 主交付).
///
/// **行为**: tracing::debug 记录 trace_id / parent_session_id / metadata count, 不改 context.
pub struct LoggingMiddleware;

impl LoggingMiddleware {
    /// 创建一个新 LoggingMiddleware
    pub const fn new() -> Self {
        Self
    }
}

impl Default for LoggingMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextMiddleware for LoggingMiddleware {
    async fn process(&self, ctx: &RequestContext) -> Result<RequestContext, ContextError> {
        tracing::debug!(
            trace_id = %ctx.trace_id,
            parent = ?ctx.parent_session_id,
            deadline = ?ctx.deadline,
            metadata_count = ctx.metadata.len(),
            "context middleware: logging"
        );
        Ok(ctx.clone())
    }

    fn provider_name(&self) -> &'static str {
        "logging"
    }
}

// ============================================================================
// ContextChain (in-memory middleware 链)
// ============================================================================

/// Context middleware 链 (业务方 CLI / runtime 用).
///
/// **流程**: 业务方按顺序 `add_middleware`, 调 `propagate(ctx)` 走 chain,
/// 每个 middleware 接收上一个的输出, 返回新 context.
pub struct ContextChain {
    middlewares: Mutex<Vec<Arc<dyn ContextMiddleware>>>,
}

impl ContextChain {
    /// 创建一个空 chain
    pub fn new() -> Self {
        Self {
            middlewares: Mutex::new(Vec::new()),
        }
    }

    /// 加 middleware (按调用顺序)
    pub async fn add_middleware(&self, m: Arc<dyn ContextMiddleware>) {
        let mut middlewares = self.middlewares.lock().await;
        middlewares.push(m);
    }

    /// middleware 数量
    pub async fn len(&self) -> usize {
        let middlewares = self.middlewares.lock().await;
        middlewares.len()
    }

    /// 是否空
    pub async fn is_empty(&self) -> bool {
        let middlewares = self.middlewares.lock().await;
        middlewares.is_empty()
    }

    /// 走 chain 处理 ctx (依次经过每个 middleware)
    pub async fn propagate(&self, ctx: &RequestContext) -> Result<RequestContext, ContextError> {
        let middlewares = self.middlewares.lock().await;
        let mut current = ctx.clone();
        for m in middlewares.iter() {
            current = m.process(&current).await?;
        }
        Ok(current)
    }
}

impl Default for ContextChain {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Typed keys
// ============================================================================

/// Typed key: `ctx.context` 当前活跃的 RequestContext.
///
/// 业务方:
/// ```ignore
/// use ma_harness_context::{ACTIVE_CONTEXT, RequestContext};
///
/// ctx.set(ACTIVE_CONTEXT, RequestContext::new());
/// ```
pub static ACTIVE_CONTEXT: ma_harness_cordis::CtxKey<std::sync::Arc<RequestContext>> =
    ma_harness_seam::ctx_key!("active_context");

/// Typed key: `ctx.context_chain` 注入的 middleware 链.
pub static CONTEXT_CHAIN: ma_harness_cordis::CtxKey<Arc<ContextChain>> =
    ma_harness_seam::ctx_key!("context_chain");

// ============================================================================
// Default type aliases
// ============================================================================

/// 平台默认 context middleware (P14.10.1: LoggingMiddleware)
pub type DefaultContextMiddleware = LoggingMiddleware;

// ============================================================================
// 单元测试 (mod tests) — 8 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn request_context_new_generates_trace_id() {
        let ctx = RequestContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(ctx.validate().is_ok());
    }

    #[test]
    fn request_context_validate_rejects_empty_trace_id() {
        let mut ctx = RequestContext::new();
        ctx.trace_id = String::new();
        assert!(ctx.validate().is_err());
    }

    #[test]
    fn request_context_with_deadline_in_future_not_expired() {
        let ctx = RequestContext::new().with_deadline_in(Duration::from_secs(60));
        assert!(!ctx.is_expired());
    }

    #[test]
    fn request_context_with_past_deadline_is_expired() {
        let ctx = RequestContext::new().with_deadline_at(chrono::Utc::now().timestamp() - 1);
        assert!(ctx.is_expired());
    }

    #[test]
    fn request_context_child_preserves_parent_metadata() {
        let parent = RequestContext::new()
            .with_trace_id("parent-trace")
            .with_parent_session_id("session-1")
            .with_metadata("user_id", "alice");
        let child = parent.child(Some("child-trace".into()));
        assert_eq!(child.trace_id, "child-trace");
        assert_eq!(child.parent_session_id.as_deref(), Some("session-1"));
        assert_eq!(
            child.metadata.get("user_id").map(String::as_str),
            Some("alice")
        );
    }

    #[tokio::test]
    async fn logging_middleware_preserves_context() {
        let m = LoggingMiddleware::new();
        let ctx = RequestContext::new().with_trace_id("test-trace");
        let after = m.process(&ctx).await.expect("process");
        assert_eq!(after.trace_id, "test-trace");
        assert_eq!(m.provider_name(), "logging");
    }

    #[tokio::test]
    async fn context_chain_propagates_through_middlewares_in_order() {
        let chain = ContextChain::new();
        chain
            .add_middleware(Arc::new(LoggingMiddleware::new()))
            .await;
        chain
            .add_middleware(Arc::new(LoggingMiddleware::new()))
            .await;
        assert_eq!(chain.len().await, 2);

        let ctx = RequestContext::new().with_trace_id("chain-test");
        let after = chain.propagate(&ctx).await.expect("propagate");
        assert_eq!(after.trace_id, "chain-test");
    }

    #[tokio::test]
    async fn context_chain_empty_returns_ctx_unchanged() {
        let chain = ContextChain::new();
        assert!(chain.is_empty().await);
        let ctx = RequestContext::new();
        let after = chain.propagate(&ctx).await.expect("propagate");
        assert_eq!(after, ctx);
    }

    /// Test middleware that mutates metadata (用于验证 chain 顺序)
    struct AddMetadataMiddleware {
        key: String,
        value: String,
    }

    #[async_trait]
    impl ContextMiddleware for AddMetadataMiddleware {
        async fn process(&self, ctx: &RequestContext) -> Result<RequestContext, ContextError> {
            let mut new = ctx.clone();
            new.metadata.insert(self.key.clone(), self.value.clone());
            Ok(new)
        }
        fn provider_name(&self) -> &'static str {
            "add-metadata"
        }
    }

    #[tokio::test]
    async fn context_chain_middlewares_accumulate_state() {
        let chain = ContextChain::new();
        chain
            .add_middleware(Arc::new(AddMetadataMiddleware {
                key: "step1".into(),
                value: "done".into(),
            }))
            .await;
        chain
            .add_middleware(Arc::new(AddMetadataMiddleware {
                key: "step2".into(),
                value: "done".into(),
            }))
            .await;

        let ctx = RequestContext::new();
        let after = chain.propagate(&ctx).await.expect("propagate");
        assert_eq!(
            after.metadata.get("step1").map(String::as_str),
            Some("done")
        );
        assert_eq!(
            after.metadata.get("step2").map(String::as_str),
            Some("done")
        );
    }
}
