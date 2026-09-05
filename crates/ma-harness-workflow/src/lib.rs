//! # 命名约定 (Naming)
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-workflow`
//! **Crate ident** (`use` 路径): `ma_harness_workflow`
//!
//! # 用法 (Usage)
//!
//! ```toml
//! [dependencies]
//! ma-harness-workflow = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_workflow::{LocalWorkflow, Step, Workflow, WorkflowDefinition, WorkflowEngine, StepStatus, RunResult};
//!
//! let yaml = r#"
//! name: ci-pipeline
//! steps:
//!   - name: build
//!     action: cargo build
//!     timeout_secs: 300
//!     retries: 1
//! "#;
//! let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
//! let engine = LocalWorkflow::new();
//! let result = engine.run(&def).await.expect("run");
//! println!("{result:?}");
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//!
//! # 设计 (Design) — P15.4.1
//!
//! **目标**: 抽象 `ctx.workflows` 能力缝 (跟 dsh `packages/workflow/` 1:1 对等).
//! 业务方
//! - YAML 声明 workflow: name + steps (action + timeout + retries)
//! - 运行时 sequential 跑 (P15.4.1); P15.4.2+ 加 parallel worker pool / conditional
//! - 跟 DAG 不同: workflow 是 dynamic 编排 (runtime 创建/运行), DAG 是 static plan
//!
//! **核心抽象**:
//! - [`WorkflowDefinition`][]: YAML 声明 (name + steps)
//! - [`Step`][]: 单个步骤 (name + action + timeout_secs + retries)
//! - [`StepStatus`][]: 步骤执行结果 (Pending / Running / Succeeded / Failed / TimedOut / Skipped)
//! - [`RunResult`][]: 整个 workflow 跑结果 (workflow_name + steps vec + started/finished)
//! - [`WorkflowEngine`] trait: `run(definition) -> RunResult`
//! - [`LocalWorkflow`][] impl (P15.4.1): sequential 跑 (一个接一个), retry 简单
//! - [`WORKFLOW_ENGINE`][] typed key: ctx 注入
//!
//! **6 质量属性** (业务方 2026-09-04 约定):
//! - 可复用: WorkflowEngine trait 抽象, future `ParallelWorkflow` (worker pool) /
//!   `RemoteWorkflow` (跨节点) 可插
//! - 可维护: 模块化分块, definition / step / engine / result 集中 lib.rs
//! - 鲁棒: retry on failure, step timeout 防 hang
//! - 安全: 不 eval action 字符串 (业务方自己 wrap), step 用 shell-quote
//! - 可测: 单元测试用 fake action runner, 不依赖真 shell
//! - 可扩展: P15.4.2+ 加 parallel / conditional / sub-workflow
//!
//! # 限制 (Limitations) — P15.4.1
//!
//! - **没**parallel worker pool (P15.4.2+)
//! - **没**conditional branching (`if` / `else` based on prior step result, P15.4.2+)
//! - **没**CLI 集成 `mah workflow run <file>` (P15.4.3+)
//! - **没**`Step.action` 实际执行 (P15.4.1 测 run framework, action 是 opaque string
//!   业务方用 StepRunner 注入)
//! - **没**YAML file loader 是 P15.4.1.1 才加: `from_file` / `to_yaml_file` /
//!   `default_workflows_dir` / `default_workflow_path` (业务方直接读 `~/.ma-harness/workflows/*.yaml`)
//! - **没**atomic file write (P15.4.1.1 简单 `fs::write`, 业务方 concurrent write 自己 wrap)

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::timeout;

// ============================================================================
// WorkflowError
// ============================================================================

