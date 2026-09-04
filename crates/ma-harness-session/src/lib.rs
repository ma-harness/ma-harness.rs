//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-session`
//! **Crate ident** (`use` 路径): `ma_harness_session`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-session = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_session::{EventForker, BasicTitleProvider, TitleProvider};
//! use ma_harness_core::{EventLog, EventQuery, SessionEvent};
//!
//! let log = EventLog::open_in_memory()?;
//! // 业务方先往 log 写 events...
//!
//! // Fork session
//! let forker = EventForker::new(&log);
//! let copied = forker.fork("source-session", None, "child-session")?;
//!
//! // Generate title from first user message
//! let title = BasicTitleProvider::new().generate(&events);
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-session
//!
//! # 设计 (Design) — P14.8
//!
//! **目标**: 抽象 `ctx.sessions.fork()` + `ctx.goals` + `ctx.sessionTitle` (跟 dsh `ctx.sessions` + `ctx.goals` 1:1 对等).
//! 业务方
//! - `EventForker::fork(source, boundary?, new_id?)` — 复制 source session events 到新 session
//! - `GoalStore` — 注册 / 列出 / 完成 session 目标
//! - `TitleProvider` — 从 events 自动生成 session title
//!
//! **背景**: 见 [dsh-feature-parity-table §6, §12]. ma-harness 之前只有 `SessionEvent` log + projection, 没有 fork/goal/title 高层 API.
//!
//! **核心抽象**:
//! - [`EventForker`] — 绑 `&EventLog` (ma-harness-core), 提供 `fork` 操作
//! - [`Goal`] + [`GoalStatus`] (Active / Done / Cancelled)
//! - [`GoalStore`] trait (register / list / complete)
//! - [`InMemoryGoalStore`] (P14.8.1 主交付)
//! - [`TitleProvider`] trait + [`BasicTitleProvider`] (从 first UserInput payload)
//!
//! **6 质量属性**:
//! - 可复用: 业务方可注入自定义 GoalStore (P15+ sqlite) / TitleProvider (P15+ LLM-summarized)
//! - 可维护: 模块化分块, forker / goal / title 集中 lib.rs
//! - 鲁棒: fork 边界 (boundary) 显式,空 events 跳过,Goal 状态机显式
//! - 安全: 不 eval payload, 静态 string
//! - 可测: 6 测试覆盖 fork / goal lifecycle / title generate
//! - 可扩展: trait 抽象, future plugin-session
//!
//! # 限制 (Limitations) — P14.8.1
//!
//! - 绑 `ma_harness_core::EventLog` 具体类型 (P15+ 抽 trait)
//! - `GoalStore` 仅 in-memory (P15+ sqlite / redis)
//! - `BasicTitleProvider` 只取 first UserInput, 不调 LLM (P15+ 用 LLM summarize)
//! - `fork` 用 `EventLog::append` 会 panic on 不变量, 业务方先 query 验证
//!
//! [dsh-feature-parity-table §6]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#6-session-log

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

use ma_harness_core::{EventLog, EventQuery, EventType, SessionEvent};

// ============================================================================
// SessionError
// ============================================================================

/// Session capability 错误.
#[derive(Debug, Error)]
pub enum SessionError {
    /// Underlying core log IO error
    #[error("session log I/O error: {0}")]
    Io(String),

    /// Session 不存在
    #[error("session not found: {0}")]
    NotFound(String),

    /// 业务方没设 boundary 但 source session 为空
    #[error("source session has no events: {0}")]
    EmptySource(String),

    /// Invalid goal transition
    #[error("invalid goal status transition: {from:?} -> {to:?}")]
    InvalidTransition {
        /// 旧状态
        from: String,
        /// 新状态
        to: String,
    },

    /// Validation failed
    #[error("validation failed: {0}")]
    Validation(String),
}

// ============================================================================
// EventForker
// ============================================================================

/// Session fork helper (P14.8.1 主交付, 跟 dsh `ctx.sessions.fork()` 对等).
///
/// **绑定**: `&EventLog` (ma-harness-core 具体类型). P15+ 抽 trait 让 mock / remote 业务方注入.
///
/// **算法**:
/// 1. `EventQuery { session_id: source, .. }` 查 source events
/// 2. (可选) 截到 `boundary_seq` (None = 全部)
/// 3. 对每个 event, clone, 改 `session_id = new_id`, 调 `log.append(new_event)`
/// 4. 返回 copied count
pub struct EventForker<'a> {
    log: &'a EventLog,
}

impl<'a> EventForker<'a> {
    /// 创建一个 EventForker (绑 log 引用)
    pub fn new(log: &'a EventLog) -> Self {
        Self { log }
    }

