//! Conformance runner: 跑 fixture, 收集实际事件。
//!
//! 算法见 `docs/conformance-design.md` § 4。

use crate::compare::{CompareEngine, CompareResult};
use crate::fixture::{Fixture, FixtureEvent};
use crate::report::ReportSummary;
use ma_harness_core::event::SessionEvent;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

/// 单个 fixture 跑出来的结果。
#[derive(Debug, Clone)]
pub struct ConformanceResult {
    /// Fixture 名
    pub fixture_name: String,
    /// 比对结果
    pub compare: CompareResult,
    /// 跑了多久 (ms)
    pub duration_ms: u64,
    /// 错误信息 (Runner 异常, 不是 compare diff)
    pub error: Option<String>,
    /// 跑出的实际事件 (debug 用)
    pub actual_events: Vec<FixtureEvent>,
}

impl ConformanceResult {
    /// 通过 (compare.passed=true, 无 runner error)
    pub fn is_pass(&self) -> bool {
        self.compare.passed && self.error.is_none()
    }
}

/// Runner 内部统计 (供 ReportSummary 用)。
#[derive(Debug, Clone, Default)]
pub struct RunnerStats {
    /// 总数
    pub total: usize,
    /// 通过
    pub passed: usize,
    /// 失败
    pub failed: usize,
    /// Runner 异常 (跟 compare 失败分开)
    pub errored: usize,
    /// 总耗时 ms
    pub total_duration_ms: u64,
}

impl RunnerStats {
    /// 从结果列表汇总
    pub fn from_results(results: &[ConformanceResult]) -> Self {
        let mut stats = Self::default();
        stats.total = results.len();
        for r in results {
            if r.error.is_some() {
                stats.errored += 1;
            } else if r.is_pass() {
                stats.passed += 1;
            } else {
                stats.failed += 1;
            }
            stats.total_duration_ms += r.duration_ms;
        }
        stats
    }

    /// 转为 ReportSummary
    pub fn to_summary(&self) -> ReportSummary {
        let pass_rate = if self.total == 0 {
            1.0
        } else {
            self.passed as f64 / self.total as f64
        };
        ReportSummary {
            total: self.total,
            passed: self.passed,
            failed: self.failed,
            errored: self.errored,
            pass_rate,
            total_duration_ms: self.total_duration_ms,
        }
    }
}

/// Conformance runner。
///
/// 主要 API:
/// - [`ConformanceRunner::new`] — 默认配置
/// - [`ConformanceRunner::with_plugin_dir`] — 显式指定 plugin 目录
/// - [`ConformanceRunner::run_fixture`] — 跑单个 fixture
/// - [`ConformanceRunner::run_all`] — 跑一组 fixture
pub struct ConformanceRunner {
    /// Plugin 目录 (None = 不装载 plugin)
    plugin_dir: Option<PathBuf>,
    /// 是否在跑时打印 debug log
    verbose: bool,
}

impl Default for ConformanceRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ConformanceRunner {
    /// 默认 runner (不指定 plugin dir, fixture 走自己的 plugin name 列表)
    pub fn new() -> Self {
        Self {
            plugin_dir: None,
            verbose: false,
        }
    }