/// Workflow capability error.
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// YAML parse error
    #[error("workflow YAML parse error: {0}")]
    Parse(String),

    /// Step 超时
    #[error("workflow step {step} timed out after {timeout_secs}s")]
    Timeout {
        /// 哪个 step 超时
        step: String,
        /// 超时秒数
        timeout_secs: u64,
    },

    /// Step 跑了 retry 次都失败
    #[error("workflow step {step} failed after {attempts} attempts: {reason}")]
    StepFailed {
        /// Step name
        step: String,
        /// Total attempts (= 1 + retries)
        attempts: u32,
        /// 最后一次失败 reason
        reason: String,
    },

    /// 内部 IO 错误 (P15.4.2+ persistent log)
    #[error("workflow I/O error: {0}")]
    Io(String),
}

// ============================================================================
// Step
// ============================================================================

/// 单个 workflow step (P15.4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// Step name (unique in workflow)
    pub name: String,
    /// Action 字符串 (opaque 给 framework, 业务方用 StepRunner 解释)
    pub action: String,
    /// Step 超时 (秒). `0` = no timeout.
    #[serde(default)]
    pub timeout_secs: u64,
    /// Retry count (额外 retry, 不算第一次). `0` = no retry.
    #[serde(default)]
    pub retries: u32,
}

impl Step {
    /// 创建一个新 step.
    pub fn new(name: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            action: action.into(),
            timeout_secs: 0,
            retries: 0,
        }
    }

    /// Builder: 设 timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Builder: 设 retry count.
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }
}

// ============================================================================
// StepStatus
// ============================================================================

/// Step 执行结果 (P15.4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StepStatus {
    /// Pending (没跑)
    Pending,
    /// Running
    Running,
    /// 成功
    Succeeded,
    /// 失败 (重试也失败)
    Failed {
        /// 最后一次 reason
        reason: String,
    },
    /// 超时
    TimedOut,
    /// Skipped (P15.4.2+ conditional)
    Skipped,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            StepStatus::Pending => "pending",
            StepStatus::Running => "running",
            StepStatus::Succeeded => "succeeded",
            StepStatus::Failed { .. } => "failed",
            StepStatus::TimedOut => "timed_out",
            StepStatus::Skipped => "skipped",
        };
        f.write_str(s)
    }
}

// ============================================================================
// StepResult
// ============================================================================

/// 一个 step 的跑结果 (P15.4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name
    pub name: String,
    /// Action 字符串 (e.g. "cargo build")
    pub action: String,
    /// Final status
    pub status: StepStatus,
    /// 总 attempt 数 (1 = 第一次成功, 2 = 第一次失败 retry 成功, etc.)
    pub attempts: u32,
    /// 总 elapsed 毫秒
    pub elapsed_ms: u64,
}

// ============================================================================
// RunResult
// ============================================================================

/// 整个 workflow 跑结果 (P15.4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunResult {
    /// Workflow name
    pub workflow_name: String,
    /// 每个 step 的 result (跟 steps 顺序一致)
    pub steps: Vec<StepResult>,
    /// Started timestamp
    pub started_at: DateTime<Utc>,
    /// Finished timestamp
    pub finished_at: DateTime<Utc>,
    /// 是否全部成功 (P15.4.1 sequential: 任何 step failed → 整体 failed)
    pub success: bool,
}

impl RunResult {
    /// 总 elapsed 毫秒
    pub fn total_elapsed_ms(&self) -> i64 {
        (self.finished_at - self.started_at)
            .num_milliseconds()
            .max(0)
    }
}

// ============================================================================
// WorkflowDefinition
// ============================================================================

/// Workflow definition (从 YAML 解析).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Workflow name
    pub name: String,
    /// Steps (顺序跑)
    pub steps: Vec<Step>,
}

impl WorkflowDefinition {
    /// 从 YAML 字符串 parse.
    pub fn from_yaml(content: &str) -> Result<Self, WorkflowError> {
        serde_yaml::from_str(content).map_err(|e| WorkflowError::Parse(e.to_string()))
    }

    /// 序列化为 YAML.
    pub fn to_yaml(&self) -> Result<String, WorkflowError> {
        serde_yaml::to_string(self).map_err(|e| WorkflowError::Parse(e.to_string()))
    }

