//! # 命名约定
//!
//! **Package name** ([Cargo.toml] / [crates.io]): `ma-harness-core`
//! **Crate ident** (`use` 路径): `ma_harness_core`
//!
//! Rust 自动从 kebab-case package name 推 snake_case crate ident.
//! 跟 `tokio-util` / `async-trait` / `crc32fast` 等生态完全一致.
//!
//! # 用法
//!
//! ```toml
//! [dependencies]
//! ma-harness-core = "0.1"
//! ```
//!
//! ```ignore
//! use ma_harness_core::*;
//! ```
//!
//! [Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
//! [crates.io]: https://crates.io/crates/ma-harness-core
//!
//! ma_harness_core — 核心: SessionEvent / 日志 / agent loop 骨架
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变.
//!
//! Week 1 Day 6-8 实现: SessionEvent 类型 + rusqlite append-only 日志 + agent loop 骨架
//! Week 2 完整: tool_call 循环 + multi-iteration + model adapter OpenAI 实现
//!
//! 设计见 `docs/ma-harness-arch-map.md` §4 (SessionEvent) + §5 (Operating Mode).

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(missing_docs)] // 2026-08-18: 内部 crate, 暂不强制 doc (Phase 2 release 前补)

pub mod agent;
pub mod event;
pub mod log;
pub mod tool;

pub use agent::{AgentLoop, AgentRunRequest, AgentRunResponse, FinishReason, ModelAdapter, ModelMessage, ModelRequest, ModelResponse, StubModelAdapter};
pub use event::{EventType, SessionEvent, Severity};
pub use log::{EventLog, EventQuery, EventPage, StoredEvent};
pub use tool::{ToolEntry, ToolInvokeFn, ToolRegistry, ToolSchema};
