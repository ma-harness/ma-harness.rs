//! Conformance 报告: 汇总 + 写 markdown / json。
//!
//! 模板见 `docs/conformance-design.md` § 6。

use crate::compare::Diff;
use crate::runner::ConformanceResult;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// 报告汇总数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    /// 总 fixture 数
    pub total: usize,
    /// 通过
    pub passed: usize,
    /// 失败 (compare diff)
    pub failed: usize,
    /// 异常 (runner 错误, 跟 compare 分开)
    pub errored: usize,
    /// 通过率 (0.0 - 1.0)
    pub pass_rate: f64,
    /// 总耗时 ms
    pub total_duration_ms: u64,
}

impl ReportSummary {
    /// 是否达到 ≥ 95% 通过率 (Week 11 报告指标)
    pub fn meets_target(&self) -> bool {
        self.pass_rate >= 0.95
    }
}

/// 报告格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    /// Markdown (人类看)
    Markdown,
    /// JSON (机器读)
    Json,
}

/// 一份完整 conformance 报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceReport {
    /// 报告生成时间 (RFC3339)
    pub generated_at: String,
    /// 汇总
    pub summary: ReportSummary,
    /// 每个 fixture 的结果
    pub results: Vec<ConformanceResultSerde>,
}

/// 序列化的 ConformanceResult (独立 type 避免暴露内部字段)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceResultSerde {
    pub fixture_name: String,
    pub passed: bool,
    pub diffs: Vec<Diff>,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub actual_count: usize,
    pub expected_count: usize,
}

impl From<&ConformanceResult> for ConformanceResultSerde {
    fn from(r: &ConformanceResult) -> Self {
        Self {
            fixture_name: r.fixture_name.clone(),
            passed: r.is_pass(),
            diffs: r.compare.diffs.clone(),
            duration_ms: r.duration_ms,
            error: r.error.clone(),
            actual_count: r.compare.actual_count,
            expected_count: r.compare.expected_count,
        }
    }
}

/// 报告写入器。
pub struct ReportWriter;

impl ReportWriter {
    /// 从 ConformanceResult 列表 + ReportSummary 构建报告。
    pub fn build(results: &[ConformanceResult], summary: ReportSummary) -> ConformanceReport {
        ConformanceReport {
            generated_at: chrono::Utc::now().to_rfc3339(),
            summary,
            results: results.iter().map(ConformanceResultSerde::from).collect(),
        }
    }

    /// 写 markdown 报告。
    pub fn write_markdown(
        report: &ConformanceReport,
        path: impl AsRef<Path>,
    ) -> Result<(), ReportError> {
        let mut out = String::new();
        out.push_str(&Self::render_markdown(report));
        std::fs::write(path, out)?;
        Ok(())
    }

    /// 写 json 报告。
    pub fn write_json(
        report: &ConformanceReport,
        path: impl AsRef<Path>,
    ) -> Result<(), ReportError> {
        let json = serde_json::to_string_pretty(report)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// 渲染 markdown (不写文件, 给 stdout / log 用)
    pub fn render_markdown(report: &ConformanceReport) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        let _ = writeln!(out, "# Conformance Report — {}\n", report.generated_at);
        let _ = writeln!(
            out,
            "**Pass rate**: {} / {} = {:.1}% {}\n",
            report.summary.passed,
            report.summary.total,
            report.summary.pass_rate * 100.0,
            if report.summary.meets_target() { "✅ (target ≥ 95%)" } else { "❌ (target ≥ 95%)" }
        );
        let _ = writeln!(
            out,
            "**Duration**: {} ms\n",
            report.summary.total_duration_ms
        );
        let _ = writeln!(
            out,
            "- Total: {}\n- Passed: {}\n- Failed: {}\n- Errored: {}\n",
            report.summary.total,
            report.summary.passed,
            report.summary.failed,
            report.summary.errored
        );

        // 失败 + 异常段
        let failed: Vec<_> = report
            .results
            .iter()
            .filter(|r| !r.passed)
            .collect();

        if !failed.is_empty() {
            let _ = writeln!(out, "## Failed fixtures ({})\n", failed.len());
            for r in &failed {
                let _ = writeln!(out, "### {}\n", r.fixture_name);
                if let Some(err) = &r.error {
                    let _ = writeln!(out, "- **Runner error**: {}\n", err);
                }
                if r.diffs.is_empty() {
                    let _ = writeln!(out, "- (no diffs, error in runner)\n");
                } else {
                    let _ = writeln!(out, "**Diffs** ({}):\n", r.diffs.len());
                    for d in &r.diffs {
                        let _ = writeln!(out, "- {}\n", d.summary());
                    }
                }
            }
        }

        // 通过段
        let passed: Vec<_> = report.results.iter().filter(|r| r.passed).collect();
        if !passed.is_empty() {
            let _ = writeln!(out, "\n## Passed fixtures ({})\n", passed.len());
            for r in &passed {
                let _ = writeln!(
                    out,
                    "- ✅ {} ({} events, {} ms)\n",
                    r.fixture_name, r.actual_count, r.duration_ms
                );
            }
        }

        out
    }
}