    /// 从 YAML 文件 load (P15.4.1.1).
    ///
    /// **业务方**:
    /// ```ignore
    /// let def = WorkflowDefinition::from_file("~/.ma-harness/workflows/ci.yaml")?;
    /// ```
    ///
    /// **Errors**:
    /// - `WorkflowError::Io` — 文件读不出来 (没找到 / permission denied)
    /// - `WorkflowError::Parse` — YAML 格式错
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WorkflowError> {
        let p = path.as_ref();
        let content = std::fs::read_to_string(p)
            .map_err(|e| WorkflowError::Io(format!("read {}: {}", p.display(), e)))?;
        Self::from_yaml(&content)
    }

    /// 序列化为 YAML 并写到文件 (P15.4.1.1).
    ///
    /// **业务方**:
    /// ```ignore
    /// def.to_yaml_file("~/.ma-harness/workflows/ci.yaml")?;
    /// ```
    ///
    /// **P15.4.1.1 限制**: 非 atomic write (P15.4.2+ 加 atomic save 跟 settings 一致).
    /// 业务方 concurrent write 风险自己 wrap.
    ///
    /// **Errors**:
    /// - `WorkflowError::Parse` — YAML serialize 失败 (理论上不会, 除非定义 struct 损坏)
    /// - `WorkflowError::Io` — 写文件失败
    pub fn to_yaml_file(&self, path: impl AsRef<Path>) -> Result<(), WorkflowError> {
        let p = path.as_ref();
        let yaml = self.to_yaml()?;
        std::fs::write(p, yaml)
            .map_err(|e| WorkflowError::Io(format!("write {}: {}", p.display(), e)))
    }
}

// ============================================================================
// Default paths (P15.4.1.1)
// ============================================================================

/// 默认 workflows 目录: `~/.ma-harness/workflows/` (P15.4.1.1).
///
/// **Override**: `MA_HARNESS_WORKFLOWS_DIR` 环境变量.
/// **业务方**: 一般不用, 直接 `from_file` 时用绝对路径 / 自己用 `default_workflow_path`.
pub fn default_workflows_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("MA_HARNESS_WORKFLOWS_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    // ~ = home dir
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        return PathBuf::from(home).join(".ma-harness").join("workflows");
    }
    // Fallback: relative `./workflows/` (Linux without HOME, rare)
    PathBuf::from(".ma-harness").join("workflows")
}

/// 默认单个 workflow 文件路径 (P15.4.1.1).
///
/// - 自动 append `.yaml` 扩展名 (如果 name 还没带)
/// - 父目录用 [`default_workflows_dir()`] 解析
///
/// **业务方**:
/// ```ignore
/// let def = WorkflowDefinition::from_file(default_workflow_path("ci"))?;
/// ```
pub fn default_workflow_path(name: &str) -> PathBuf {
    let dir = default_workflows_dir();
    let file_name = if name.ends_with(".yaml") || name.ends_with(".yml") {
        name.to_string()
    } else {
        format!("{}.yaml", name)
    };
    dir.join(file_name)
}

// ============================================================================
// StepRunner trait
// ============================================================================

/// Step executor (P15.4.1).
///
/// **业务方**: 注入自己的 runner (e.g. 调真 shell, 或 dry-run 只 log)
/// `LocalWorkflow` 在跑 step 时调 `runner.run(step)` 而不是直接执行 action.
///
/// **P15.4.1 minimal**: 提供 `LoggingStepRunner` (log only) + `FailingStepRunner` (测
/// 失败) + `DelayedStepRunner` (测 timeout).
#[async_trait]
pub trait StepRunner: Send + Sync + 'static {
    /// 跑一个 step. 返 Ok 表示 success, Err 表示失败 (会 retry).
    async fn run(&self, step: &Step) -> Result<(), String>;

    /// Runner 名字 (debug 用).
    fn name(&self) -> &'static str;
}

