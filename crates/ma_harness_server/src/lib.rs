//! ma_harness_server — 服务层
//!
//! **内部 crate** (2026-08-18 锁定). axum + tonic 拼装, 频繁变.
//! Week 7-9 起, 把 `ma_harness_seam` 的 5 个 registry 暴露成 gRPC service + HTTP endpoint.

#![deny(unsafe_code)]
#![warn(missing_docs)]