    /// Fork source session 到 new_session_id.
    ///
    /// # Arguments
    /// - `source`: 源 session id
    /// - `boundary_seq`: 复制截止 seq (None = 全部 events)
    /// - `new_session_id`: 新 session id (业务方生成, 业务方保证全局唯一)
    ///
    /// # Returns
    /// 复制的 event 数.
    ///
    /// # Errors
    /// - source 没 events: `EmptySource`
    /// - log IO 失败: `Io`
    pub fn fork(
        &self,
        source: &str,
        boundary_seq: Option<i64>,
        new_session_id: &str,
    ) -> Result<usize, SessionError> {
        // 1. query source events
        let query = EventQuery {
            session_id: source.to_string(),
            ..Default::default()
        };
        let page = self
            .log
            .query(&query)
            .map_err(|e| SessionError::Io(e.to_string()))?;

        // 2. 过滤 boundary
        let events_to_copy: Vec<SessionEvent> = page
            .events
            .into_iter()
            .filter_map(|stored| {
                if let Some(b) = boundary_seq {
                    if stored.seq > b {
                        return None;
                    }
                }
                Some(stored.event)
            })
            .collect();

        if events_to_copy.is_empty() {
            return Err(SessionError::EmptySource(source.to_string()));
        }

        // 3. 复制每个 event 到新 session
        // 注: EventLog::append 返回 i64 (seq), panic on 不变量违反
        // 业务方需保证 source events 本身合法
        // 重要: 给每个 event 生成新 id (避免 UNIQUE 冲突)
        let count = events_to_copy.len();
        for mut event in events_to_copy {
            event.session_id = new_session_id.to_string();
            event.id = uuid::Uuid::new_v4().to_string();
            self.log.append(event);
        }

        tracing::debug!(
            source = %source,
            new_id = %new_session_id,
            count = count,
            boundary = ?boundary_seq,
            "session forked"
        );
        Ok(count)
    }
}

// ============================================================================
// GoalStatus
// ============================================================================

/// Goal 状态 (P14.8.1: 3 状态).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GoalStatus {
    /// Active
    Active,
    /// Done
    Done,
    /// Cancelled
    Cancelled,
}

impl GoalStatus {
    /// 字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Done => "done",
            GoalStatus::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for GoalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// Goal
// ============================================================================

/// Session Goal (业务方声明的 session 目标).
///
/// **业务方用**: `ctx.goals.register(Goal::new("deploy to prod", session_id))`
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    /// Unique ID
    pub id: String,
    /// 所属 session
    pub session_id: String,
    /// Goal content (业务方描述, LLM 看的)
    pub content: String,
    /// Status
    pub status: GoalStatus,
    /// 创建时间 (Unix epoch seconds)
    pub created_at: i64,
    /// 完成时间 (Unix epoch seconds, None = 未完成)
    pub completed_at: Option<i64>,
}

impl Goal {
    /// 创建一个新 Goal (auto id + created_at)
    pub fn new(content: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            session_id: session_id.into(),
            status: GoalStatus::Active,
            created_at: chrono::Utc::now().timestamp(),
            completed_at: None,
        }
    }

    /// 业务方拿 id
    pub fn id(&self) -> &str {
        &self.id
    }
}

// ============================================================================
// GoalStore trait
// ============================================================================

/// Goal 存储 (P14.8.1: in-memory; P15+: sqlite / redis).
#[async_trait]
pub trait GoalStore: Send + Sync + 'static {
    /// 注册一个 goal
    async fn register(&self, goal: &Goal) -> Result<String, SessionError>;

    /// 按 id 拿
    async fn get(&self, id: &str) -> Result<Goal, SessionError>;

    /// 列 session 的所有 goals
    async fn list(&self, session_id: &str) -> Result<Vec<Goal>, SessionError>;

    /// 标记 goal 完成 (Active -> Done)
    async fn complete(&self, id: &str) -> Result<(), SessionError>;

    /// 取消 goal (Active -> Cancelled)
    async fn cancel(&self, id: &str) -> Result<(), SessionError>;
}

// ============================================================================
// InMemoryGoalStore (P14.8.1 主交付)
// ============================================================================

/// In-memory goal store (P14.8.1 主交付).
pub struct InMemoryGoalStore {
    goals: Mutex<std::collections::HashMap<String, Goal>>,
}