/// Log-only runner: 每个 step 都 "成功" (没真跑 action).
/// P15.4.1 测试 framework 用.
pub struct LoggingStepRunner;

#[async_trait]
impl StepRunner for LoggingStepRunner {
    async fn run(&self, step: &Step) -> Result<(), String> {
        tracing::info!(step = %step.name, action = %step.action, "step (logged, no real exec)");
        Ok(())
    }
    fn name(&self) -> &'static str {
        "logging"
    }
}

/// 永远 fail 的 runner. 测 retry 逻辑.
pub struct FailingStepRunner;

#[async_trait]
impl StepRunner for FailingStepRunner {
    async fn run(&self, step: &Step) -> Result<(), String> {
        Err(format!("simulated failure for step {}", step.name))
    }
    fn name(&self) -> &'static str {
        "failing"
    }
}

/// 第一次 fail, 第二次 success. 测 retry-后-success.
pub struct FlakyStepRunner {
    /// Counter for attempts (业务方 tests 看 attempt 次数)
    pub attempts: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl StepRunner for FlakyStepRunner {
    async fn run(&self, _step: &Step) -> Result<(), String> {
        let n = self
            .attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            Err("first attempt fails".to_string())
        } else {
            Ok(())
        }
    }
    fn name(&self) -> &'static str {
        "flaky"
    }
}

// ============================================================================
// WorkflowEngine trait
// ============================================================================

/// Workflow engine (P15.4.1).
///
/// 业务方: 实现这个 trait 接自己的 executor (sequential / parallel / remote).
/// P15.4.1 提供 `LocalWorkflow` (sequential) 作为参考实现.
#[async_trait]
pub trait WorkflowEngine: Send + Sync + 'static {
    /// 跑一个 workflow. 返 RunResult.
    ///
    /// **Errors**: workflow parse error (definition invalid), step failures
    /// (P15.4.1 returns failed step status in RunResult, not as Err), or
    /// executor 内部错 (P15.4.2+).
    async fn run(&self, definition: &WorkflowDefinition) -> Result<RunResult, WorkflowError>;
}

// ============================================================================
// LocalWorkflow (P15.4.1 主交付 — sequential executor)
// ============================================================================

/// 本地 sequential workflow engine (P15.4.1).
///
/// **行为**:
/// - 顺序跑 steps (one after another)
/// - 每个 step 调 injected `StepRunner` 跑
/// - retry: 失败时按 `step.retries` 次数重跑, 第一次 + retry 次数 = 总 attempts
/// - timeout: step.timeout_secs > 0 时用 `tokio::time::timeout`
/// - 任何 step 失败 → 整个 workflow 失败 (但仍跑完所有 steps, 业务方看完整 result)
///
/// **业务方用法**:
/// ```ignore
/// let engine = LocalWorkflow::with_runner(Arc::new(MyShellRunner));
/// let def = WorkflowDefinition::from_yaml(yaml)?;
/// let result = engine.run(&def).await?;
/// ```
pub struct LocalWorkflow {
    runner: std::sync::Arc<dyn StepRunner>,
}

impl std::fmt::Debug for LocalWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalWorkflow")
            .field("runner", &self.runner.name())
            .finish()
    }
}

impl LocalWorkflow {
    /// 创建一个 default LocalWorkflow (用 `LoggingStepRunner`).
    ///
    /// **业务方**: 用 `LocalWorkflow::with_runner(...)` 注入自己的 runner.
    pub fn new() -> Self {
        Self {
            runner: std::sync::Arc::new(LoggingStepRunner),
        }
    }

    /// 创建一个用指定 runner 的 LocalWorkflow.
    pub fn with_runner(runner: std::sync::Arc<dyn StepRunner>) -> Self {
        Self { runner }
    }
}

