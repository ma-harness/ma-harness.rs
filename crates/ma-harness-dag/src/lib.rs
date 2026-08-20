//! # ma-harness DAG 任务编排 (P12-9)
//!
//! 业务方多 Agent 拓扑 (DAG) 而非简单 fork. P11-10 推到 P12-9.
//!
//! ## v1 简化
//!
//! - YAML 描述 (tasks + dependencies)
//! - 拓扑排序 (Kahn's algorithm)
//! - 调度器 (按顺序跑 task)
//! - 状态跟踪 (Pending / Running / Completed / Failed)
//! - 失败短路 (下游 task 自动 skip)
//!
//! ## 后续 v2
//!
//! - 状态持久化 (DAG 中断 / 恢复)
//! - 失败重试 + 指数 backoff
//! - Web UI 拓扑图

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

/// Task 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 待跑
    Pending,
    /// 跑中
    Running,
    /// 跑完
    Completed,
    /// 失败
    Failed,
    /// 被上游失败短路 skip
    Skipped,
}

/// DAG task 定义 (YAML 业务方写)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    /// Task 名 (DAG 内 unique)
    pub name: String,
    /// Task 描述
    #[serde(default)]
    pub description: String,
    /// 依赖的 task 名列表 (空 = 根 task, 可并行)
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// 业务方要跑的 command / prompt (P12-9 v1: 简化为 string)
    pub command: String,
}

/// DAG (YAML 业务方写)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dag {
    /// DAG 名
    pub name: String,
    /// DAG 描述
    #[serde(default)]
    pub description: String,
    /// Task 列表
    pub tasks: Vec<Task>,
}

/// 运行时 task 状态 (含 状态 / 时间戳 / 结果)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRun {
    /// Task 名
    pub name: String,
    /// 业务方原始定义
    pub task: Task,
    /// 业务方当前状态
    pub status: TaskStatus,
    /// 业务方开始时间
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// 业务方结束时间
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    /// 业务方输出 / 错误
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// DAG 运行时状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DagRun {
    /// DAG 名
    pub name: String,
    /// 业务方 run id (unique)
    pub run_id: String,
    /// 业务方开始时间
    pub started_at: DateTime<Utc>,
    /// 业务方结束时间
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    /// 业务方 task run 列表
    pub tasks: BTreeMap<String, TaskRun>,
}

impl DagRun {
    /// 业务方构造新 run
    pub fn new(dag: &Dag) -> Self {
        let mut tasks = BTreeMap::new();
        for t in &dag.tasks {
            tasks.insert(
                t.name.clone(),
                TaskRun {
                    name: t.name.clone(),
                    task: t.clone(),
                    status: TaskStatus::Pending,
                    started_at: None,
                    ended_at: None,
                    output: None,
                },
            );
        }
        Self {
            name: dag.name.clone(),
            run_id: Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            ended_at: None,
            tasks,
        }
    }

    /// 业务方按名找 task run
    pub fn get(&self, name: &str) -> Option<&TaskRun> {
        self.tasks.get(name)
    }

    /// 业务方按名找 task run (mut)
    pub fn get_mut(&mut self, name: &str) -> Option<&mut TaskRun> {
        self.tasks.get_mut(name)
    }

    /// 业务方完成的 task 数
    pub fn completed_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .count()
    }

    /// 业务方失败的 task 数
    pub fn failed_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Failed)
            .count()
    }

    /// 业务方总数
    pub fn total_count(&self) -> usize {
        self.tasks.len()
    }

    /// 业务方是否全完 (Completed / Failed / Skipped)
    pub fn is_complete(&self) -> bool {
        self.tasks.values().all(|t| {
            matches!(
                t.status,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Skipped
            )
        })
    }

    /// 业务方是否成功
    pub fn is_success(&self) -> bool {
        self.tasks
            .values()
            .all(|t| t.status == TaskStatus::Completed)
    }
}

