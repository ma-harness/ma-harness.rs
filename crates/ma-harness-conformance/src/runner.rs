//! Conformance runner: 跑 fixture, 收集实际事件。
//!
//! 算法见 `docs/conformance-design.md` § 4。

use crate::compare::{CompareEngine, CompareResult};
use crate::convert::{fixture_to_session, session_to_fixture};
use crate::fixture::{Fixture, FixtureEvent};
use crate::report::ReportSummary;
use ma_harness_core::log::{EventLog, EventQuery};
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
    /// Fixture 是否声明 `expect_fail: true` (negative test, 期望 comparer 报 diff)
    pub expect_fail: bool,
}

impl ConformanceResult {
    /// 通过 (compare.passed=true, 无 runner error)
    ///
    /// 这是**纯 comparer 判定** (没考虑 expect_fail 翻转)。
    /// 报告/统计用 [`is_pass_expected`] 走"用户期望"判定。
    pub fn is_pass(&self) -> bool {
        self.compare.passed && self.error.is_none()
    }

    /// 用户视角是否通过:
    /// - 默认 fixture (`expect_fail=false`): comparer 不报 diff = pass
    /// - 翻转 fixture (`expect_fail=true`): comparer 报 diff 才算 pass
    ///   (测"comparer 能否抓 mismatch", 由 fixture 作者保证 compare 真的 fail)
    pub fn is_pass_expected(&self) -> bool {
        if self.expect_fail {
            !self.is_pass() && self.error.is_none()
        } else {
            self.is_pass()
        }
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
    ///
    /// `passed` / `failed` 走**用户期望**判定 ([`ConformanceResult::is_pass_expected`]):
    /// - `expect_fail=false` fixture: comparer 没报 diff 才算 pass
    /// - `expect_fail=true` fixture: comparer 报 diff 才算 pass (negative test)
    pub fn from_results(results: &[ConformanceResult]) -> Self {
        // clippy 提示: field assignment outside initializer, 用 struct update syntax
        let mut stats = Self {
            total: results.len(),
            ..Self::default()
        };
        for r in results {
            if r.error.is_some() {
                stats.errored += 1;
            } else if r.is_pass_expected() {
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
        debug!(fixture = %fixture.name, expect_fail = fixture.expect_fail, "running fixture");

        // 步骤 1: 创建新 ctx (Phase 2 会用, 现在保留 hook)
        let _ctx = match self.build_ctx(fixture) {
            Ok(ctx) => ctx,
            Err(e) => {
                return ConformanceResult {
                    fixture_name: fixture.name.clone(),
                    compare: CompareResult::ok(0, fixture.output.events.len()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("ctx build failed: {e}")),
                    actual_events: Vec::new(),
                    expect_fail: fixture.expect_fail,
                };
            }
        };

        // 步骤 2-3: 实际 emit (Phase 2: 用 EventLog 真落库)
        let actual_events = match self.replay_events_via_event_log(fixture) {
            Ok(events) => events,
            Err(e) => {
                return ConformanceResult {
                    fixture_name: fixture.name.clone(),
                    compare: CompareResult::ok(0, fixture.output.events.len()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("event log replay failed: {e}")),
                    actual_events: Vec::new(),
                    expect_fail: fixture.expect_fail,
                };
            }
        };
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
            expect_fail: fixture.expect_fail,
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
    fn build_ctx(
        &self,
        _fixture: &Fixture,
    ) -> Result<Arc<ma_harness_cordis::Context>, RunnerError> {
        // Phase 1: 不真装载 plugin, 留 fixture.input.plugins 字段给未来
        // Phase 2: 用 plugin_dir + fixture.input.plugins 真装载
        Ok(Arc::new(ma_harness_cordis::Context::new()))
    }

    /// 重放事件到 EventLog (Phase 2: 真装载).
    ///
    /// 步骤:
    /// 1. 开 in-memory EventLog
    /// 2. 对每个 input event:
    ///    - 转 SessionEvent (via convert)
    ///    - 追加到 EventLog
    /// 3. 从 EventLog 读回所有事件
    /// 4. 转回 FixtureEvent (via convert) 供 compare
    fn replay_events_via_event_log(
        &self,
        fixture: &Fixture,
    ) -> Result<Vec<FixtureEvent>, RunnerError> {
        let log = EventLog::open_in_memory()
            .map_err(|e| RunnerError::EventLog(format!("open_in_memory failed: {e}")))?;

        for input_event in &fixture.input.events {
            let session_event = fixture_to_session(&fixture.input.session_id, input_event)
                .map_err(|e| RunnerError::Convert(e.to_string()))?;
            let seq = log.append(session_event);
            if self.verbose {
                debug!(seq, "appended event");
            }
        }

        let page = log
            .query(&EventQuery {
                session_id: fixture.input.session_id.clone(),
                ..Default::default()
            })
            .map_err(|e| RunnerError::EventLog(format!("query failed: {e}")))?;

        let actual: Vec<FixtureEvent> = page
            .events
            .iter()
            .map(|s| session_to_fixture(&s.event))
            .collect();

        Ok(actual)
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
    /// EventLog 操作失败
    #[error("event log error: {0}")]
    EventLog(String),
    /// 事件转换失败
    #[error("event convert error: {0}")]
    Convert(String),
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
            expect_fail: false,
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
        assert!(
            r.is_pass(),
            "error={:?} diffs={:?}",
            r.error,
            r.compare.diffs
        );
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

    /// 构造一个多事件 fixture, 验证 EventLog 真实路径
    fn multi_event_fixture() -> Fixture {
        Fixture {
            name: "multi_event".to_string(),
            category: FixtureCategory::AgentRun,
            description: Some("Run lifecycle with one tool call".to_string()),
            expect_fail: false,
            input: FixtureInput {
                session_id: "session-multi".to_string(),
                plugins: vec!["bash".to_string()],
                events: vec![
                    FixtureEvent {
                        event_type: "RunStart".to_string(),
                        payload: serde_json::json!({"model": "stub"}),
                        timestamp_ms: None,
                    },
                    FixtureEvent {
                        event_type: "ToolCall".to_string(),
                        payload: serde_json::json!({"tool": "bash", "args": {"command": "echo hi"}}),
                        timestamp_ms: None,
                    },
                    FixtureEvent {
                        event_type: "ToolResult".to_string(),
                        payload: serde_json::json!({"tool": "bash", "result": "hi\n"}),
                        timestamp_ms: None,
                    },
                    FixtureEvent {
                        event_type: "RunEnd".to_string(),
                        payload: serde_json::json!({"status": "ok"}),
                        timestamp_ms: None,
                    },
                ],
            },
            output: FixtureOutput {
                events: vec![
                    ExpectedEvent {
                        event_type: "RunStart".to_string(),
                        payload_match: BTreeMap::new(),
                        timestamp_ms: None,
                    },
                    ExpectedEvent {
                        event_type: "ToolCall".to_string(),
                        payload_match: BTreeMap::new(),
                        timestamp_ms: None,
                    },
                    ExpectedEvent {
                        event_type: "ToolResult".to_string(),
                        payload_match: BTreeMap::new(),
                        timestamp_ms: None,
                    },
                    ExpectedEvent {
                        event_type: "RunEnd".to_string(),
                        payload_match: BTreeMap::new(),
                        timestamp_ms: None,
                    },
                ],
                final_state: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn runner_via_event_log_preserves_event_order() {
        let runner = ConformanceRunner::new();
        let f = multi_event_fixture();
        let r = runner.run_fixture(&f);
        assert!(
            r.is_pass(),
            "error={:?} diffs={:?}",
            r.error,
            r.compare.diffs
        );
        // 4 个事件按 input 顺序
        assert_eq!(r.actual_events.len(), 4);
        assert_eq!(r.actual_events[0].event_type, "RunStart");
        assert_eq!(r.actual_events[1].event_type, "ToolCall");
        assert_eq!(r.actual_events[2].event_type, "ToolResult");
        assert_eq!(r.actual_events[3].event_type, "RunEnd");
    }

    #[test]
    fn runner_via_event_log_preserves_payload() {
        let runner = ConformanceRunner::new();
        let f = multi_event_fixture();
        let r = runner.run_fixture(&f);
        // ToolCall payload 完整保留
        let tool_call = &r.actual_events[1];
        assert_eq!(tool_call.payload["tool"], "bash");
        assert_eq!(tool_call.payload["args"]["command"], "echo hi");
    }

    /// 期望数量 < 实际数量, 应该报 "extra event"
    #[test]
    fn runner_detects_extra_event() {
        let runner = ConformanceRunner::new();
        let f = multi_event_fixture();
        // 把 expected 删一个 (RunEnd 期望空)
        let mut f = f;
        f.output.events.pop();
        let r = runner.run_fixture(&f);
        assert!(!r.is_pass());
        // 应该有 1 个 diff: ExtraEvent at index 3
        assert_eq!(r.compare.diffs.len(), 1);
        assert!(r.compare.diffs[0].summary().contains("extra event"));
        assert!(r.compare.diffs[0].summary().contains("RunEnd"));
    }

    /// 构造一个 by-design fail fixture: input 2 events, expected 3 events
    /// (期望 RunStart + RunEnd + ToolResult, 实际只 emit RunStart + RunEnd)
    fn expect_fail_fixture() -> Fixture {
        let mut f = sample_fixture();
        f.name = "by_design_fail".to_string();
        f.description = Some("by-design fail fixture".to_string());
        f.input = FixtureInput {
            session_id: "by-design".to_string(),
            plugins: vec!["bash".to_string()],
            events: vec![
                FixtureEvent {
                    event_type: "RunStart".to_string(),
                    payload: serde_json::json!({"prompt": "test"}),
                    timestamp_ms: None,
                },
                FixtureEvent {
                    event_type: "RunEnd".to_string(),
                    payload: serde_json::json!({"status": "ok"}),
                    timestamp_ms: None,
                },
            ],
        };
        f.output = FixtureOutput {
            events: vec![
                ExpectedEvent {
                    event_type: "RunStart".to_string(),
                    payload_match: BTreeMap::new(),
                    timestamp_ms: None,
                },
                ExpectedEvent {
                    event_type: "RunEnd".to_string(),
                    payload_match: BTreeMap::new(),
                    timestamp_ms: None,
                },
                ExpectedEvent {
                    event_type: "ToolResult".to_string(),
                    payload_match: BTreeMap::new(),
                    timestamp_ms: None,
                },
            ],
            final_state: BTreeMap::new(),
        };
        f.expect_fail = true;
        f
    }

    /// expect_fail fixture + comparer 报 diff: 用户视角 pass (翻转)
    #[test]
    fn runner_expect_fail_flip_to_pass_when_diff_present() {
        let runner = ConformanceRunner::new();
        let f = expect_fail_fixture();
        let r = runner.run_fixture(&f);
        // comparer 视角: fail (input 2 events vs expected 3)
        assert!(!r.is_pass(), "comparer should report diff");
        assert_eq!(r.compare.diffs.len(), 1);
        // 用户视角: pass (翻转)
        assert!(r.is_pass_expected(), "expect_fail should flip to pass");
        assert!(r.expect_fail);
    }

    /// expect_fail fixture + comparer 没报 diff: 用户视角 fail (翻转过来反而不通过)
    /// 这种"假阴性"说明 fixture 作者标 expect_fail 但实际 comparer 没抓到。
    #[test]
    fn runner_expect_fail_flag_without_diff_is_user_fail() {
        // 复用 sample_fixture (实际 comparer 会 pass), 强行 expect_fail=true
        let runner = ConformanceRunner::new();
        let mut f = sample_fixture();
        f.expect_fail = true;
        let r = runner.run_fixture(&f);
        // comparer 视角: pass
        assert!(r.is_pass());
        // 用户视角: fail (期望 comparer 报 diff 但没报)
        assert!(!r.is_pass_expected());
    }

    /// RunnerStats 走 `is_pass_expected` 翻转: expect_fail fixture 在 stats 里算 passed
    #[test]
    fn runner_stats_flip_expect_fail_to_passed() {
        let runner = ConformanceRunner::new();
        let f1 = sample_fixture(); // expect_fail=false, comparer 报 pass
        let f2 = expect_fail_fixture(); // expect_fail=true, comparer 报 fail
        let results = runner.run_all(&[f1, f2]);
        let stats = RunnerStats::from_results(&results);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.passed, 2, "both should be user-pass");
        assert_eq!(stats.failed, 0);
    }
}