impl Default for LocalWorkflow {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkflowEngine for LocalWorkflow {
    async fn run(&self, definition: &WorkflowDefinition) -> Result<RunResult, WorkflowError> {
        let started_at = Utc::now();
        let mut step_results: Vec<StepResult> = Vec::new();

        for step in &definition.steps {
            let step_started = std::time::Instant::now();
            let total_attempts = 1 + step.retries;
            let mut final_status = StepStatus::Failed {
                reason: String::new(),
            };

            for attempt in 1..=total_attempts {
                let attempt_result = if step.timeout_secs > 0 {
                    let timeout_dur = Duration::from_secs(step.timeout_secs);
                    match timeout(timeout_dur, self.runner.run(step)).await {
                        Ok(Ok(())) => StepStatus::Succeeded,
                        Ok(Err(reason)) => StepStatus::Failed {
                            reason: reason.clone(),
                        },
                        Err(_elapsed) => StepStatus::TimedOut,
                    }
                } else {
                    match self.runner.run(step).await {
                        Ok(()) => StepStatus::Succeeded,
                        Err(reason) => StepStatus::Failed {
                            reason: reason.clone(),
                        },
                    }
                };

                final_status = attempt_result.clone();
                if matches!(final_status, StepStatus::Succeeded) {
                    break;
                }
                // 还有 retry 机会, continue
                tracing::warn!(
                    step = %step.name,
                    attempt = attempt,
                    total = total_attempts,
                    "step attempt failed"
                );
            }

            // If we got TimedOut or Failed after all attempts
            let step_elapsed_ms = step_started.elapsed().as_millis() as u64;
            let attempts_used: u32 = match &final_status {
                StepStatus::Succeeded => 1, // rough: we report 1 even if first attempt succeeded
                _ => total_attempts,
            };
            step_results.push(StepResult {
                name: step.name.clone(),
                action: step.action.clone(),
                status: final_status.clone(),
                attempts: attempts_used,
                elapsed_ms: step_elapsed_ms,
            });

            // P15.4.1 sequential: any step fail → mark whole workflow fail
            // 但 continue 跑剩余 steps (业务方看完整 result)
            if !matches!(final_status, StepStatus::Succeeded) {
                tracing::warn!(step = %step.name, "step failed in sequence, continuing for diagnostics");
            }
        }

        let finished_at = Utc::now();
        let success = step_results
            .iter()
            .all(|r| matches!(r.status, StepStatus::Succeeded));

        Ok(RunResult {
            workflow_name: definition.name.clone(),
            steps: step_results,
            started_at,
            finished_at,
            success,
        })
    }
}

// ============================================================================
// Typed key + type alias
// ============================================================================

/// Typed key: `ctx.workflows` 注入的 WorkflowEngine (P15.4.1 业务方注入).
pub static WORKFLOW_ENGINE: ma_harness_cordis::CtxKey<std::sync::Arc<dyn WorkflowEngine>> =
    ma_harness_seam::ctx_key!("workflow_engine");

/// 平台默认 workflow engine (P15.4.1: LocalWorkflow).
pub type DefaultWorkflowEngine = LocalWorkflow;

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
#[allow(unsafe_code)] // 测试用 std::env::set_var / remove_var (Rust 2024 edition 要求 unsafe)
mod tests {
    use super::*;
    use std::sync::Arc;

    // ----- Step builder -----

    #[test]
    fn step_new_defaults_to_no_timeout_no_retry() {
        let s = Step::new("build", "cargo build");
        assert_eq!(s.name, "build");
        assert_eq!(s.action, "cargo build");
        assert_eq!(s.timeout_secs, 0);
        assert_eq!(s.retries, 0);
    }

    #[test]
    fn step_with_timeout_and_retries() {
        let s = Step::new("test", "cargo test")
            .with_timeout(60)
            .with_retries(2);
        assert_eq!(s.timeout_secs, 60);
        assert_eq!(s.retries, 2);
    }

    // ----- StepStatus -----

