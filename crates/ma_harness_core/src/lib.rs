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

pub mod agent;
pub mod event;
pub mod log;
pub mod tool;

pub use agent::{AgentLoop, AgentRunRequest, AgentRunResponse, FinishReason, ModelAdapter, ModelMessage, ModelRequest, ModelResponse, StubModelAdapter};
pub use event::{EventType, SessionEvent, Severity};
pub use log::{EventLog, EventQuery, EventPage, StoredEvent};
pub use tool::{ToolEntry, ToolInvokeFn, ToolRegistry, ToolSchema};
