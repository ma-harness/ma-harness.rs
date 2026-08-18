//! ma_harness_cordis — 元框架 (Cordis-rs)
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变, 改它不要走 ADR.
//! 插件作者**不**直接 use 这个 crate, 走 [`ma_harness_seam`](../ma_harness_seam/index.html) 抽象层.
//!
//! Week 1 Day 1-5 实现: 完整 Cordis-rs (Context / Service / Plugin / typed key /
//! listener / disposable / scope / fork / dispose).
//!
//! 设计见 `docs/ma-harness-arch-map.md` §2.

#![warn(unsafe_code)] // 2026-08-18: 从 deny 降级到 warn, 允许 context.rs 用 unsafe 延长 lifetime
#![warn(missing_docs)]
// 2026-08-18: 删除 #![feature(associated_type_defaults)], stable 不支持
// Service impl 必须显式 `type Ctx = Context;` (见 decision-log §3 改动原因)

mod context;
mod disposable;
mod error;
mod event;
mod key;
mod listener;
mod plugin;
mod service;

pub use context::Context;
pub use disposable::{Disposable, Scope};
pub use error::{BoxedError, CordisError};
pub use event::{Event as CordisEvent, EventSeverity};
pub use key::{is_snake_case, CtxKey}; // 2026-08-18: is_snake_case 公开, 给 ctx_key! macro 用
pub use listener::{Listener, ListenerEvent}; // ListenerRegistry 是 pub(crate), 不再 export
pub use plugin::Plugin;
pub use service::Service;

pub use std::any::TypeId; // 公开 re-export, 方便插件作者写 typed key
