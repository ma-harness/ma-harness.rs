//! ma_harness_cordis — 元框架 (Cordis-rs)
//!
//! **内部 crate** (2026-08-18 锁定). API 频繁变, 改它不要走 ADR.
//! 插件作者**不**直接 use 这个 crate, 走 [`ma_harness_seam`](../ma_harness_seam/index.html) 抽象层.
//!
//! Week 1 Day 1-5 实现: 完整 Cordis-rs (Context / Service / Plugin / typed key /
//! listener / disposable / scope / fork / dispose).
//!
//! 设计见 `docs/ma-harness-arch-map.md` §2.

#![deny(unsafe_code)]
#![warn(missing_docs)]

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
pub use error::CordisError;
pub use event::{Event as CordisEvent, EventSeverity};
pub use key::CtxKey;
pub use listener::{Listener, ListenerEvent, ListenerRegistry};
pub use plugin::Plugin;
pub use service::Service;

pub use std::any::TypeId; // 公开 re-export, 方便插件作者写 typed key
