//! ma_harness_core — 核心: agent loop / session / event
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变.
//! Week 1-2 计划: agent loop 骨架 + session ID 生成 + 事件 emit 抽象.
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §4 (SessionEvent) + §5 (Operating Mode).

#![deny(unsafe_code)]
#![warn(missing_docs)]

// 占位阶段. Week 1 Day 6-7 加 rusqlite + SessionEvent.

/// 占位类型
pub struct Placeholder;
