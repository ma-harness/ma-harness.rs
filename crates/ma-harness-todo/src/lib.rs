//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-todo`
//! **Crate ident** (`use` 路径): `ma_harness_todo`
//!
//! Rust 自动从 kebab-case package name 转 snake_case crate ident.
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-todo = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_todo::{TodoItem, TodoList, TodoStatus, TodoStore};
//!
//! let store = TodoStore::in_memory();
//! let item = TodoItem::new("Fix bug #123")
//!     .with_priority(1)
//!     .with_status(TodoStatus::InProgress);
//! let id = store.write(&item).await?;
//! store.update_status(&id, TodoStatus::Done).await?;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-todo
//!
//! # 设计 (Design) — P14.7
//!
//! **目标**: 抽象 `ctx.todo` + `ctx.plan` (跟 dsh `packages/todo/` + `packages/plan/` 1:1 对等),
//! 业务方
//! - 用 `TodoStore` track multi-step work (`todo_write` / `todo_read` / `update_status`)
//! - 用 `PlanStore` 切到 plan 模式 (read-only proposals before execute)
//!
//! **P14.7.1 简化**: todo + plan 在同一 crate (`ma-harness-todo`), 业务方后续可拆
//! `ma-harness-plan` 单独 crate (P14.7.3) — 跟 _local plan 描述的 2 crate 拆一致.
//!
//! **核心抽象**:
//! - [`TodoItem`] + [`TodoStatus`] (Pending / InProgress / Done / Cancelled) + priority
//! - [`TodoList`] — Vec<TodoItem> 包装
//! - [`TodoStore`] trait (in-memory + future sqlite / redis)
//! - [`InMemoryTodoStore`] — P14.7.1 主交付 (tokio::sync::Mutex 保护)
//! - [`Plan`] + [`PlanStep`] + [`PlanStatus`]
//! - [`PlanStore`] trait
//! - [`InMemoryPlanStore`] — P14.7.1 主交付
//! - [`TODO_STORE`] / [`PLAN_STORE`] typed keys
//!
//! **6 质量属性**:
//! - 可复用: 业务方可实现 SqlTodoStore / RedisPlanStore (P15+ persistence)
//! - 可维护: 模块化分块, todo / plan / store / error 集中 lib.rs
//! - 鲁棒: 状态机显式 (Pending → InProgress → Done / Cancelled, 不能乱跳)
//! - 安全: 不 eval content, 静态 string
//! - 可测: 8 测试覆盖 happy / 状态转换 / 持久化 / error
//! - 可扩展: Plugin 化 (plugin-todo + plugin-plan 走 ctx.todo / ctx.plan)
//!
//! # 限制 (Limitations) — P14.7.1
//!
//! - 仅 in-memory store (P15+ 加 sqlite / redis 持久化)
//! - 没 `todo_write` / `plan` proc-macro (P14.7.2 proc-macro)
//! - plan 跟 session log 集成 (plan/* events) 留 P14.7.2
//!
//! [dsh-feature-parity-table §2]: https://github.com/ma-harness/ma-harness.rs/blob/main/docs/en/dsh-feature-parity-table.md#2-capability-seams

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

// ============================================================================
// TodoError
// ============================================================================

/// Todo / Plan 错误.
#[derive(Debug, Error)]
pub enum TodoError {
    /// Item 不存在
    #[error("todo/plan item not found: {0}")]
    NotFound(String),

    /// 状态转换无效 (Pending → Done 不能跳, 必须先 InProgress)
    #[error("invalid status transition: {from:?} -> {to:?}")]
    InvalidTransition {
        /// 当前状态
        from: String,
        /// 目标状态
        to: String,
    },

    /// 字段校验失败
    #[error("validation failed: {0}")]
    Validation(String),

    /// 序列化错误
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ============================================================================
// TodoStatus
// ============================================================================

/// Todo 状态 (P14.7.1: 4 状态).
///
/// **状态机**:
/// - `Pending` → `InProgress` / `Cancelled`
/// - `InProgress` → `Done` / `Cancelled` / `Pending` (回退)
/// - `Done` → (终态, 不再转换)
/// - `Cancelled` → (终态, 不再转换)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TodoStatus {
    /// 还没开始
    Pending,
    /// 进行中
    InProgress,
    /// 已完成
    Done,
    /// 已取消
    Cancelled,
}

