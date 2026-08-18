//! ma_harness_cordis — 元框架 (Cordis-rs)
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变, 改它不要走 ADR.
//! 插件作者**不**直接 use 这个 crate, 走 [`ma_harness_seam`](../ma_harness_seam/index.html) 抽象层.
//!
//! Week 1 Day 1-2 实现: 最小可用 (Context / Service / Plugin / typed key).
//! Week 1 Day 5 扩展: listener / command / disposable.
//! Week 2 完整: fork / dispose / 事件总线.
//!
//! 设计见 `docs/ma-harness-arch-map.md` §2.
//! Spec 阶段, 正在实现中. 占位阶段已结束.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod context;
mod error;
mod event;
mod key;
mod plugin;
mod service;

pub use context::Context;
pub use error::CordisError;
pub use event::{Event, EventSeverity};
pub use key::CtxKey;
pub use plugin::Plugin;
pub use service::Service;

pub use std::any::TypeId; // 公开 re-export, 方便插件作者写 typed key
