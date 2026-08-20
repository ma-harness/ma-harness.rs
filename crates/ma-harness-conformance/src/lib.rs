//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-conformance`
//! **Crate ident** (`use` 路径): `ma_harness_conformance`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-conformance = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_conformance::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-conformance
//!
//! `ma_harness_conformance` — Conformance test framework for ma-harness.
//!
//! ## 目的
//!
//! 验证 ma-harness 在相同 trace 输入下, 跟 DeepSeek Harness (dsh) 产生语义等价的输出。
//!
//! ## 用法
//!
//! ```ignore
//! use ma_harness_conformance::{FixtureLoader, ConformanceRunner};
//!
//! let fixtures = FixtureLoader::from_jsonl("fixtures/dsh/basic.jsonl").unwrap();
//! let runner = ConformanceRunner::new();
//! let results = runner.run_all(&fixtures);
//! ```
//!
//! 完整 report 流程见 `docs/conformance-design.md`.
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
#![allow(missing_docs)] // 2026-08-18: 内部 crate, 暂不强制 doc (Phase 2 release 前补)
#![warn(unused_must_use)]

pub mod compare;
pub mod convert;
pub mod dsh_format;
pub mod fixture;
pub mod report;
pub mod runner;

pub use fixture::{
    Fixture, FixtureCategory, FixtureInput, FixtureOutput, FixtureEvent, FixtureLoader,
    FixtureError,
};
pub use runner::{ConformanceRunner, ConformanceResult, RunnerError, RunnerStats};
pub use compare::{CompareEngine, Diff, CompareResult, CompareError};
pub use report::{ConformanceReport, ReportFormat, ReportSummary, ReportWriter, ReportError};
pub use convert::{
    event_type_from_str, event_type_to_str, fixture_to_session, session_to_fixture, ConvertError,
};
pub use dsh_format::{
    DshFixture, DshInput, DshMessage, DshEvent, DshExpectedOutput, dsh_to_fixture, parse_dsh_jsonl,
    DshError,
};

// 重新导出 ma-harness 公开类型, 业务方不用自己引
pub use ma_harness_cordis;
pub use ma_harness_core;
