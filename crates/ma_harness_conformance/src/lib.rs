//! `ma_harness_conformance` — Conformance test framework for ma-harness.
//!
//! ## 目的
//!
//! 验证 ma-harness 在相同 trace 输入下, 跟 DeepSeek Harness (dsh) 产生语义等价的输出。
//!
//! ## 用法
//!
//! ```no_run
//! use ma_harness_conformance::{FixtureLoader, ConformanceRunner};
//!
//! # async fn run() {
//! let fixtures = FixtureLoader::from_jsonl("fixtures/dsh/basic.jsonl").unwrap();
//! let runner = ConformanceRunner::new();
//! let results = runner.run_all(&fixtures).await;
//! let report = runner.build_report(results);
//! report.write_markdown("target/conformance.md").unwrap();
//! # }
//! ```
//!
//! ## 设计
//!
//! 见 `docs/conformance-design.md` (Week 10 设计稿)。
//!
//! ## 模块
//!
//! - [`fixture`] — Fixture schema (JSONL) + 加载器
//! - [`runner`] — 跑 fixture, 收集实际事件
//! - [`compare`] — 比对实际 vs 期望, 产出 diff
//! - [`report`] — 汇总 pass/fail, 输出 markdown + json
//!
//! ## 不在 scope
//!
//! - 真实 model adapter (用 stub)
//! - 持久化层 (Phase 2)
//! - 跨进程 (server vs cli) — 只测 in-process

#![warn(missing_docs)]
#![warn(unused_must_use)]

pub mod fixture;
pub mod runner;
pub mod compare;
pub mod report;
pub mod convert;

pub use fixture::{Fixture, FixtureCategory, FixtureInput, FixtureOutput, FixtureEvent, FixtureLoader, FixtureError};
pub use runner::{ConformanceRunner, ConformanceResult, RunnerError, RunnerStats};
pub use compare::{CompareEngine, Diff, CompareResult, CompareError};
pub use report::{ConformanceReport, ReportFormat, ReportSummary, ReportWriter, ReportError};
pub use convert::{event_type_from_str, event_type_to_str, fixture_to_session, session_to_fixture, ConvertError};

// 重新导出 ma-harness 公开类型, 业务方不用自己引
pub use ma_harness_cordis;
pub use ma_harness_core;