    /// 显式指定 plugin 目录
    pub fn with_plugin_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.plugin_dir = Some(dir.into());
        self
    }

    /// 打开 verbose
    pub fn verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// 跑单个 fixture。
    ///
    /// 步骤:
    /// 1. 创建新 ctx
    /// 2. (TODO) 装载 fixture.input.plugins 指定的 plugin
    /// 3. 准备 event log (用于 collect 实际事件)
    /// 4. 顺序 emit fixture.input.events
    /// 5. 收集实际事件
    /// 6. compare 实际 vs fixture.output.events
    pub fn run_fixture(&self, fixture: &Fixture) -> ConformanceResult {
        let start = Instant::now();
        debug!(fixture = %fixture.name, "running fixture");

        // 步骤 1: 创建新 ctx
        let ctx = match self.build_ctx(fixture) {
            Ok(ctx) => ctx,
            Err(e) => {
                return ConformanceResult {
                    fixture_name: fixture.name.clone(),
                    compare: CompareResult::ok(0, fixture.output.events.len()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("ctx build failed: {e}")),
                    actual_events: Vec::new(),
                };
            }
        };

        // 步骤 2-3: 实际 emit
        let actual_events = self.replay_events(&ctx, &fixture.input.events);
        let duration_ms = start.elapsed().as_millis() as u64;

        if self.verbose {
            info!(
                fixture = %fixture.name,
                actual_count = actual_events.len(),
                expected_count = fixture.output.events.len(),
                "replay done"
            );
        }

        // 步骤 4: compare
        let compare = CompareEngine::compare(&actual_events, &fixture.output.events);

        ConformanceResult {
            fixture_name: fixture.name.clone(),
            compare,
            duration_ms,
            error: None,
            actual_events,
        }
    }

    /// 跑一组 fixture, 顺序跑。
    pub fn run_all(&self, fixtures: &[Fixture]) -> Vec<ConformanceResult> {
        let mut results = Vec::with_capacity(fixtures.len());
        for fixture in fixtures {
            let r = self.run_fixture(fixture);
            if !r.is_pass() {
                warn!(fixture = %fixture.name, "fixture failed");
            }
            results.push(r);
        }
        results
    }

    /// 汇总结果 → ReportSummary
    pub fn build_summary(&self, results: &[ConformanceResult]) -> ReportSummary {
        RunnerStats::from_results(results).to_summary()
    }

    // ---------- 私有 ----------

    /// 构建 ctx (目前是简化版, plugin 装载留给 Phase 2)
    fn build_ctx(&self, _fixture: &Fixture) -> Result<Arc<ma_harness_cordis::Context>, RunnerError> {
        // Phase 1: 不真装载 plugin, 留 fixture.input.plugins 字段给未来
        // Phase 2: 用 plugin_dir + fixture.input.plugins 真装载
        Ok(Arc::new(ma_harness_cordis::Context::new()))
    }

    /// 重放事件到 ctx, 收集实际事件。
    ///
    /// Phase 1 简化版:
    /// - 把每个 input event 直接转成 FixtureEvent 收集 (不动 ctx)
    /// - 业务方关心的"实际产出"由 fixture.output 描述
    /// - 真正的 ctx.emit / ctx.service 调用留给 plugin integration test
    ///
    /// Phase 2: 用 ma_harness_core::log::EventLog 收集, 跑 ctx.emit 触发 listener。
    fn replay_events(
        &self,
        _ctx: &Arc<ma_harness_cordis::Context>,
        input_events: &[FixtureEvent],
    ) -> Vec<FixtureEvent> {
        // Phase 1: 透传 input 事件 (假设 fixture 期望的 = fixture 输入)
        // 这样 framework 自身可以验证 compare 引擎正确
        input_events.to_vec()
    }
}

/// Runner 错误。
#[derive(Debug, Error)]
pub enum RunnerError {
    /// Plugin 装载失败
    #[error("plugin load failed: {0}")]
    PluginLoad(String),
    /// Ctx 初始化失败
    #[error("ctx init failed: {0}")]
    CtxInit(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{FixtureCategory, FixtureInput, FixtureOutput};
    use crate::fixture::ExpectedEvent;
    use std::collections::BTreeMap;

    fn sample_fixture() -> Fixture {
        Fixture {
            name: "sample".to_string(),
            category: FixtureCategory::ToolCall,
            description: Some("sample for test".to_string()),
            input: FixtureInput {
                session_id: "s1".to_string(),
                plugins: vec!["hello".to_string()],
                events: vec![FixtureEvent {
                    event_type: "ToolCall".to_string(),
                    payload: serde_json::json!({"tool": "bash", "args": {"command": "echo hi"}}),
                    timestamp_ms: None,
                }],
            },
            output: FixtureOutput {
                events: vec![ExpectedEvent {
                    event_type: "ToolCall".to_string(),
                    payload_match: BTreeMap::new(),
                    timestamp_ms: None,
                }],
                final_state: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn runner_runs_passing_fixture() {
        let runner = ConformanceRunner::new();
        let f = sample_fixture();
        let r = runner.run_fixture(&f);
        assert!(r.is_pass(), "error={:?} diffs={:?}", r.error, r.compare.diffs);
        assert_eq!(r.actual_events.len(), 1);
        assert_eq!(r.fixture_name, "sample");
    }

    #[test]
    fn runner_returns_stats() {
        let runner = ConformanceRunner::new();
        let f = sample_fixture();
        let results = runner.run_all(&[f.clone(), f.clone(), f]);
        let stats = RunnerStats::from_results(&results);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.passed, 3);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.errored, 0);
        let summary = stats.to_summary();
        assert_eq!(summary.total, 3);
        assert!((summary.pass_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn runner_collects_all_results() {
        let runner = ConformanceRunner::new();
        let results = runner.run_all(&[sample_fixture()]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].fixture_name, "sample");
    }
}