impl InMemoryGoalStore {
    /// 创建一个新 in-memory goal store
    pub fn new() -> Self {
        Self {
            goals: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryGoalStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GoalStore for InMemoryGoalStore {
    async fn register(&self, goal: &Goal) -> Result<String, SessionError> {
        if goal.content.trim().is_empty() {
            return Err(SessionError::Validation("content is empty".into()));
        }
        let mut goals = self.goals.lock().await;
        goals.insert(goal.id.clone(), goal.clone());
        Ok(goal.id.clone())
    }

    async fn get(&self, id: &str) -> Result<Goal, SessionError> {
        let goals = self.goals.lock().await;
        goals
            .get(id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound(id.to_string()))
    }

    async fn list(&self, session_id: &str) -> Result<Vec<Goal>, SessionError> {
        let goals = self.goals.lock().await;
        Ok(goals
            .values()
            .filter(|g| g.session_id == session_id)
            .cloned()
            .collect())
    }

    async fn complete(&self, id: &str) -> Result<(), SessionError> {
        let mut goals = self.goals.lock().await;
        let goal = goals
            .get(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        if goal.status != GoalStatus::Active {
            return Err(SessionError::InvalidTransition {
                from: goal.status.to_string(),
                to: GoalStatus::Done.to_string(),
            });
        }
        let mut updated = goal.clone();
        updated.status = GoalStatus::Done;
        updated.completed_at = Some(chrono::Utc::now().timestamp());
        goals.insert(id.to_string(), updated);
        Ok(())
    }

    async fn cancel(&self, id: &str) -> Result<(), SessionError> {
        let mut goals = self.goals.lock().await;
        let goal = goals
            .get(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;
        if goal.status != GoalStatus::Active {
            return Err(SessionError::InvalidTransition {
                from: goal.status.to_string(),
                to: GoalStatus::Cancelled.to_string(),
            });
        }
        let mut updated = goal.clone();
        updated.status = GoalStatus::Cancelled;
        updated.completed_at = Some(chrono::Utc::now().timestamp());
        goals.insert(id.to_string(), updated);
        Ok(())
    }
}

// ============================================================================
// TitleProvider
// ============================================================================

/// Session title provider (跟 dsh `ctx.sessionTitle` 对等).
///
/// **业务方用**: 跑 session 完事, 调 `provider.generate(&events) -> Option<String>` 自动取 title.
pub trait TitleProvider: Send + Sync + 'static {
    /// 从 events 生成 title
    fn generate(&self, events: &[SessionEvent]) -> Option<String>;

    /// Provider 标识
    fn provider_name(&self) -> &'static str;
}

// ============================================================================
// BasicTitleProvider (P14.8.1 主交付)
// ============================================================================

/// Basic title provider (P14.8.1 主交付).
///
/// **算法**: 找 first `EventType::UserInput` event, 取 `payload_json` 头 `max_len` 字符.
/// 业务方后续 P15+ 可加 LlmTitleProvider (调 LLM 摘要).
pub struct BasicTitleProvider {
    max_len: usize,
}

impl BasicTitleProvider {
    /// 创建一个新 BasicTitleProvider (default max_len=30)
    pub fn new() -> Self {
        Self { max_len: 30 }
    }

    /// 设置 max_len
    pub fn with_max_len(mut self, n: usize) -> Self {
        self.max_len = n;
        self
    }
}

impl Default for BasicTitleProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleProvider for BasicTitleProvider {
    fn generate(&self, events: &[SessionEvent]) -> Option<String> {
        for event in events {
            if event.event_type == EventType::UserInput {
                if let Some(ref payload) = event.payload_json {
                    // 简单处理: 取 raw string 头 max_len 字符
                    // 业务方如果 payload 是 JSON, 可加 JSON parse (P15+)
                    let truncated: String = payload.chars().take(self.max_len).collect();
                    let trimmed = truncated.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
        None
    }

    fn provider_name(&self) -> &'static str {
        "basic-first-user-input"
    }
}

// ============================================================================
// Typed keys
// ============================================================================

/// Typed key: `ctx.goals` 注入的 GoalStore.
pub static GOAL_STORE: ma_harness_cordis::CtxKey<Arc<dyn GoalStore>> =
    ma_harness_seam::ctx_key!("goal_store");

/// Typed key: `ctx.session_title` 注入的 TitleProvider.
pub static TITLE_PROVIDER: ma_harness_cordis::CtxKey<Arc<dyn TitleProvider>> =
    ma_harness_seam::ctx_key!("title_provider");

// ============================================================================
// Default type aliases
// ============================================================================

/// 平台默认 goal store (P14.8.1: InMemoryGoalStore)
pub type DefaultGoalStore = InMemoryGoalStore;

/// 平台默认 title provider (P14.8.1: BasicTitleProvider)
pub type DefaultTitleProvider = BasicTitleProvider;

// ============================================================================
// 单元测试 (mod tests) — 7 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// 创建一个 in-memory EventLog
    fn make_log() -> EventLog {
        EventLog::open_in_memory().expect("in-memory log")
    }

    /// 写一个 sample event 到 log
    fn write_event(log: &EventLog, session_id: &str, event_type: EventType, payload: Option<&str>) {
        let mut event = SessionEvent::new(session_id, event_type);
        event.payload_json = payload.map(String::from);
        log.append(event);
    }

    #[test]
    fn forker_copies_events_to_new_session() {
        let log = make_log();
        write_event(&log, "source", EventType::SessionStart, Some("{}"));
        write_event(&log, "source", EventType::UserInput, Some("hello"));
        write_event(&log, "source", EventType::ModelResponse, Some("hi"));

        let forker = EventForker::new(&log);
        let count = forker.fork("source", None, "child").expect("fork");
        assert_eq!(count, 3);

        // 验证 child session 有 3 个 events
        let q = EventQuery {
            session_id: "child".to_string(),
            ..Default::default()
        };
        let page = log.query(&q).expect("query child");
        assert_eq!(page.events.len(), 3);
        assert_eq!(page.events[0].event.event_type, EventType::SessionStart);
    }

    #[test]
    fn forker_respects_boundary_seq() {
        let log = make_log();
        write_event(&log, "source", EventType::SessionStart, Some("{}"));
        write_event(&log, "source", EventType::UserInput, Some("first"));
        write_event(&log, "source", EventType::ModelResponse, Some("reply1"));
        write_event(&log, "source", EventType::UserInput, Some("second"));
        write_event(&log, "source", EventType::ModelResponse, Some("reply2"));

        // query 拿 seq
        let q = EventQuery {
            session_id: "source".to_string(),
            ..Default::default()
        };
        let page = log.query(&q).expect("query");
        let boundary = page.events[2].seq; // 第 3 个 event (ModelResponse reply1) 的 seq

        let forker = EventForker::new(&log);
        let count = forker
            .fork("source", Some(boundary), "child")
            .expect("fork");
        assert_eq!(count, 3, "应只复制 seq <= boundary 的 3 个 events");

        let page = log
            .query(&EventQuery {
                session_id: "child".to_string(),
                ..Default::default()
            })
            .expect("query child");
        assert_eq!(page.events.len(), 3);
    }

    #[test]
    fn forker_empty_source_errors() {
        let log = make_log();
        let forker = EventForker::new(&log);
        let err = forker.fork("nonexistent", None, "child").unwrap_err();
        assert!(matches!(err, SessionError::EmptySource(_)));
    }

    #[tokio::test]
    async fn goal_store_register_and_list() {
        let store = InMemoryGoalStore::new();
        let g = Goal::new("deploy to prod", "session-1");
        let id = store.register(&g).await.expect("register");
        let list = store.list("session-1").await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "deploy to prod");
        assert_eq!(list[0].id, id);
    }

    #[tokio::test]
    async fn goal_store_complete_active_to_done() {
        let store = InMemoryGoalStore::new();
        let g = Goal::new("test", "session-1");
        let id = store.register(&g).await.expect("register");
        store.complete(&id).await.expect("complete");
        let after = store.get(&id).await.expect("get");
        assert_eq!(after.status, GoalStatus::Done);
        assert!(after.completed_at.is_some());
    }

    #[tokio::test]
    async fn goal_store_complete_twice_errors() {
        let store = InMemoryGoalStore::new();
        let g = Goal::new("test", "session-1");
        let id = store.register(&g).await.expect("register");
        store.complete(&id).await.expect("complete");
        let err = store.complete(&id).await.unwrap_err();
        assert!(matches!(err, SessionError::InvalidTransition { .. }));
    }

    #[test]
    fn basic_title_provider_extracts_first_user_input() {
        let events = vec![
            SessionEvent::new("s", EventType::SessionStart),
            {
                let mut e = SessionEvent::new("s", EventType::UserInput);
                e.payload_json = Some("Fix bug #123 in module".into());
                e
            },
            SessionEvent::new("s", EventType::ModelResponse),
        ];
        let provider = BasicTitleProvider::new();
        let title = provider.generate(&events).expect("title");
        assert_eq!(title, "Fix bug #123 in module");
    }

    #[test]
    fn basic_title_provider_truncates_long_input() {
        let events = vec![{
            let mut e = SessionEvent::new("s", EventType::UserInput);
            e.payload_json = Some("a".repeat(100));
            e
        }];
        let provider = BasicTitleProvider::new().with_max_len(10);
        let title = provider.generate(&events).expect("title");
        assert_eq!(title, "a".repeat(10));
    }

    #[test]
    fn basic_title_provider_returns_none_when_no_user_input() {
        let events = vec![SessionEvent::new("s", EventType::SessionStart)];
        let provider = BasicTitleProvider::new();
        assert!(provider.generate(&events).is_none());
    }
}