/// DAG 错误
#[derive(Debug, Error)]
pub enum DagError {
    /// 重复 task 名
    #[error("duplicate task name: {0}")]
    DuplicateTask(String),
    /// Task 引用未知依赖
    #[error("task {task} depends on unknown task: {dep}")]
    UnknownDep {
        /// Task 名
        task: String,
        /// 未知依赖
        dep: String,
    },
    /// 循环依赖
    #[error("cycle detected involving task: {0}")]
    Cycle(String),
    /// YAML 解析
    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    /// IO 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 错误
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// DAG 结果
pub type Result<T> = std::result::Result<T, DagError>;

/// DAG 调度器 (P12-9 v1)
pub struct DagScheduler;

impl DagScheduler {
    /// 校验 DAG (无重复 task, 所有依赖存在, 无循环)
    pub fn validate(dag: &Dag) -> Result<()> {
        let mut names: HashSet<&str> = HashSet::new();
        for t in &dag.tasks {
            if !names.insert(t.name.as_str()) {
                return Err(DagError::DuplicateTask(t.name.clone()));
            }
        }
        for t in &dag.tasks {
            for dep in &t.depends_on {
                if !names.contains(dep.as_str()) {
                    return Err(DagError::UnknownDep {
                        task: t.name.clone(),
                        dep: dep.clone(),
                    });
                }
            }
        }
        // 循环检测 (Kahn's algorithm)
        let _ = Self::topological_order(dag)?;
        Ok(())
    }

    /// 拓扑排序 (Kahn's algorithm) — 返 task 名列表, 按执行顺序
    pub fn topological_order(dag: &Dag) -> Result<Vec<String>> {
        // 1. 算 in-degree
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        for t in &dag.tasks {
            in_degree.insert(t.name.as_str(), t.depends_on.len());
        }
        // 2. queue: 根 task (in-degree = 0)
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, d)| *d == 0)
            .map(|(n, _)| *n)
            .collect();
        let mut order = Vec::new();
        // 3. BFS: pop root, 减下游 in-degree, 新 root 入 queue
        while let Some(n) = queue.pop() {
            order.push(n.to_string());
            for t in &dag.tasks {
                if t.depends_on.iter().any(|d| d == n) {
                    let d = in_degree.get_mut(t.name.as_str()).expect("entry");
                    *d -= 1;
                    if *d == 0 {
                        queue.push(t.name.as_str());
                    }
                }
            }
        }
        if order.len() != dag.tasks.len() {
            // 找剩余的 task (in cycle)
            let remaining: Vec<&str> = in_degree
                .iter()
                .filter(|&(_, d)| *d > 0)
                .map(|(n, _)| *n)
                .collect();
            return Err(DagError::Cycle(remaining.join(", ")));
        }
        Ok(order)
    }

    /// 准备下一批可跑 task (依赖都已 Completed)
    ///
    /// 业务方调度器循环调, 跑完一批后调下一次
    pub fn next_batch(run: &DagRun) -> Vec<String> {
        let completed: HashSet<&str> = run
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .map(|t| t.name.as_str())
            .collect();

        run.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .filter(|t| {
                t.task
                    .depends_on
                    .iter()
                    .all(|d| completed.contains(d.as_str()))
            })
            .map(|t| t.name.clone())
            .collect()
    }

    /// 模拟跑 (v1: 不真跑 command, 标 Completed)
    ///
    /// 业务方 v2 替换为: 真 invoke agent / subprocess
    pub async fn execute_task(run: &mut DagRun, task_name: &str) {
        let task = match run.get(task_name) {
            Some(t) => t.task.clone(),
            None => return,
        };
        // 标 Running
        if let Some(t) = run.get_mut(task_name) {
            t.status = TaskStatus::Running;
            t.started_at = Some(Utc::now());
        }
        eprintln!("[dag] running task: {task_name} ({})", task.command);
        // 模拟延迟
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // 标 Completed (v1: 永远成功)
        if let Some(t) = run.get_mut(task_name) {
            t.status = TaskStatus::Completed;
            t.ended_at = Some(Utc::now());
            t.output = Some(format!("simulated output for: {}", task.command));
        }
    }

    /// 短路下游 (当 task 失败)
    pub fn short_circuit(run: &mut DagRun, failed_task: &str) {
        // 找所有直接/间接依赖 failed_task 的 task, 标 Skipped
        let mut to_skip: HashSet<String> = HashSet::new();
        let mut stack = vec![failed_task.to_string()];
        while let Some(t) = stack.pop() {
            for task in run.tasks.values() {
                if task.task.depends_on.iter().any(|d| d == &t) && to_skip.insert(task.name.clone())
                {
                    stack.push(task.name.clone());
                }
            }
        }
        for name in to_skip {
            if let Some(t) = run.get_mut(&name) {
                if t.status == TaskStatus::Pending {
                    t.status = TaskStatus::Skipped;
                }
            }
        }
    }
}

