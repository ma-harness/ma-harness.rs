//! ma_harness_seam — 插件抽象层
//!
//! **公开 crate** (2026-08-18 锁定). 插件作者 use 这个, 不直接 use `ma_harness_cordis` (内部).
//! 改一次要走 ADR. API 标记 `#[non_exhaustive]` 预留扩展空间.
//!
//! 详细设计见 `docs/ma-harness-arch-map.md` §3 (Seam 类型) + `docs/macro-design.md` (5 个 proc-macro).

#![deny(unsafe_code)]
#![warn(missing_docs)]

// Week 2-3 加, 公开:
// - `Plugin` 抽象 (跟 cordis 的 Plugin 解耦, 提供更高层 API)
// - `Tool` / `Listener` / `Handler` / `Service` / `Command` trait
// - `ToolRegistry` / `AdapterRegistry` / `CommandRegistry`
// - re-export 5 个 proc-macro (来自 ma_harness_plugin_macro)