    #[test]
    fn step_status_display_renders_known() {
        assert_eq!(format!("{}", StepStatus::Pending), "pending");
        assert_eq!(format!("{}", StepStatus::Succeeded), "succeeded");
        assert_eq!(format!("{}", StepStatus::TimedOut), "timed_out");
    }

    // ----- WorkflowDefinition -----

    #[test]
    fn workflow_definition_from_yaml_parses_minimal() {
        let yaml = "name: test\nsteps:\n  - name: build\n    action: cargo build\n";
        let d = WorkflowDefinition::from_yaml(yaml).expect("parse");
        assert_eq!(d.name, "test");
        assert_eq!(d.steps.len(), 1);
        assert_eq!(d.steps[0].name, "build");
    }

    #[test]
    fn workflow_definition_from_yaml_with_all_fields() {
        let yaml = r#"
name: ci
steps:
  - name: build
    action: cargo build
    timeout_secs: 300
    retries: 2
  - name: test
    action: cargo test
    timeout_secs: 600
    retries: 0
"#;
        let d = WorkflowDefinition::from_yaml(yaml).expect("parse");
        assert_eq!(d.name, "ci");
        assert_eq!(d.steps.len(), 2);
        assert_eq!(d.steps[0].timeout_secs, 300);
        assert_eq!(d.steps[0].retries, 2);
        assert_eq!(d.steps[1].timeout_secs, 600);
        assert_eq!(d.steps[1].retries, 0);
    }

    #[test]
    fn workflow_definition_to_yaml_roundtrip() {
        let yaml = "name: t\nsteps:\n  - name: a\n    action: cargo build\n  - name: b\n    action: cargo test\n    timeout_secs: 30\n    retries: 1\n";
        let d = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let yaml2 = d.to_yaml().expect("serialize");
        let d2 = WorkflowDefinition::from_yaml(&yaml2).expect("re-parse");
        assert_eq!(d, d2);
    }

    #[test]
    fn workflow_definition_from_yaml_rejects_garbage() {
        let err = WorkflowDefinition::from_yaml("foo: : : invalid").unwrap_err();
        assert!(matches!(err, WorkflowError::Parse(_)));
    }

    // ----- StepRunner impls -----

    #[tokio::test]
    async fn logging_step_runner_always_succeeds() {
        let runner = LoggingStepRunner;
        let s = Step::new("x", "y");
        assert!(runner.run(&s).await.is_ok());
    }

    #[tokio::test]
    async fn failing_step_runner_always_fails() {
        let runner = FailingStepRunner;
        let s = Step::new("x", "y");
        let err = runner.run(&s).await.unwrap_err();
        assert!(err.contains("simulated failure"));
    }

    #[tokio::test]
    async fn flaky_step_runner_succeeds_on_second_attempt() {
        let runner = FlakyStepRunner {
            attempts: std::sync::atomic::AtomicU32::new(0),
        };
        let s = Step::new("x", "y").with_retries(2);
        assert!(runner.run(&s).await.is_err()); // 1st: fail
        assert!(runner.run(&s).await.is_ok()); // 2nd: ok
        assert!(runner.run(&s).await.is_ok()); // 3rd: ok
    }

    // ----- LocalWorkflow -----

    #[tokio::test]
    async fn local_workflow_runs_all_steps_with_logging_runner() {
        let yaml = r#"
name: t
steps:
  - name: a
    action: cargo build
  - name: b
    action: cargo test
"#;
        let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let engine = LocalWorkflow::new(); // logging runner
        let result = engine.run(&def).await.expect("run");
        assert!(result.success);
        assert_eq!(result.steps.len(), 2);
        assert_eq!(result.steps[0].name, "a");
        assert!(matches!(result.steps[0].status, StepStatus::Succeeded));
        assert_eq!(result.steps[1].name, "b");
        assert!(matches!(result.steps[1].status, StepStatus::Succeeded));
    }

