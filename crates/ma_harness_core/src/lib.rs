//! ma_harness_core — 核心: SessionEvent / 日志 / agent loop 骨架
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变.
//!
//! Week 1 Day 6-7 实现: SessionEvent 类型 + rusqlite append-only 日志
//! Week 1 Day 8 扩展: agent loop 骨架
//! Week 2 完整: model adapter 集成 + tool registry + end-to-end Default 模式
//!
//! 设计见 `docs/ma-harness-arch-map.md` §4 (SessionEvent) + §5 (Operating Mode).

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod event;
pub mod log;

pub use event::{EventType, SessionEvent, Severity};
pub use log::{EventLog, EventQuery, EventPage};