impl TodoStatus {
    /// 字符串表示 (e.g. "pending" / "in_progress" / "done" / "cancelled")
    pub fn as_str(&self) -> &'static str {
        match self {
            TodoStatus::Pending => "pending",
            TodoStatus::InProgress => "in_progress",
            TodoStatus::Done => "done",
            TodoStatus::Cancelled => "cancelled",
        }
    }

    /// 验证状态转换合法
    pub fn can_transition_to(&self, target: TodoStatus) -> bool {
        use TodoStatus::*;
        match (self, target) {
            // Pending 可以去 InProgress 或 Cancelled
            (Pending, InProgress) | (Pending, Cancelled) => true,
            // InProgress 可以去 Done / Cancelled / 回退 Pending
            (InProgress, Done) | (InProgress, Cancelled) | (InProgress, Pending) => true,
            // 相同状态 (idempotent)
            (a, b) if *a == b => true,
            // Done / Cancelled 终态
            (Done, _) | (Cancelled, _) => false,
            // 其他不允许 (例如 Pending → Done 跳过 InProgress)
            _ => false,
        }
    }
}

impl std::str::FromStr for TodoStatus {
    type Err = TodoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(TodoStatus::Pending),
            "in_progress" => Ok(TodoStatus::InProgress),
            "done" => Ok(TodoStatus::Done),
            "cancelled" => Ok(TodoStatus::Cancelled),
            _ => Err(TodoError::Validation(format!("invalid status: {s}"))),
        }
    }
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// TodoItem
// ============================================================================

/// 单条 Todo (业务方写到 store 的单元).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    /// 唯一 ID (UUID v4)
    pub id: String,
    /// 内容 (业务方写的描述, LLM 看的)
    pub content: String,
    /// 状态
    pub status: TodoStatus,
    /// 优先级 (0 = 默认, 数字越小越优先, 业务方可自定义语义)
    pub priority: i32,
    /// 创建时间 (Unix epoch seconds, 业务方排序用)
    pub created_at: i64,
    /// 更新时间 (Unix epoch seconds)
    pub updated_at: i64,
}

impl TodoItem {
    /// 创建一个新 TodoItem (auto id + timestamps)
    pub fn new(content: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            status: TodoStatus::Pending,
            priority: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// 设置优先级
    pub fn with_priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }

    /// 设置初始状态
    pub fn with_status(mut self, s: TodoStatus) -> Self {
        self.status = s;
        self
    }

    /// 业务方拿 id
    pub fn id(&self) -> &str {
        &self.id
    }
}

// ============================================================================
// TodoList (immutable snapshot)
// ============================================================================

/// Todo 列表 (业务方 `read_all()` 拿到的 snapshot).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TodoList {
    /// 全部 Todo (业务方自行排序 — store 不保证顺序)
    pub items: Vec<TodoItem>,
}

impl TodoList {
    /// 空 list
    pub fn empty() -> Self {
        Self { items: Vec::new() }
    }

    /// 业务方按 status filter
    pub fn by_status(&self, status: TodoStatus) -> Vec<&TodoItem> {
        self.items.iter().filter(|i| i.status == status).collect()
    }

    /// 业务方按 priority 排序 (小 → 大, 数字越小越优先)
    pub fn sorted_by_priority(&self) -> Vec<&TodoItem> {
        let mut sorted: Vec<&TodoItem> = self.items.iter().collect();
        sorted.sort_by_key(|i| (i.priority, i.created_at));
        sorted
    }

    /// 数量
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ============================================================================
// TodoStore trait
// ============================================================================

/// Todo 存储 (P14.7.1: in-memory; P15+: sqlite / redis).
#[async_trait]
pub trait TodoStore: Send + Sync + 'static {
    /// 写一条 Todo (新建或覆盖)
    async fn write(&self, item: &TodoItem) -> Result<String, TodoError>;

    /// 按 id 读
    async fn read(&self, id: &str) -> Result<TodoItem, TodoError>;

    /// 全部读
    async fn read_all(&self) -> Result<TodoList, TodoError>;

    /// 更新状态 (带转换校验)
    async fn update_status(&self, id: &str, status: TodoStatus) -> Result<(), TodoError>;

    /// 删除
    async fn delete(&self, id: &str) -> Result<(), TodoError>;
}

// ============================================================================
// InMemoryTodoStore (P14.7.1 主交付)
// ============================================================================