/// 从 YAML 文件加载 DAG
pub fn load_dag_from_file(path: impl AsRef<std::path::Path>) -> Result<Dag> {
    let content = std::fs::read_to_string(path)?;
    let dag: Dag = serde_yaml::from_str(&content)?;
    Ok(dag)
}

/// 从 YAML 字符串加载 DAG
pub fn load_dag_from_str(s: &str) -> Result<Dag> {
    let dag: Dag = serde_yaml::from_str(s)?;
    Ok(dag)
}

/// 跑 DAG (P12-9 v1: 模拟跑, 标 Completed)
pub async fn run_dag(dag: &Dag) -> Result<DagRun> {
    DagScheduler::validate(dag)?;
    let mut run = DagRun::new(dag);
    while !run.is_complete() {
        let batch = DagScheduler::next_batch(&run);
        if batch.is_empty() {
            // 无可跑 task, 终止
            break;
        }
        for name in batch {
            DagScheduler::execute_task(&mut run, &name).await;
        }
    }
    run.ended_at = Some(Utc::now());
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dag() -> Dag {
        load_dag_from_str(
            r#"
name: "test-dag"
description: "Test DAG"
tasks:
  - name: "fetch"
    command: "curl https://example.com"
  - name: "parse"
    depends_on: ["fetch"]
    command: "parse the html"
  - name: "store"
    depends_on: ["parse"]
    command: "store in db"
"#,
        )
        .unwrap()
    }

    #[test]
    fn load_dag_from_str_minimal() {
        let d = sample_dag();
        assert_eq!(d.name, "test-dag");
        assert_eq!(d.tasks.len(), 3);
    }

    #[test]
    fn validate_dag_ok() {
        let d = sample_dag();
        assert!(DagScheduler::validate(&d).is_ok());
    }

    #[test]
    fn validate_duplicate_task() {
        let toml = r#"
name: "dup"
tasks:
  - name: "a"
    command: "x"
  - name: "a"
    command: "y"
"#;
        let d = load_dag_from_str(toml).unwrap();
        let err = DagScheduler::validate(&d).unwrap_err();
        assert!(matches!(err, DagError::DuplicateTask(_)));
    }

    #[test]
    fn validate_unknown_dep() {
        let toml = r#"
name: "missing"
tasks:
  - name: "a"
    command: "x"
    depends_on: ["nonexistent"]
"#;
        let d = load_dag_from_str(toml).unwrap();
        let err = DagScheduler::validate(&d).unwrap_err();
        assert!(matches!(err, DagError::UnknownDep { .. }));
    }

    #[test]
    fn validate_cycle() {
        let toml = r#"
name: "cycle"
tasks:
  - name: "a"
    command: "x"
    depends_on: ["b"]
  - name: "b"
    command: "y"
    depends_on: ["a"]
"#;
        let d = load_dag_from_str(toml).unwrap();
        let err = DagScheduler::validate(&d).unwrap_err();
        assert!(matches!(err, DagError::Cycle(_)));
    }

    #[test]
    fn topological_order_linear() {
        let d = sample_dag();
        let order = DagScheduler::topological_order(&d).unwrap();
        assert_eq!(order, vec!["fetch", "parse", "store"]);
    }

    #[test]
    fn topological_order_diamond() {
        let toml = r#"
name: "diamond"
tasks:
  - name: "a"
    command: "a"
  - name: "b"
    command: "b"
    depends_on: ["a"]
  - name: "c"
    command: "c"
    depends_on: ["a"]
  - name: "d"
    command: "d"
    depends_on: ["b", "c"]
"#;
        let d = load_dag_from_str(toml).unwrap();
        let order = DagScheduler::topological_order(&d).unwrap();
        // a 必须先, d 必须最后
        let a_idx = order.iter().position(|n| n == "a").unwrap();
        let d_idx = order.iter().position(|n| n == "d").unwrap();
        assert!(a_idx < d_idx);
    }

    #[test]
    fn next_batch_returns_roots() {
        let d = sample_dag();
        let run = DagRun::new(&d);
        let batch = DagScheduler::next_batch(&run);
        assert_eq!(batch, vec!["fetch".to_string()]);
    }

    #[test]
    fn next_batch_after_completion() {
        let d = sample_dag();
        let mut run = DagRun::new(&d);
        // 标 fetch 完成
        run.get_mut("fetch").unwrap().status = TaskStatus::Completed;
        let batch = DagScheduler::next_batch(&run);
        assert_eq!(batch, vec!["parse".to_string()]);
        // 标 parse 完成
        run.get_mut("parse").unwrap().status = TaskStatus::Completed;
        let batch = DagScheduler::next_batch(&run);
        assert_eq!(batch, vec!["store".to_string()]);
    }

    #[test]
    fn short_circuit_marks_downstream() {
        let d = sample_dag();
        let mut run = DagRun::new(&d);
        // 标 fetch Failed
        run.get_mut("fetch").unwrap().status = TaskStatus::Failed;
        DagScheduler::short_circuit(&mut run, "fetch");
        // parse 跟 store 应标 Skipped
        assert_eq!(run.get("parse").unwrap().status, TaskStatus::Skipped);
        assert_eq!(run.get("store").unwrap().status, TaskStatus::Skipped);
    }

    #[tokio::test]
    async fn run_dag_linear_all_completed() {
        let d = sample_dag();
        let run = run_dag(&d).await.unwrap();
        assert!(run.is_success());
        assert_eq!(run.completed_count(), 3);
        assert_eq!(run.failed_count(), 0);
    }

    #[tokio::test]
    async fn run_dag_diamond_all_completed() {
        let toml = r#"
name: "diamond"
tasks:
  - name: "a"
    command: "a"
  - name: "b"
    command: "b"
    depends_on: ["a"]
  - name: "c"
    command: "c"
    depends_on: ["a"]
  - name: "d"
    command: "d"
    depends_on: ["b", "c"]
"#;
        let d = load_dag_from_str(toml).unwrap();
        let run = run_dag(&d).await.unwrap();
        assert!(run.is_success());
        assert_eq!(run.completed_count(), 4);
    }

    #[test]
    fn dag_run_count_helpers() {
        let d = sample_dag();
        let run = DagRun::new(&d);
        assert_eq!(run.total_count(), 3);
        assert_eq!(run.completed_count(), 0);
        assert_eq!(run.failed_count(), 0);
        assert!(!run.is_complete());
    }

    #[test]
    fn dag_run_save_load_roundtrip() {
        let d = sample_dag();
        let mut run = DagRun::new(&d);
        run.get_mut("fetch").unwrap().status = TaskStatus::Completed;
        run.get_mut("fetch").unwrap().output = Some("done".to_string());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dag_run.json");
        let json = serde_json::to_string_pretty(&run).unwrap();
        std::fs::write(&path, json).unwrap();

        let loaded: DagRun =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(run, loaded);
    }
}