/// 报告错误。
#[derive(Debug, Error)]
pub enum ReportError {
    /// IO 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 序列化错误
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::CompareResult;

    fn make_result(name: &str, passed: bool, diffs: Vec<Diff>) -> ConformanceResult {
        ConformanceResult {
            fixture_name: name.to_string(),
            compare: if passed {
                CompareResult::ok(2, 2)
            } else {
                CompareResult::failed(diffs, 2, 2)
            },
            duration_ms: 10,
            error: None,
            actual_events: vec![],
        }
    }

    #[test]
    fn summary_meets_target_at_95_percent() {
        let s = ReportSummary {
            total: 20,
            passed: 19,
            failed: 1,
            errored: 0,
            pass_rate: 0.95,
            total_duration_ms: 100,
        };
        assert!(s.meets_target());
    }

    #[test]
    fn summary_fails_below_95_percent() {
        let s = ReportSummary {
            total: 20,
            passed: 18,
            failed: 2,
            errored: 0,
            pass_rate: 0.90,
            total_duration_ms: 100,
        };
        assert!(!s.meets_target());
    }

    #[test]
    fn summary_handles_zero_total() {
        let s = ReportSummary {
            total: 0,
            passed: 0,
            failed: 0,
            errored: 0,
            pass_rate: 1.0,
            total_duration_ms: 0,
        };
        assert!(s.meets_target()); // 空 = 100% pass rate (vacuous truth)
    }

    #[test]
    fn report_writer_builds_from_results() {
        let results = vec![
            make_result("a", true, vec![]),
            make_result(
                "b",
                false,
                vec![Diff::TypeMismatch {
                    index: 0,
                    expected_type: "X".to_string(),
                    actual_type: "Y".to_string(),
                }],
            ),
        ];
        let summary = ReportSummary {
            total: 2,
            passed: 1,
            failed: 1,
            errored: 0,
            pass_rate: 0.5,
            total_duration_ms: 20,
        };
        let report = ReportWriter::build(&results, summary);
        assert_eq!(report.results.len(), 2);
        assert!(!report.summary.meets_target());
    }

    #[test]
    fn report_renders_markdown() {
        let results = vec![
            make_result("pass_one", true, vec![]),
            make_result(
                "fail_one",
                false,
                vec![Diff::MissingField {
                    index: 1,
                    key: "result".to_string(),
                }],
            ),
        ];
        let summary = ReportSummary {
            total: 2,
            passed: 1,
            failed: 1,
            errored: 0,
            pass_rate: 0.5,
            total_duration_ms: 50,
        };
        let report = ReportWriter::build(&results, summary);
        let md = ReportWriter::render_markdown(&report);
        assert!(md.contains("# Conformance Report"));
        assert!(md.contains("pass_one"));
        assert!(md.contains("fail_one"));
        // Diff::summary 输出 "[#1] missing field: result" (lowercase m)
        assert!(md.contains("missing field"));
        assert!(md.contains("result"));
    }
}