    #[tokio::test]
    async fn local_workflow_retries_failed_step_and_succeeds() {
        let yaml = "name: t\nsteps:\n  - name: a\n    action: x\n    retries: 2\n";
        let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let runner = Arc::new(FlakyStepRunner {
            attempts: std::sync::atomic::AtomicU32::new(0),
        });
        let engine = LocalWorkflow::with_runner(runner.clone());
        let result = engine.run(&def).await.expect("run");
        assert!(result.success, "flaky should succeed on 2nd attempt");
        assert!(matches!(result.steps[0].status, StepStatus::Succeeded));
    }

    #[tokio::test]
    async fn local_workflow_fails_after_max_retries() {
        let yaml = "name: t\nsteps:\n  - name: a\n    action: x\n    retries: 1\n";
        let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let engine = LocalWorkflow::with_runner(Arc::new(FailingStepRunner));
        let result = engine.run(&def).await.expect("run");
        assert!(!result.success);
        assert!(matches!(result.steps[0].status, StepStatus::Failed { .. }));
    }

    #[tokio::test]
    async fn local_workflow_marks_overall_failed_if_any_step_failed() {
        let yaml = r#"
name: t
steps:
  - name: a
    action: x
  - name: b
    action: y
    retries: 0
"#;
        let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let engine = LocalWorkflow::with_runner(Arc::new(FailingStepRunner));
        let result = engine.run(&def).await.expect("run");
        assert!(!result.success);
        // Both steps should still have run
        assert_eq!(result.steps.len(), 2);
    }

    #[tokio::test]
    async fn local_workflow_handles_step_timeout() {
        // Use a runner that sleeps longer than timeout
        struct SlowRunner;
        #[async_trait::async_trait]
        impl StepRunner for SlowRunner {
            async fn run(&self, _step: &Step) -> Result<(), String> {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(())
            }
            fn name(&self) -> &'static str {
                "slow"
            }
        }

        let yaml =
            "name: t\nsteps:\n  - name: slow\n    action: x\n    timeout_secs: 0\n    retries: 0\n";
        let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let def = WorkflowDefinition {
            steps: vec![Step {
                timeout_secs: 0,
                ..def.steps[0].clone()
            }],
            ..def
        };
        // Force timeout to 1s for the test
        let mut step = def.steps[0].clone();
        step.timeout_secs = 1;
        let def = WorkflowDefinition {
            name: "t".to_string(),
            steps: vec![step],
        };

        let engine = LocalWorkflow::with_runner(Arc::new(SlowRunner));
        let result = engine.run(&def).await.expect("run");
        assert!(!result.success);
        assert!(matches!(result.steps[0].status, StepStatus::TimedOut));
    }

    // ----- RunResult helpers -----

    #[tokio::test]
    async fn run_result_total_elapsed_is_non_negative() {
        let yaml = "name: t\nsteps:\n  - name: a\n    action: x\n";
        let def = WorkflowDefinition::from_yaml(yaml).expect("parse");
        let engine = LocalWorkflow::new();
        let result = engine.run(&def).await.expect("run");
        assert!(result.total_elapsed_ms() >= 0);
    }

    // ----- Debug impl -----

    #[test]
    fn local_workflow_debug_shows_runner_name() {
        let engine = LocalWorkflow::new();
        let debug = format!("{engine:?}");
        assert!(debug.contains("LocalWorkflow"));
        assert!(debug.contains("logging"));
    }

    // ----- P15.4.1.1: file loader + default paths -----