/// In-memory todo store (P14.7.1 主交付, P15+ 业务方可注入 SqlTodoStore).
///
/// **实现**: `tokio::sync::Mutex<HashMap<id, TodoItem>>` 包装.
/// **并发**: 业务方多 task 同时调 store, 内部 mutex 串行化.
pub struct InMemoryTodoStore {
    items: Mutex<std::collections::HashMap<String, TodoItem>>,
}

impl InMemoryTodoStore {
    /// 创建一个新 in-memory store
    pub fn new() -> Self {
        Self {
            items: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryTodoStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TodoStore for InMemoryTodoStore {
    async fn write(&self, item: &TodoItem) -> Result<String, TodoError> {
        if item.content.trim().is_empty() {
            return Err(TodoError::Validation("content is empty".into()));
        }
        let mut items = self.items.lock().await;
        items.insert(item.id.clone(), item.clone());
        Ok(item.id.clone())
    }

    async fn read(&self, id: &str) -> Result<TodoItem, TodoError> {
        let items = self.items.lock().await;
        items
            .get(id)
            .cloned()
            .ok_or_else(|| TodoError::NotFound(id.to_string()))
    }

    async fn read_all(&self) -> Result<TodoList, TodoError> {
        let items = self.items.lock().await;
        let mut list: Vec<TodoItem> = items.values().cloned().collect();
        list.sort_by_key(|i| (i.priority, i.created_at));
        Ok(TodoList { items: list })
    }

    async fn update_status(&self, id: &str, status: TodoStatus) -> Result<(), TodoError> {
        let mut items = self.items.lock().await;
        let item = items
            .get(id)
            .ok_or_else(|| TodoError::NotFound(id.to_string()))?;
        if !item.status.can_transition_to(status) {
            return Err(TodoError::InvalidTransition {
                from: item.status.to_string(),
                to: status.to_string(),
            });
        }
        let mut updated = item.clone();
        updated.status = status;
        updated.updated_at = chrono::Utc::now().timestamp();
        items.insert(id.to_string(), updated);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), TodoError> {
        let mut items = self.items.lock().await;
        items
            .remove(id)
            .ok_or_else(|| TodoError::NotFound(id.to_string()))?;
        Ok(())
    }
}

// ============================================================================
// PlanStatus
// ============================================================================

/// Plan 状态 (P14.7.1: 4 状态).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PlanStatus {
    /// 草稿 (业务方写 plan, 还没 execute)
    Draft,
    /// 已批准 (业务方确认要执行)
    Approved,
    /// 执行中
    InProgress,
    /// 已完成 (所有 step Done 或 Cancelled)
    Completed,
    /// 已拒绝 (业务方不执行)
    Rejected,
}

impl std::fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PlanStatus::Draft => "draft",
            PlanStatus::Approved => "approved",
            PlanStatus::InProgress => "in_progress",
            PlanStatus::Completed => "completed",
            PlanStatus::Rejected => "rejected",
        })
    }
}

// ============================================================================
// PlanStep / Plan
// ============================================================================

/// Plan 单步 (业务方 plan mode 写的 proposal).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlanStep {
    /// 步骤 ID
    pub id: String,
    /// 步骤描述 (LLM 看)
    pub description: String,
    /// 步骤状态
    pub status: TodoStatus,
    /// 依赖步骤 ID (P15+ DAG)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

/// 完整 Plan (一组 PlanStep + 总状态).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    /// Plan ID
    pub id: String,
    /// Plan 标题
    pub title: String,
    /// 总状态
    pub status: PlanStatus,
    /// 步骤
    pub steps: Vec<PlanStep>,
}

impl Plan {
    /// 创建一个新 Plan
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            status: PlanStatus::Draft,
            steps: Vec::new(),
        }
    }

    /// 加一个 step
    pub fn with_step(mut self, step: PlanStep) -> Self {
        self.steps.push(step);
        self
    }

    /// 业务方拿 id
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl PlanStep {
    /// 创建一个新 step
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            status: TodoStatus::Pending,
            depends_on: Vec::new(),
        }
    }

    /// 设置初始状态
    pub fn with_status(mut self, s: TodoStatus) -> Self {
        self.status = s;
        self
    }

    /// 加依赖
    pub fn with_depends_on(mut self, dep: impl Into<String>) -> Self {
        self.depends_on.push(dep.into());
        self
    }
}

// ============================================================================
// PlanStore trait
// ============================================================================