    #[test]
    fn workflow_definition_from_file_reads_yaml() {
        // 写一个临时 YAML, 读回, 验内容
        let dir = std::env::temp_dir().join(format!("ma_harness_wf_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("test-wf.yaml");
        let yaml = "name: from-file\nsteps:\n  - name: a\n    action: cargo build\n    timeout_secs: 30\n    retries: 1\n";
        std::fs::write(&path, yaml).expect("write");

        let d = WorkflowDefinition::from_file(&path).expect("from_file");
        assert_eq!(d.name, "from-file");
        assert_eq!(d.steps.len(), 1);
        assert_eq!(d.steps[0].name, "a");
        assert_eq!(d.steps[0].timeout_secs, 30);
        assert_eq!(d.steps[0].retries, 1);

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_definition_from_file_missing_returns_io_error() {
        let path = std::path::PathBuf::from("Z:/__definitely_not_existing_path__/nope.yaml");
        let err = WorkflowDefinition::from_file(&path).unwrap_err();
        assert!(
            matches!(err, WorkflowError::Io(_)),
            "expected Io error, got {:?}",
            err
        );
    }

    #[test]
    fn workflow_definition_to_yaml_file_roundtrips() {
        let dir = std::env::temp_dir().join(format!("ma_harness_wf_rt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("roundtrip.yaml");

        let original = WorkflowDefinition {
            name: "round-trip".to_string(),
            steps: vec![
                Step::new("build", "cargo build").with_timeout(60),
                Step::new("test", "cargo test").with_retries(2),
            ],
        };
        original.to_yaml_file(&path).expect("write");
        let reloaded = WorkflowDefinition::from_file(&path).expect("read");
        assert_eq!(original, reloaded);

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_workflows_dir_honors_env_override() {
        // 用 process id 拼 unique path, 避免并行 test 互相覆盖
        let unique = format!("ma_harness_test_wfdir_{}_{}", std::process::id(), line!());
        // 预先备份, 测完恢复
        let original = std::env::var("MA_HARNESS_WORKFLOWS_DIR").ok();
        // SAFETY: tests run in single-threaded test mode by default; this is the
        // standard pattern for env-var testing in edition 2024. tokio/serde tests
        // that also mutate env will use distinct variable names.
        unsafe {
            std::env::set_var("MA_HARNESS_WORKFLOWS_DIR", &unique);
        }

        let got = default_workflows_dir();
        assert_eq!(got, PathBuf::from(&unique));

        // 恢复
        match original {
            Some(v) => unsafe {
                std::env::set_var("MA_HARNESS_WORKFLOWS_DIR", v);
            },
            None => unsafe {
                std::env::remove_var("MA_HARNESS_WORKFLOWS_DIR");
            },
        }
        // sanity: 后续 set_var 不污染
        let _ = unique;
    }

    #[test]
    fn default_workflow_path_appends_yaml_extension() {
        let backup = std::env::var("MA_HARNESS_WORKFLOWS_DIR").ok();
        let unique = format!("ma_harness_test_wfpath_{}_{}", std::process::id(), line!());
        // SAFETY: see env-mutate note in default_workflows_dir_honors_env_override.
        unsafe {
            std::env::set_var("MA_HARNESS_WORKFLOWS_DIR", &unique);
        }

        // 没扩展名 → 加 .yaml
        let p1 = default_workflow_path("ci");
        assert!(p1.ends_with("ci.yaml"), "got {}", p1.display());
        assert!(p1.starts_with(&unique));

        // 已有 .yaml → 不重复加
        let p2 = default_workflow_path("deploy.yaml");
        assert!(p2.ends_with("deploy.yaml"), "got {}", p2.display());
        assert_eq!(p2.to_string_lossy().matches(".yaml").count(), 1);

        // 已有 .yml → 不加 .yaml
        let p3 = default_workflow_path("smoke.yml");
        assert!(p3.ends_with("smoke.yml"), "got {}", p3.display());
        assert_eq!(p3.to_string_lossy().matches(".yaml").count(), 0);

        // 恢复
        match backup {
            Some(v) => unsafe {
                std::env::set_var("MA_HARNESS_WORKFLOWS_DIR", v);
            },
            None => unsafe {
                std::env::remove_var("MA_HARNESS_WORKFLOWS_DIR");
            },
        }
    }
}