/// Plan 存储.
#[async_trait]
pub trait PlanStore: Send + Sync + 'static {
    /// 写一个 Plan (新建或覆盖)
    async fn write(&self, plan: &Plan) -> Result<String, TodoError>;

    /// 按 id 读
    async fn read(&self, id: &str) -> Result<Plan, TodoError>;

    /// 全部读
    async fn read_all(&self) -> Result<Vec<Plan>, TodoError>;

    /// 更新 Plan 状态
    async fn update_status(&self, id: &str, status: PlanStatus) -> Result<(), TodoError>;

    /// 更新 step 状态
    async fn update_step_status(
        &self,
        plan_id: &str,
        step_id: &str,
        status: TodoStatus,
    ) -> Result<(), TodoError>;

    /// 删除
    async fn delete(&self, id: &str) -> Result<(), TodoError>;
}

// ============================================================================
// InMemoryPlanStore (P14.7.1 主交付)
// ============================================================================

/// In-memory plan store.
pub struct InMemoryPlanStore {
    plans: Mutex<std::collections::HashMap<String, Plan>>,
}

impl InMemoryPlanStore {
    /// 创建一个新 in-memory plan store
    pub fn new() -> Self {
        Self {
            plans: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryPlanStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlanStore for InMemoryPlanStore {
    async fn write(&self, plan: &Plan) -> Result<String, TodoError> {
        if plan.title.trim().is_empty() {
            return Err(TodoError::Validation("title is empty".into()));
        }
        let mut plans = self.plans.lock().await;
        plans.insert(plan.id.clone(), plan.clone());
        Ok(plan.id.clone())
    }

    async fn read(&self, id: &str) -> Result<Plan, TodoError> {
        let plans = self.plans.lock().await;
        plans
            .get(id)
            .cloned()
            .ok_or_else(|| TodoError::NotFound(id.to_string()))
    }

    async fn read_all(&self) -> Result<Vec<Plan>, TodoError> {
        let plans = self.plans.lock().await;
        Ok(plans.values().cloned().collect())
    }

    async fn update_status(&self, id: &str, status: PlanStatus) -> Result<(), TodoError> {
        let mut plans = self.plans.lock().await;
        let plan = plans
            .get(id)
            .ok_or_else(|| TodoError::NotFound(id.to_string()))?;
        let mut updated = plan.clone();
        updated.status = status;
        plans.insert(id.to_string(), updated);
        Ok(())
    }

    async fn update_step_status(
        &self,
        plan_id: &str,
        step_id: &str,
        status: TodoStatus,
    ) -> Result<(), TodoError> {
        let mut plans = self.plans.lock().await;
        let plan = plans
            .get(plan_id)
            .ok_or_else(|| TodoError::NotFound(plan_id.to_string()))?;
        let mut updated = plan.clone();
        let step = updated
            .steps
            .iter_mut()
            .find(|s| s.id == step_id)
            .ok_or_else(|| TodoError::NotFound(step_id.to_string()))?;
        step.status = status;
        plans.insert(plan_id.to_string(), updated);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), TodoError> {
        let mut plans = self.plans.lock().await;
        plans
            .remove(id)
            .ok_or_else(|| TodoError::NotFound(id.to_string()))?;
        Ok(())
    }
}

// ============================================================================
// TODO_STORE / PLAN_STORE typed keys
// ============================================================================

/// Typed key: `ctx.todo` 注入的 TodoStore.
pub static TODO_STORE: ma_harness_cordis::CtxKey<Arc<dyn TodoStore>> =
    ma_harness_seam::ctx_key!("todo_store");

/// Typed key: `ctx.plan` 注入的 PlanStore.
pub static PLAN_STORE: ma_harness_cordis::CtxKey<Arc<dyn PlanStore>> =
    ma_harness_seam::ctx_key!("plan_store");

// ============================================================================
// Default store type aliases
// ============================================================================

/// 平台默认 Todo store (P14.7.1: InMemoryTodoStore)
pub type DefaultTodoStore = InMemoryTodoStore;

/// 平台默认 Plan store (P14.7.1: InMemoryPlanStore)
pub type DefaultPlanStore = InMemoryPlanStore;

// ============================================================================
// 单元测试 (mod tests) — 10 个核心场景
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn todo_store_write_and_read() {
        let store = InMemoryTodoStore::new();
        let item = TodoItem::new("Fix bug #123")
            .with_priority(1)
            .with_status(TodoStatus::InProgress);
        let id = store.write(&item).await.expect("write");
        let read = store.read(&id).await.expect("read");
        assert_eq!(read.content, "Fix bug #123");
        assert_eq!(read.priority, 1);
        assert_eq!(read.status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn todo_store_update_status_validates_transitions() {
        let store = InMemoryTodoStore::new();
        let item = TodoItem::new("test");
        let id = store.write(&item).await.expect("write");

        // Pending → InProgress: OK
        store
            .update_status(&id, TodoStatus::InProgress)
            .await
            .expect("transition ok");
        // InProgress → Done: OK
        store
            .update_status(&id, TodoStatus::Done)
            .await
            .expect("transition ok");
        // Done → Pending: 拒绝 (终态)
        let err = store
            .update_status(&id, TodoStatus::Pending)
            .await
            .unwrap_err();
        assert!(matches!(err, TodoError::InvalidTransition { .. }));
    }

    #[tokio::test]
    async fn todo_store_read_all_sorted_by_priority() {
        let store = InMemoryTodoStore::new();
        store
            .write(&TodoItem::new("low").with_priority(10))
            .await
            .expect("write");
        store
            .write(&TodoItem::new("high").with_priority(1))
            .await
            .expect("write");
        let list = store.read_all().await.expect("read_all");
        let sorted = list.sorted_by_priority();
        assert_eq!(sorted[0].content, "high");
        assert_eq!(sorted[1].content, "low");
    }

    #[tokio::test]
    async fn todo_store_by_status_filter() {
        let store = InMemoryTodoStore::new();
        store
            .write(&TodoItem::new("a").with_status(TodoStatus::Pending))
            .await
            .expect("write");
        store
            .write(&TodoItem::new("b").with_status(TodoStatus::Done))
            .await
            .expect("write");
        let list = store.read_all().await.expect("read_all");
        let done = list.by_status(TodoStatus::Done);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].content, "b");
    }

    #[tokio::test]
    async fn todo_store_delete() {
        let store = InMemoryTodoStore::new();
        let id = store.write(&TodoItem::new("x")).await.expect("write");
        store.delete(&id).await.expect("delete");
        let err = store.read(&id).await.unwrap_err();
        assert!(matches!(err, TodoError::NotFound(_)));
    }

    #[tokio::test]
    async fn todo_store_empty_content_errors() {
        let store = InMemoryTodoStore::new();
        let item = TodoItem::new("   ");
        let err = store.write(&item).await.unwrap_err();
        assert!(matches!(err, TodoError::Validation(_)));
    }

    #[tokio::test]
    async fn plan_store_write_and_update_status() {
        let store = InMemoryPlanStore::new();
        let plan = Plan::new("Refactor auth module")
            .with_step(PlanStep::new("Extract user service"))
            .with_step(PlanStep::new("Migrate to JWT"));
        let id = store.write(&plan).await.expect("write");
        store
            .update_status(&id, PlanStatus::Approved)
            .await
            .expect("update");
        let read = store.read(&id).await.expect("read");
        assert_eq!(read.status, PlanStatus::Approved);
        assert_eq!(read.steps.len(), 2);
    }

    #[tokio::test]
    async fn plan_store_update_step_status() {
        let store = InMemoryPlanStore::new();
        let plan = Plan::new("Test")
            .with_step(PlanStep::new("step 1"))
            .with_step(PlanStep::new("step 2"));
        let pid = store.write(&plan).await.expect("write");
        let read = store.read(&pid).await.expect("read");
        let sid = read.steps[0].id.clone();
        store
            .update_step_status(&pid, &sid, TodoStatus::Done)
            .await
            .expect("update step");
        let after = store.read(&pid).await.expect("read");
        assert_eq!(after.steps[0].status, TodoStatus::Done);
        assert_eq!(after.steps[1].status, TodoStatus::Pending);
    }

    #[test]
    fn todo_status_can_transition() {
        use TodoStatus::*;
        assert!(Pending.can_transition_to(InProgress));
        assert!(Pending.can_transition_to(Cancelled));
        assert!(!Pending.can_transition_to(Done), "Pending 不能跳到 Done");
        assert!(InProgress.can_transition_to(Done));
        assert!(InProgress.can_transition_to(Pending), "允许回退");
        assert!(!Done.can_transition_to(Pending), "Done 终态");
        assert!(!Cancelled.can_transition_to(InProgress), "Cancelled 终态");
    }

    #[test]
    fn todo_status_from_str_roundtrip() {
        use TodoStatus::*;
        for s in [Pending, InProgress, Done, Cancelled] {
            let parsed: TodoStatus = s.as_str().parse().expect("parse");
            assert_eq!(parsed, s);
        }
    }
}
